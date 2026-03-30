mod config;
mod db;
mod fetcher;
mod logging;
mod models;
mod parser;
mod scheduler;

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;

use crate::config::{AppConfig, CliArgs};
use crate::db::Database;
use crate::fetcher::Fetcher;
use crate::models::SearchResult;
use crate::parser::Parser as HtmlParser;
use crate::scheduler::Crawler;

#[tokio::main]
async fn main() -> Result<()> {
    logging::init()?;
    let cli = CliArgs::parse();
    tracing::info!("Starting crawler CLI");

    let config = Arc::new(AppConfig::from_args(&cli)?);
    tracing::info!(?config, "Configuration loaded");

    let db = Database::connect(&config.database_url).await?;
    if let Some(query) = cli.search.as_deref() {
        run_search(&db, query, cli.search_limit).await?;
        return Ok(());
    }

    db.enqueue_seeds(&config.seed_urls, config.default_priority)
        .await?;
    tracing::info!(seeds = config.seed_urls.len(), "Queue seeded");

    let fetcher = Arc::new(Fetcher::from_config(&config)?);
    let parser = HtmlParser::new();

    let crawler = Crawler::new(config, db, fetcher, parser);
    crawler.run().await
}

async fn run_search(db: &Database, query: &str, limit: u32) -> Result<()> {
    tracing::info!(%query, limit, "Executing search query");
    let hits = db.search(query, limit).await?;
    if hits.is_empty() {
        println!("Nenhum resultado para: {query}");
    } else {
        for (
            idx,
            SearchResult {
                url,
                title,
                snippet,
                lang,
                score,
            },
        ) in hits.iter().enumerate()
        {
            let title = title.as_deref().unwrap_or("Untitled");
            let snippet = snippet.as_deref().unwrap_or("(sem snippet)");
            let lang = lang.as_deref().unwrap_or("??");
            println!(
                "{index}. {title} [{lang}]\n   Score: {score:.2}\n   {snippet}\n   {url}\n",
                index = idx + 1,
                title = title,
                lang = lang,
                score = score,
                snippet = snippet,
                url = url
            );
        }
    }
    Ok(())
}
