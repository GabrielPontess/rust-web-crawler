use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::broadcast::{self, Receiver, Sender};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CrawlEventKind {
    Started,
    Succeeded,
    Failed,
    Retrying,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CrawlEvent {
    pub kind: CrawlEventKind,
    pub url: String,
    pub host: Option<String>,
    pub message: Option<String>,
    pub duration_ms: Option<u64>,
    pub attempts: Option<i64>,
    pub timestamp: DateTime<Utc>,
}

impl CrawlEvent {
    pub fn started(url: &str, host: Option<String>) -> Self {
        Self {
            kind: CrawlEventKind::Started,
            url: url.to_string(),
            host,
            message: None,
            duration_ms: None,
            attempts: None,
            timestamp: Utc::now(),
        }
    }

    pub fn succeeded(url: &str, host: Option<String>, duration_ms: u64) -> Self {
        Self {
            kind: CrawlEventKind::Succeeded,
            url: url.to_string(),
            host,
            message: None,
            duration_ms: Some(duration_ms),
            attempts: None,
            timestamp: Utc::now(),
        }
    }

    pub fn failed(url: &str, host: Option<String>, message: String) -> Self {
        Self {
            kind: CrawlEventKind::Failed,
            url: url.to_string(),
            host,
            message: Some(message),
            duration_ms: None,
            attempts: None,
            timestamp: Utc::now(),
        }
    }

    pub fn retrying(url: &str, host: Option<String>, attempts: i64, message: String) -> Self {
        Self {
            kind: CrawlEventKind::Retrying,
            url: url.to_string(),
            host,
            message: Some(message),
            duration_ms: None,
            attempts: Some(attempts),
            timestamp: Utc::now(),
        }
    }
}

#[derive(Clone)]
pub struct EventBus {
    sender: Arc<Sender<CrawlEvent>>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self {
            sender: Arc::new(tx),
        }
    }

    pub fn subscribe(&self) -> Receiver<CrawlEvent> {
        self.sender.subscribe()
    }

    pub fn emit(&self, event: CrawlEvent) {
        let _ = self.sender.send(event);
    }
}
