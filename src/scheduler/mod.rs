use anyhow::{Context, Result};
use tracing::{error, info, warn};
use url::Url;

use crate::config::AppConfig;
use crate::db::Database;
use crate::fetcher::Fetcher;
use crate::parser::Parser;

pub struct Crawler {
    config: AppConfig,
    db: Database,
    fetcher: Fetcher,
    parser: Parser,
}

impl Crawler {
    pub fn new(config: AppConfig, db: Database, fetcher: Fetcher, parser: Parser) -> Self {
        Self {
            config,
            db,
            fetcher,
            parser,
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Crawler event loop started");
        loop {
            if let Some(url_str) = self.db.next_ready().await? {
                let url = Url::parse(&url_str).context("invalid url stored in queue")?;
                info!(%url_str, "Processing URL");

                self.db.mark_processing(&url_str).await?;

                match self.fetcher.fetch(&url).await {
                    Ok(html) => match self.parser.parse(&url, &html) {
                        Ok(page_record) => {
                            if let Err(err) = self
                                .db
                                .store_page(&page_record, self.config.default_priority)
                                .await
                            {
                                error!(url = %page_record.url, error = %err, "Failed to store page");
                                self.handle_failure(&url_str, err.to_string()).await?;
                            } else {
                                info!(url = %page_record.url, "Page stored successfully");
                            }
                        }
                        Err(err) => {
                            warn!(url = %url_str, error = %err, "Parsing failed");
                            self.handle_failure(&url_str, err.to_string()).await?;
                        }
                    },
                    Err(err) => {
                        warn!(url = %url_str, error = %err, "Fetching failed");
                        self.handle_failure(&url_str, err.to_string()).await?;
                    }
                }
            } else {
                info!("No more URLs in queue. Finished.");
                break;
            }
        }

        info!("Crawler event loop completed");
        Ok(())
    }

    async fn handle_failure(&self, url: &str, error: String) -> Result<()> {
        if !self
            .db
            .schedule_retry(
                url,
                self.config.retry_max_attempts,
                self.config.retry_backoff_secs,
                &error,
            )
            .await?
        {
            warn!(%url, "URL reached retry limit; marked as failed");
        }
        Ok(())
    }
}
