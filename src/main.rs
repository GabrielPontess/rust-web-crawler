mod config;
mod db;
mod fetcher;
mod logging;
mod models;
mod parser;
mod scheduler;

use anyhow::Result;

use crate::config::AppConfig;
use crate::db::Database;
use crate::fetcher::Fetcher;
use crate::parser::Parser;
use crate::scheduler::Crawler;

#[tokio::main]
async fn main() -> Result<()> {
    logging::init()?;
    tracing::info!("Starting crawler CLI");

    let config = AppConfig::from_cli()?;
    tracing::info!(?config, "Configuration loaded");

    let db = Database::connect(&config.database_url).await?;
    db.enqueue_seeds(&config.seed_urls, config.default_priority)
        .await?;
    tracing::info!(seeds = config.seed_urls.len(), "Queue seeded");

    let fetcher = Fetcher::from_config(&config)?;
    let parser = Parser::new();

    let crawler = Crawler::new(config, db, fetcher, parser);
    crawler.run().await
}
