use std::str::FromStr;
use std::time::Duration;

use anyhow::Result;
use chrono::{Duration as ChronoDuration, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Executor, Pool, Row, Sqlite, Transaction};
use tracing::{debug, info, warn};
use url::Url;

use crate::models::PageRecord;

pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let options = SqliteConnectOptions::from_str(database_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal);

        info!(%database_url, "Connecting to SQLite database");

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(3))
            .connect_with(options)
            .await?;

        info!("Running database migrations");
        sqlx::migrate!("./migrations").run(&pool).await?;
        info!("Migrations applied");
        Self::ensure_schema(&pool).await?;

        info!("Database ready");
        Ok(Self { pool })
    }

    pub async fn enqueue_seeds(&self, seeds: &[Url], priority: i32) -> Result<()> {
        if !seeds.is_empty() {
            info!(count = seeds.len(), "Seeding initial URLs");
        }
        for url in seeds {
            self.enqueue_url(url.as_str(), priority).await?;
        }
        Ok(())
    }

    async fn enqueue_url(&self, url: &str, priority: i32) -> Result<()> {
        debug!(%url, priority, "Enqueueing URL");
        Self::upsert_queue_entry(&self.pool, url, priority).await
    }

    pub async fn next_ready(&self) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT url FROM queue
             WHERE status = 'pending'
               AND (next_run_at IS NULL OR next_run_at <= CURRENT_TIMESTAMP)
               AND (host IS NULL OR host NOT IN (
                    SELECT host FROM queue WHERE status = 'processing' AND host IS NOT NULL
               ))
             ORDER BY priority DESC, rowid
             LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some((ref url,)) = row {
            debug!(%url, "Dequeued next ready URL");
        }

        Ok(row.map(|(url,)| url))
    }

    pub async fn mark_processing(&self, url: &str) -> Result<()> {
        debug!(%url, "Marking URL as processing");
        sqlx::query(
            "UPDATE queue SET status = 'processing', next_run_at = CURRENT_TIMESTAMP WHERE url = ?",
        )
        .bind(url)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn schedule_retry(
        &self,
        url: &str,
        max_attempts: u32,
        backoff_secs: u64,
        error: &str,
    ) -> Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as("SELECT attempts FROM queue WHERE url = ?")
            .bind(url)
            .fetch_optional(&self.pool)
            .await?;

        let Some((attempts,)) = row else {
            return Ok(false);
        };

        let next_attempt = attempts + 1;
        if next_attempt as u32 > max_attempts {
            warn!(%url, attempts = next_attempt, "Retry limit reached; marking as failed");
            self.mark_failed(url, Some(error)).await?;
            return Ok(false);
        }

        let exponent = if next_attempt > 0 {
            (next_attempt - 1) as u32
        } else {
            0
        };
        let shift = exponent.min(20);
        let multiplier = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
        let delay = backoff_secs.saturating_mul(multiplier);
        let next_run_at = Utc::now() + ChronoDuration::seconds(delay as i64);

        sqlx::query(
            "UPDATE queue SET attempts = ?, last_error = ?, next_run_at = ?, status = 'pending' WHERE url = ?",
        )
        .bind(next_attempt)
        .bind(error)
        .bind(next_run_at)
        .bind(url)
        .execute(&self.pool)
        .await?;

        info!(%url, attempts = next_attempt, delay_secs = delay, "Retry scheduled");
        Ok(true)
    }

    pub async fn mark_failed(&self, url: &str, error: Option<&str>) -> Result<()> {
        info!(%url, "Marking URL as failed");
        sqlx::query(
            "UPDATE queue SET status = 'failed', last_error = ?, next_run_at = NULL WHERE url = ?",
        )
        .bind(error)
        .bind(url)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn store_page(&self, record: &PageRecord, default_priority: i32) -> Result<()> {
        info!(url = %record.url, links = record.links.len(), "Persisting page");
        let mut tx = self.pool.begin().await?;
        self.insert_page(&mut tx, record).await?;
        self.insert_links(&mut tx, &record.links, default_priority)
            .await?;
        self.mark_completed(&mut tx, &record.url).await?;
        tx.commit().await?;
        Ok(())
    }

    fn host_from_url(url: &str) -> Option<String> {
        Url::parse(url).ok().and_then(|u| {
            u.host_str().map(|host| {
                if let Some(port) = u.port() {
                    format!("{host}:{port}")
                } else {
                    host.to_string()
                }
            })
        })
    }

    async fn insert_page(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        record: &PageRecord,
    ) -> Result<()> {
        let headings_text = Self::headings_to_text(&record.headings);
        sqlx::query(
            "INSERT INTO pages (url, title, description, headings, content, summary, lang)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(url) DO UPDATE SET
                title=excluded.title,
                description=excluded.description,
                headings=excluded.headings,
                content=excluded.content,
                summary=excluded.summary,
                lang=excluded.lang,
                crawled_at=CURRENT_TIMESTAMP",
        )
        .bind(&record.url)
        .bind(&record.title)
        .bind(&record.description)
        .bind(&headings_text)
        .bind(&record.content)
        .bind(&record.summary)
        .bind(&record.language)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    fn headings_to_text(headings: &[String]) -> Option<String> {
        if headings.is_empty() {
            None
        } else {
            Some(headings.join("\n"))
        }
    }

    async fn insert_links(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        links: &[String],
        priority: i32,
    ) -> Result<()> {
        for link in links {
            debug!(%link, "Queueing discovered link");
            if Self::host_from_url(link).is_none() {
                warn!(%link, "Skipping invalid link without host");
                continue;
            }
            Self::upsert_queue_entry(&mut **tx, link, priority).await?;
        }
        Ok(())
    }

    async fn mark_completed(&self, tx: &mut Transaction<'_, Sqlite>, url: &str) -> Result<()> {
        info!(%url, "Marking URL as completed");
        sqlx::query(
            "UPDATE queue SET status = 'completed', attempts = 0, last_error = NULL, next_run_at = NULL WHERE url = ?",
        )
        .bind(url)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn upsert_queue_entry<'c, E>(executor: E, url: &str, priority: i32) -> Result<()>
    where
        E: Executor<'c, Database = Sqlite>,
    {
        let host = Self::host_from_url(url);
        sqlx::query(
            "INSERT INTO queue (url, host, priority) VALUES (?, ?, ?)
             ON CONFLICT(url) DO UPDATE SET
                priority = MAX(queue.priority, excluded.priority),
                host = COALESCE(queue.host, excluded.host)",
        )
        .bind(url)
        .bind(host)
        .bind(priority)
        .execute(executor)
        .await?;
        Ok(())
    }
}

impl Database {
    async fn ensure_schema(pool: &Pool<Sqlite>) -> Result<()> {
        info!("Ensuring tables and columns exist");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS pages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                url TEXT UNIQUE NOT NULL,
                title TEXT,
                description TEXT,
                headings TEXT,
                content TEXT,
                summary TEXT,
                lang TEXT,
                crawled_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS queue (
                url TEXT PRIMARY KEY,
                status TEXT NOT NULL DEFAULT 'pending',
                priority INTEGER NOT NULL DEFAULT 0,
                attempts INTEGER NOT NULL DEFAULT 0,
                last_error TEXT,
                next_run_at DATETIME,
                host TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .execute(pool)
        .await?;

        Self::ensure_column(pool, "queue", "priority", "INTEGER NOT NULL DEFAULT 0").await?;
        Self::ensure_column(pool, "queue", "attempts", "INTEGER NOT NULL DEFAULT 0").await?;
        Self::ensure_column(pool, "queue", "last_error", "TEXT").await?;
        Self::ensure_column(pool, "queue", "next_run_at", "DATETIME").await?;
        Self::ensure_column(pool, "queue", "host", "TEXT").await?;
        Self::ensure_column(
            pool,
            "queue",
            "created_at",
            "DATETIME DEFAULT CURRENT_TIMESTAMP",
        )
        .await?;
        Self::ensure_column(pool, "pages", "description", "TEXT").await?;
        Self::ensure_column(pool, "pages", "headings", "TEXT").await?;
        Self::ensure_column(pool, "pages", "summary", "TEXT").await?;
        Self::ensure_column(pool, "pages", "lang", "TEXT").await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_queue_status_priority ON queue (status, priority DESC, next_run_at)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_queue_host_status ON queue (host, status)")
            .execute(pool)
            .await?;

        Ok(())
    }

    async fn ensure_column(
        pool: &Pool<Sqlite>,
        table: &str,
        column: &str,
        definition: &str,
    ) -> Result<()> {
        if !Self::column_exists(pool, table, column).await? {
            let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
            sqlx::query(&sql).execute(pool).await?;
            info!(table, column, "Added missing column");
        }
        Ok(())
    }

    async fn column_exists(pool: &Pool<Sqlite>, table: &str, column: &str) -> Result<bool> {
        let sql = format!("PRAGMA table_info({table})");
        let rows = sqlx::query(&sql).fetch_all(pool).await?;
        Ok(rows.iter().any(|row| {
            row.try_get::<String, _>("name")
                .map(|n| n == column)
                .unwrap_or(false)
        }))
    }
}
