mod config;
mod db;
mod events;
mod fetcher;
mod logging;
mod models;
mod parser;
mod scheduler;
mod server;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use tokio::time::sleep;

use crate::config::AppConfig;
use crate::db::Database;
use crate::events::{CrawlEvent, CrawlEventKind, EventBus};
use crate::fetcher::Fetcher;
use crate::models::SearchResult;
use crate::parser::Parser as HtmlParser;
use crate::scheduler::Crawler;
use crate::server::run_server;

#[derive(Parser)]
#[command(name = "crawler", version, about = "Personal search crawler")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Crawl(CrawlArgs),
    Serve(ServeArgs),
}

#[derive(Args)]
struct CrawlArgs {
    #[arg(long, default_value = "appsettings.json")]
    config: String,
    #[arg(long)]
    search: Option<String>,
    #[arg(long, default_value_t = 10)]
    search_limit: u32,
}

#[derive(Args)]
struct ServeArgs {
    #[arg(long, default_value = "appsettings.json")]
    config: String,
    #[arg(long, default_value = "127.0.0.1:8080")]
    addr: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<()> {
    logging::init()?;
    let cli = Cli::parse();
    match cli.command {
        Commands::Crawl(args) => run_crawl(args).await,
        Commands::Serve(args) => run_serve(args).await,
    }
}

async fn run_crawl(args: CrawlArgs) -> Result<()> {
    let config = Arc::new(AppConfig::from_file_path(&args.config)?);
    tracing::info!(?config, "Configuration loaded");
    let db = Database::connect(&config.database_url, config.database_max_connections).await?;
    if let Some(query) = args.search {
        run_search(&db, &query, args.search_limit).await?;
        return Ok(());
    }

    db.enqueue_seeds(&config.seed_urls, config.default_priority)
        .await?;
    tracing::info!(seeds = config.seed_urls.len(), "Queue seeded");

    let fetcher = Arc::new(Fetcher::from_config(&config)?);
    let parser = HtmlParser::new();
    let crawler = Crawler::new(config, db, fetcher, parser, None);
    crawler.run().await
}

async fn run_serve(args: ServeArgs) -> Result<()> {
    let config = Arc::new(AppConfig::from_file_path(&args.config)?);
    tracing::info!(?config, "Configuration loaded (serve mode)");
    let db = Database::connect(&config.database_url, config.database_max_connections).await?;
    let events = EventBus::new(1024);
    spawn_event_poller(db.clone(), events.clone());
    run_server(config, db, Some(events), args.addr).await
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

fn spawn_event_poller(db: Database, events: EventBus) {
    tokio::spawn(async move {
        let mut last_id = 0i64;
        loop {
            match db.events_after(last_id, 100).await {
                Ok(new_events) => {
                    if new_events.is_empty() {
                        sleep(Duration::from_secs(1)).await;
                    } else {
                        for log in new_events {
                            last_id = last_id.max(log.id);
                            if let Some(event) = convert_log_to_event(&log) {
                                events.emit(event);
                            }
                        }
                    }
                }
                Err(err) => {
                    tracing::error!(error = %err, "Event poller failed");
                    sleep(Duration::from_secs(2)).await;
                }
            }
        }
    });
}

fn convert_log_to_event(log: &crate::models::EventLog) -> Option<CrawlEvent> {
    let kind = match log.event_type.as_str() {
        "started" => CrawlEventKind::Started,
        "succeeded" => CrawlEventKind::Succeeded,
        "failed" => CrawlEventKind::Failed,
        "retrying" => CrawlEventKind::Retrying,
        _ => return None,
    };

    Some(CrawlEvent {
        kind,
        url: log.url.clone(),
        host: log.host.clone(),
        message: log.message.clone(),
        duration_ms: log.duration_ms.map(|v| v as u64),
        attempts: log.attempts,
        timestamp: log.created_at,
    })
}
