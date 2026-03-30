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
use crate::events::{CrawlEvent, EventBus};
use crate::fetcher::Fetcher;
use crate::parser::Parser;

pub struct Crawler {
    config: Arc<AppConfig>,
    db: Database,
    fetcher: Arc<Fetcher>,
    parser: Parser,
    events: Option<EventBus>,
}

impl Crawler {
    pub fn new(
        config: Arc<AppConfig>,
        db: Database,
        fetcher: Arc<Fetcher>,
        parser: Parser,
        events: Option<EventBus>,
    ) -> Self {
        Self {
            config,
            db,
            fetcher,
            parser,
            events,
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Crawler event loop started");
        let start = Instant::now();
        let global_limit = self.config.max_concurrency.max(1);
        let global_sem = Arc::new(Semaphore::new(global_limit));
        let host_limiter = Arc::new(HostLimiter::new(self.config.max_host_parallelism));
        let metrics = Arc::new(CrawlMetrics::default());
        let mut tasks = JoinSet::new();

        loop {
            let permit = global_sem.clone().acquire_owned().await?;
            match self.db.next_ready().await? {
                Some(url_str) => {
                    self.db.mark_processing(&url_str).await?;
                    let db = self.db.clone();
                    let fetcher = self.fetcher.clone();
                    let parser = self.parser;
                    let config = self.config.clone();
                    let events = self.events.clone();
                    let host_limiter = host_limiter.clone();
                    let metrics = metrics.clone();

                    tasks.spawn(async move {
                        if let Err(err) = Self::process_url(
                            db,
                            fetcher,
                            parser,
                            config,
                            events,
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

                    if tasks.len() >= global_limit {
                        if let Some(result) = tasks.join_next().await {
                            result??;
                        }
                    }
                }
                None => {
                    drop(permit);
                    if let Some(result) = tasks.join_next().await {
                        result??;
                    } else {
                        break;
                    }
                }
            }
        }

        while let Some(result) = tasks.join_next().await {
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
        events: Option<EventBus>,
        host_limiter: Arc<HostLimiter>,
        metrics: Arc<CrawlMetrics>,
        url_str: String,
        _permit: OwnedSemaphorePermit,
    ) -> Result<()> {
        let url = Url::parse(&url_str).context("invalid url stored in queue")?;
        let host = host_with_port(&url);
        let _host_permit = if let Some(ref host_key) = host {
            Some(host_limiter.acquire(host_key).await?)
        } else {
            None
        };

        let start = Instant::now();
        metrics.inc_started();
        let started_event = CrawlEvent::started(&url_str, host.clone());
        emit_event(&events, started_event.clone());
        let _ = db.log_event(&started_event).await;

        match fetcher.fetch(&url).await {
            Ok(html) => match parser.parse(&url, &html) {
                Ok(page_record) => {
                    if let Err(err) = db.store_page(&page_record, config.default_priority).await {
                        metrics.inc_failed();
                        let message = err.to_string();
                        let fail_event =
                            CrawlEvent::failed(&url_str, host.clone(), message.clone());
                        emit_event(&events, fail_event.clone());
                        let _ = db.log_event(&fail_event).await;
                        Self::handle_failure(&db, &config, &url_str, message, &events).await?;
                    } else {
                        metrics.inc_succeeded();
                        let duration_ms = start.elapsed().as_millis() as u64;
                        let success_event =
                            CrawlEvent::succeeded(&url_str, host.clone(), duration_ms);
                        emit_event(&events, success_event.clone());
                        let _ = db.log_event(&success_event).await;
                        info!(url = %page_record.url, "Page stored successfully");
                    }
                }
                Err(err) => {
                    metrics.inc_failed();
                    let message = err.to_string();
                    let fail_event = CrawlEvent::failed(&url_str, host.clone(), message.clone());
                    emit_event(&events, fail_event.clone());
                    let _ = db.log_event(&fail_event).await;
                    warn!(url = %url_str, error = %message, "Parsing failed");
                    Self::handle_failure(&db, &config, &url_str, message, &events).await?;
                }
            },
            Err(err) => {
                metrics.inc_failed();
                let message = err.to_string();
                let fail_event = CrawlEvent::failed(&url_str, host.clone(), message.clone());
                emit_event(&events, fail_event.clone());
                let _ = db.log_event(&fail_event).await;
                warn!(url = %url_str, error = %message, "Fetching failed");
                Self::handle_failure(&db, &config, &url_str, message, &events).await?;
            }
        }

        Ok(())
    }

    async fn handle_failure(
        db: &Database,
        config: &AppConfig,
        url: &str,
        error: String,
        events: &Option<EventBus>,
    ) -> Result<()> {
        if let Some(attempts) = db
            .schedule_retry(
                url,
                config.retry_max_attempts,
                config.retry_backoff_secs,
                &error,
            )
            .await?
        {
            let retry_event = CrawlEvent::retrying(url, None, attempts, error.clone());
            emit_event(events, retry_event.clone());
            let _ = db.log_event(&retry_event).await;
        } else {
            warn!(%url, "URL reached retry limit; marked as failed");
        }
        Ok(())
    }
}

fn emit_event(bus: &Option<EventBus>, event: CrawlEvent) {
    if let Some(bus) = bus {
        bus.emit(event);
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
