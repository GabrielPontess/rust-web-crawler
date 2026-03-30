mod robots;

use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::Client;
use reqwest::header::CONTENT_TYPE;
use tokio::sync::Mutex;
use tokio::time::{Instant, sleep};
use tracing::{debug, info, warn};
use url::Url;

use crate::config::AppConfig;

use self::robots::{RobotsRules, parse_robots};

pub struct Fetcher {
    client: Client,
    timeout: Duration,
    user_agent: String,
    max_response_bytes: usize,
    allowed_content_types: Vec<String>,
    host_delay: Duration,
    robots_cache: Mutex<HashMap<String, RobotsRules>>,
    host_slots: Mutex<HashMap<String, Instant>>,
    global_slot: Mutex<Instant>,
    global_delay: Duration,
}

impl Fetcher {
    pub fn from_config(config: &AppConfig) -> Result<Self> {
        info!("Building HTTP client");
        let client = Client::builder()
            .user_agent(&config.user_agent)
            .timeout(config.request_timeout)
            .build()
            .context("failed to build http client")?;

        Ok(Self {
            client,
            timeout: config.request_timeout,
            user_agent: config.user_agent.clone(),
            max_response_bytes: config.max_response_bytes,
            allowed_content_types: config.allowed_content_types.clone(),
            host_delay: config.host_delay,
            robots_cache: Mutex::new(HashMap::new()),
            host_slots: Mutex::new(HashMap::new()),
            global_slot: Mutex::new(Instant::now()),
            global_delay: config.politeness_delay,
        })
    }

    pub async fn fetch(&self, url: &Url) -> Result<String> {
        self.ensure_supported_scheme(url)?;
        let host_key = host_key(url).context("URL missing host")?;

        self.wait_for_global_slot().await;
        self.wait_for_host_slot(&host_key).await;

        if !self.is_allowed_by_robots(url).await? {
            bail!("blocked by robots.txt: {}", url);
        }

        let response = self
            .client
            .get(url.as_str())
            .timeout(self.timeout)
            .send()
            .await?
            .error_for_status()?;

        self.ensure_content_type(&response)?;

        let body = self.read_limited_body(response).await?;

        if let Some(delay) = self.host_crawl_delay(&host_key).await {
            self.set_host_cooldown(&host_key, delay).await;
        } else {
            self.set_host_cooldown(&host_key, self.host_delay).await;
        }
        self.bump_global_slot().await;

        Ok(body)
    }

    fn ensure_supported_scheme(&self, url: &Url) -> Result<()> {
        if matches!(url.scheme(), "http" | "https") {
            Ok(())
        } else {
            bail!("unsupported scheme {}", url.scheme());
        }
    }

    async fn wait_for_global_slot(&self) {
        loop {
            let wait = {
                let mut guard = self.global_slot.lock().await;
                let now = Instant::now();
                if *guard > now {
                    Some(*guard - now)
                } else {
                    *guard = now;
                    None
                }
            };

            if let Some(duration) = wait {
                sleep(duration).await;
            } else {
                break;
            }
        }
    }

    async fn bump_global_slot(&self) {
        let mut guard = self.global_slot.lock().await;
        *guard = Instant::now() + self.global_delay;
    }

    async fn wait_for_host_slot(&self, host: &str) {
        loop {
            let wait = {
                let mut guard = self.host_slots.lock().await;
                let now = Instant::now();
                match guard.get(host).copied() {
                    Some(next) if next > now => Some(next - now),
                    _ => {
                        guard.insert(host.to_string(), now);
                        None
                    }
                }
            };

            if let Some(duration) = wait {
                sleep(duration).await;
            } else {
                break;
            }
        }
    }

    async fn set_host_cooldown(&self, host: &str, delay: Duration) {
        let mut guard = self.host_slots.lock().await;
        guard.insert(host.to_string(), Instant::now() + delay);
    }

    async fn is_allowed_by_robots(&self, url: &Url) -> Result<bool> {
        let key = match host_key(url) {
            Some(k) => k,
            None => return Ok(false),
        };

        if let Some(rules) = self.cached_rules(&key).await {
            return Ok(rules.allows(url.path()));
        }

        let robots_url = format!("{}://{}/robots.txt", url.scheme(), key_host_display(&key));
        debug!(%robots_url, "Fetching robots.txt");
        let rules = match self
            .client
            .get(&robots_url)
            .timeout(self.timeout)
            .send()
            .await
        {
            Ok(resp) => {
                if resp.status().is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    parse_robots(&text, &self.user_agent)
                } else {
                    RobotsRules::allow_all()
                }
            }
            Err(err) => {
                warn!(error = %err, host = %key, "Failed to fetch robots.txt; defaulting to allow");
                RobotsRules::allow_all()
            }
        };

        self.store_rules(&key, rules.clone()).await;
        Ok(rules.allows(url.path()))
    }

    async fn host_crawl_delay(&self, host: &str) -> Option<Duration> {
        self.cached_rules(host).await.and_then(|r| r.crawl_delay())
    }

    async fn cached_rules(&self, host: &str) -> Option<RobotsRules> {
        let guard = self.robots_cache.lock().await;
        guard.get(host).cloned()
    }

    async fn store_rules(&self, host: &str, rules: RobotsRules) {
        let mut guard = self.robots_cache.lock().await;
        guard.insert(host.to_string(), rules);
    }

    fn ensure_content_type(&self, response: &reqwest::Response) -> Result<()> {
        if let Some(header) = response.headers().get(CONTENT_TYPE) {
            let content_type = header.to_str().unwrap_or("").to_ascii_lowercase();
            if !self
                .allowed_content_types
                .iter()
                .any(|allowed| content_type.starts_with(allowed))
            {
                bail!("unsupported content-type: {content_type}");
            }
        }
        Ok(())
    }

    async fn read_limited_body(&self, response: reqwest::Response) -> Result<String> {
        let mut body = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk_result) = stream.next().await {
            let chunk: Bytes = chunk_result?;
            if body.len() + chunk.len() > self.max_response_bytes {
                bail!(
                    "response exceeds limit of {} bytes",
                    self.max_response_bytes
                );
            }
            body.extend_from_slice(&chunk);
        }

        let text = String::from_utf8_lossy(&body).to_string();
        Ok(text)
    }
}

fn host_key(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    let key = if let Some(port) = url.port() {
        format!("{host}:{port}")
    } else {
        host.to_string()
    };
    Some(key)
}

fn key_host_display(key: &str) -> &str {
    key
}
