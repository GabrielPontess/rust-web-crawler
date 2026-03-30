use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageRecord {
    pub url: String,
    pub title: String,
    pub description: Option<String>,
    pub headings: Vec<String>,
    pub content: String,
    pub summary: Option<String>,
    pub language: Option<String>,
    pub links: Vec<String>,
}

#[derive(Debug, Clone, FromRow, PartialEq, Serialize)]
pub struct SearchResult {
    pub url: String,
    pub title: Option<String>,
    pub snippet: Option<String>,
    pub lang: Option<String>,
    pub score: f64,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct QueueItem {
    pub url: String,
    pub status: String,
    pub priority: i64,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub host: Option<String>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct EventLog {
    pub id: i64,
    pub event_type: String,
    pub url: String,
    pub host: Option<String>,
    pub message: Option<String>,
    pub duration_ms: Option<i64>,
    pub attempts: Option<i64>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct PageSummary {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub headings: Option<String>,
    pub content: Option<String>,
    pub summary: Option<String>,
    pub lang: Option<String>,
    pub crawled_at: Option<DateTime<Utc>>,
}

impl PageRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        url: String,
        title: String,
        description: Option<String>,
        headings: Vec<String>,
        content: String,
        summary: Option<String>,
        language: Option<String>,
        links: Vec<String>,
    ) -> Self {
        Self {
            url,
            title,
            description,
            headings,
            content,
            summary,
            language,
            links,
        }
    }
}
