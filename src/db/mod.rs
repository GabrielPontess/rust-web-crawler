use std::str::FromStr;
use std::time::Duration;

use anyhow::Result;
use chrono::{Duration as ChronoDuration, Utc};
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use sqlx::{Error as SqlxError, Executor, Postgres, QueryBuilder, Transaction};
use tracing::{debug, info, warn};
use url::Url;

use crate::events::{CrawlEvent, CrawlEventKind};
use crate::models::{EventLog, PageRecord, PageSummary, QueueItem, SearchResult};

#[derive(Clone)]
pub struct Database {
    pool: PgPool,
}

impl Database {
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self> {
        let options = PgConnectOptions::from_str(database_url)?.application_name("crawler");
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect_with(options)
            .await?;

        info!(%database_url, max_connections, "Connected to Postgres");
        Self::ensure_schema(&pool).await?;
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
               AND (next_run_at IS NULL OR next_run_at <= NOW())
               AND (host IS NULL OR host NOT IN (
                    SELECT host FROM queue WHERE status = 'processing' AND host IS NOT NULL
               ))
             ORDER BY priority DESC, created_at
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
        sqlx::query("UPDATE queue SET status = 'processing', next_run_at = NOW() WHERE url = $1")
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
    ) -> Result<Option<i64>> {
        let row: Option<(i32,)> = sqlx::query_as("SELECT attempts FROM queue WHERE url = $1")
            .bind(url)
            .fetch_optional(&self.pool)
            .await?;

        let Some((attempts,)) = row else {
            return Ok(None);
        };

        let next_attempt = attempts + 1;
        if next_attempt as u32 > max_attempts {
            warn!(%url, attempts = next_attempt, "Retry limit reached; marking as failed");
            self.mark_failed(url, Some(error)).await?;
            return Ok(None);
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
            "UPDATE queue SET attempts = $1, last_error = $2, next_run_at = $3, status = 'pending' WHERE url = $4",
        )
        .bind(next_attempt)
        .bind(error)
        .bind(next_run_at)
        .bind(url)
        .execute(&self.pool)
        .await?;

        info!(%url, attempts = next_attempt, delay_secs = delay, "Retry scheduled");
        Ok(Some(next_attempt as i64))
    }

    pub async fn mark_failed(&self, url: &str, error: Option<&str>) -> Result<()> {
        info!(%url, "Marking URL as failed");
        sqlx::query("UPDATE queue SET status = 'failed', last_error = $1, next_run_at = NULL WHERE url = $2")
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
            u.host_str().map(|host| match u.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_string(),
            })
        })
    }

    async fn insert_page(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        record: &PageRecord,
    ) -> Result<()> {
        let headings_text = Self::headings_to_text(&record.headings);
        let search_blob = format!(
            "{} {} {} {} {}",
            record.title,
            record.description.as_deref().unwrap_or(""),
            headings_text.as_deref().unwrap_or(""),
            record.content,
            record.summary.as_deref().unwrap_or("")
        );

        sqlx::query(
            "INSERT INTO pages (url, title, description, headings, content, summary, lang, search_vector)
             VALUES ($1, $2, $3, $4, $5, $6, $7, to_tsvector('simple', $8))
             ON CONFLICT (url) DO UPDATE SET
                title = EXCLUDED.title,
                description = EXCLUDED.description,
                headings = EXCLUDED.headings,
                content = EXCLUDED.content,
                summary = EXCLUDED.summary,
                lang = EXCLUDED.lang,
                search_vector = to_tsvector('simple', $8),
                crawled_at = NOW()",
        )
        .bind(&record.url)
        .bind(&record.title)
        .bind(&record.description)
        .bind(&headings_text)
        .bind(&record.content)
        .bind(&record.summary)
        .bind(&record.language)
        .bind(&search_blob)
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
        tx: &mut Transaction<'_, Postgres>,
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

    async fn mark_completed(&self, tx: &mut Transaction<'_, Postgres>, url: &str) -> Result<()> {
        info!(%url, "Marking URL as completed");
        sqlx::query(
            "UPDATE queue SET status = 'completed', attempts = 0, last_error = NULL, next_run_at = NULL WHERE url = $1",
        )
        .bind(url)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    async fn upsert_queue_entry<'c, E>(executor: E, url: &str, priority: i32) -> Result<()>
    where
        E: Executor<'c, Database = Postgres>,
    {
        let host = Self::host_from_url(url);
        sqlx::query(
            "INSERT INTO queue (url, host, priority) VALUES ($1, $2, $3)
             ON CONFLICT (url) DO UPDATE SET
                priority = GREATEST(queue.priority, EXCLUDED.priority),
                host = COALESCE(queue.host, EXCLUDED.host)",
        )
        .bind(url)
        .bind(host)
        .bind(priority)
        .execute(executor)
        .await?;
        Ok(())
    }

    pub async fn log_event(&self, event: &CrawlEvent) -> Result<()> {
        let result = sqlx::query(
            "INSERT INTO crawl_events (event_type, url, host, message, duration_ms, attempts, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(event_type_label(&event.kind))
        .bind(&event.url)
        .bind(&event.host)
        .bind(&event.message)
        .bind(event.duration_ms.map(|v| v as i64))
        .bind(event.attempts)
        .bind(event.timestamp)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(err) if missing_table(&err, "crawl_events") => Ok(()),
            Err(err) => Err(err.into()),
        }
    }

    pub async fn recent_events(&self, limit: i64) -> Result<Vec<EventLog>> {
        match sqlx::query_as::<_, EventLog>(
            "SELECT id, event_type, url, host, message, duration_ms, attempts, created_at
             FROM crawl_events ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        {
            Ok(rows) => Ok(rows),
            Err(err) if missing_table(&err, "crawl_events") => Ok(Vec::new()),
            Err(err) => Err(err.into()),
        }
    }

    pub async fn events_after(&self, last_id: i64, limit: i64) -> Result<Vec<EventLog>> {
        match sqlx::query_as::<_, EventLog>(
            "SELECT id, event_type, url, host, message, duration_ms, attempts, created_at
             FROM crawl_events WHERE id > $1 ORDER BY id LIMIT $2",
        )
        .bind(last_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        {
            Ok(rows) => Ok(rows),
            Err(err) if missing_table(&err, "crawl_events") => Ok(Vec::new()),
            Err(err) => Err(err.into()),
        }
    }

    pub async fn queue_items(
        &self,
        status: Option<&str>,
        host: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<QueueItem>> {
        let mut builder = QueryBuilder::new(
            "SELECT url, status, priority, attempts, last_error, host, next_run_at, created_at FROM queue",
        );
        let mut where_used = false;
        if let Some(status) = status {
            builder.push(" WHERE status = ");
            builder.push_bind(status);
            where_used = true;
        }
        if let Some(host) = host {
            if where_used {
                builder.push(" AND host = ");
            } else {
                builder.push(" WHERE host = ");
            }
            builder.push_bind(host);
        }
        builder.push(" ORDER BY created_at DESC LIMIT ");
        builder.push_bind(limit);
        builder.push(" OFFSET ");
        builder.push_bind(offset);

        let rows = builder
            .build_query_as::<QueueItem>()
            .fetch_all(&self.pool)
            .await?;
        Ok(rows)
    }

    pub async fn page_detail(&self, url: &str) -> Result<Option<PageSummary>> {
        let row = sqlx::query_as::<_, PageSummary>(
            "SELECT url, title, description, headings, content, summary, lang, crawled_at FROM pages WHERE url = $1",
        )
        .bind(url)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn search(&self, query: &str, limit: u32) -> Result<Vec<SearchResult>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let limit = limit.clamp(1, 100) as i64;
        let rows = sqlx::query_as::<_, SearchResult>(
            "WITH q AS (SELECT plainto_tsquery('simple', $1) AS ts)
             SELECT url,
                    title,
                    ts_headline('simple', COALESCE(content, ''), q.ts) AS snippet,
                    lang,
                    ts_rank_cd(search_vector, q.ts) AS score
             FROM pages, q
             WHERE q.ts @@ search_vector
             ORDER BY score DESC
             LIMIT $2",
        )
        .bind(query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    async fn ensure_schema(pool: &PgPool) -> Result<()> {
        info!("Ensuring Postgres schema");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS pages (
                id BIGSERIAL PRIMARY KEY,
                url TEXT UNIQUE NOT NULL,
                title TEXT,
                description TEXT,
                headings TEXT,
                content TEXT,
                summary TEXT,
                lang TEXT,
                search_vector tsvector NOT NULL DEFAULT to_tsvector('simple', ''),
                crawled_at TIMESTAMPTZ DEFAULT NOW()
            )",
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
                next_run_at TIMESTAMPTZ,
                host TEXT,
                created_at TIMESTAMPTZ DEFAULT NOW()
            )",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS crawl_events (
                id BIGSERIAL PRIMARY KEY,
                event_type TEXT NOT NULL,
                url TEXT NOT NULL,
                host TEXT,
                message TEXT,
                duration_ms BIGINT,
                attempts BIGINT,
                created_at TIMESTAMPTZ DEFAULT NOW()
            )",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_queue_status_priority ON queue (status, priority DESC)",
        )
        .execute(pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_queue_host_status ON queue (host, status)")
            .execute(pool)
            .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_pages_search ON pages USING GIN (search_vector)",
        )
        .execute(pool)
        .await?;

        Ok(())
    }
}

fn event_type_label(kind: &CrawlEventKind) -> &'static str {
    match kind {
        CrawlEventKind::Started => "started",
        CrawlEventKind::Succeeded => "succeeded",
        CrawlEventKind::Failed => "failed",
        CrawlEventKind::Retrying => "retrying",
    }
}

fn missing_table(err: &SqlxError, table: &str) -> bool {
    matches!(
        err,
        SqlxError::Database(db_err) if db_err.message().contains(&format!("relation \"{}\"", table))
    )
}
