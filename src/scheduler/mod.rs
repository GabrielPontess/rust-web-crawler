use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;
use tokio::time::Instant;
use tracing::{error, info, warn};
use url::Url;

use crate::config::AppConfig;
use crate::db::Database;
use crate::fetcher::Fetcher;
use crate::parser::Parser;

pub struct Crawler {
    config: Arc<AppConfig>,
    db: Database,
    fetcher: Arc<Fetcher>,
    parser: Parser,
}

impl Crawler {
    pub fn new(
        config: Arc<AppConfig>,
        db: Database,
        fetcher: Arc<Fetcher>,
        parser: Parser,
    ) -> Self {
        Self {
            config,
            db,
            fetcher,
            parser,
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Crawler event loop started");
        let start = Instant::now();
        let concurrency = self.config.max_concurrency.max(1);
        let global_semaphore = Arc::new(Semaphore::new(concurrency));
        let host_limiter = Arc::new(HostLimiter::new(self.config.max_host_parallelism));
        let metrics = Arc::new(CrawlMetrics::default());
        let mut join_set = JoinSet::new();

        loop {
            let permit = global_semaphore.clone().acquire_owned().await?;

            match self.db.next_ready().await? {
                Some(url_str) => {
                    self.db.mark_processing(&url_str).await?;
                    let worker_db = self.db.clone();
                    let fetcher = self.fetcher.clone();
                    let parser = self.parser;
                    let config = self.config.clone();
                    let host_limiter = host_limiter.clone();
                    let metrics = metrics.clone();

                    join_set.spawn(async move {
                        if let Err(err) = Self::process_url(
                            worker_db,
                            fetcher,
                            parser,
                            config,
                            host_limiter,
                            metrics,
                            url_str,
                            permit,
                        )
                        .await
                        {
                            error!(error = %err, "Worker failed");
                            return Err(err);
                        }
                        Ok(())
                    });

                    if join_set.len() >= concurrency {
                        if let Some(result) = join_set.join_next().await {
                            result??;
                        }
                    }
                }
                None => {
                    drop(permit);
                    if let Some(result) = join_set.join_next().await {
                        result??;
                    } else {
                        break;
                    }
                }
            }
        }

        while let Some(result) = join_set.join_next().await {
            result??;
        }

        let elapsed = start.elapsed();
        let snapshot = metrics.snapshot();
        info!(
            elapsed_ms = elapsed.as_millis(),
            started = snapshot.started,
            succeeded = snapshot.succeeded,
            failed = snapshot.failed,
            "Crawler event loop completed"
        );
        Ok(())
    }

    async fn process_url(
        db: Database,
        fetcher: Arc<Fetcher>,
        parser: Parser,
        config: Arc<AppConfig>,
        host_limiter: Arc<HostLimiter>,
        metrics: Arc<CrawlMetrics>,
        url_str: String,
        _permit: OwnedSemaphorePermit,
    ) -> Result<()> {
        let url = Url::parse(&url_str).context("invalid url stored in queue")?;
        let host_key = host_with_port(&url);
        let _host_permit = if let Some(host) = host_key.as_deref() {
            Some(host_limiter.acquire(host).await?)
        } else {
            None
        };

        metrics.inc_started();

        match fetcher.fetch(&url).await {
            Ok(html) => match parser.parse(&url, &html) {
                Ok(page_record) => {
                    if let Err(err) = db.store_page(&page_record, config.default_priority).await {
                        metrics.inc_failed();
                        Self::handle_failure(&db, &config, &url_str, err.to_string()).await?;
                    } else {
                        metrics.inc_succeeded();
                        info!(url = %page_record.url, "Page stored successfully");
                    }
                }
                Err(err) => {
                    metrics.inc_failed();
                    warn!(url = %url_str, error = %err, "Parsing failed");
                    Self::handle_failure(&db, &config, &url_str, err.to_string()).await?;
                }
            },
            Err(err) => {
                metrics.inc_failed();
                warn!(url = %url_str, error = %err, "Fetching failed");
                Self::handle_failure(&db, &config, &url_str, err.to_string()).await?;
            }
        }

        Ok(())
    }

    async fn handle_failure(
        db: &Database,
        config: &AppConfig,
        url: &str,
        error: String,
    ) -> Result<()> {
        if !db
            .schedule_retry(
                url,
                config.retry_max_attempts,
                config.retry_backoff_secs,
                &error,
            )
            .await?
        {
            warn!(%url, "URL reached retry limit; marked as failed");
        }
        Ok(())
    }
}

fn host_with_port(url: &Url) -> Option<String> {
    url.host_str().map(|host| match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

struct HostLimiter {
    max_per_host: usize,
    inner: Mutex<HashMap<String, Arc<Semaphore>>>,
}

impl HostLimiter {
    fn new(max_per_host: usize) -> Self {
        Self {
            max_per_host: max_per_host.max(1),
            inner: Mutex::new(HashMap::new()),
        }
    }

    async fn acquire(&self, host: &str) -> Result<OwnedSemaphorePermit> {
        let semaphore = {
            let mut guard = self.inner.lock().await;
            guard
                .entry(host.to_string())
                .or_insert_with(|| Arc::new(Semaphore::new(self.max_per_host)))
                .clone()
        };
        Ok(semaphore.acquire_owned().await?)
    }
}

#[derive(Default)]
struct CrawlMetrics {
    started: AtomicUsize,
    succeeded: AtomicUsize,
    failed: AtomicUsize,
}

impl CrawlMetrics {
    fn inc_started(&self) {
        self.started.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_succeeded(&self) {
        self.succeeded.fetch_add(1, Ordering::Relaxed);
    }

    fn inc_failed(&self) {
        self.failed.fetch_add(1, Ordering::Relaxed);
    }

    fn snapshot(&self) -> CrawlStats {
        CrawlStats {
            started: self.started.load(Ordering::Relaxed),
            succeeded: self.succeeded.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug)]
struct CrawlStats {
    started: usize,
    succeeded: usize,
    failed: usize,
}
