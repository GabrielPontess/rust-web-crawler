mod cli;

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use tracing::info;
use url::Url;

pub use cli::CliArgs;

const DEFAULT_DATABASE_URL: &str = "sqlite:crawler.db";
const DEFAULT_SEED: &str = "https://www.rust-lang.org/";
const DEFAULT_USER_AGENT: &str = "RustyCrawlerMVP/0.1";
const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 10;
const DEFAULT_POLITENESS_DELAY_SECS: u64 = 1;
const DEFAULT_HOST_DELAY_SECS: u64 = 1;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS);
const DEFAULT_POLITENESS_DELAY: Duration = Duration::from_secs(DEFAULT_POLITENESS_DELAY_SECS);
const DEFAULT_HOST_DELAY: Duration = Duration::from_secs(DEFAULT_HOST_DELAY_SECS);
const DEFAULT_PRIORITY: i32 = 0;
const DEFAULT_RETRY_MAX_ATTEMPTS: u32 = 3;
const DEFAULT_RETRY_BACKOFF_SECS: u64 = 5;
const DEFAULT_MAX_RESPONSE_BYTES: usize = 2_000_000;
const DEFAULT_CONTENT_TYPES: [&str; 2] = ["text/html", "application/xhtml+xml"];

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub seed_urls: Vec<Url>,
    pub user_agent: String,
    pub request_timeout: Duration,
    pub politeness_delay: Duration,
    pub default_priority: i32,
    pub retry_max_attempts: u32,
    pub retry_backoff_secs: u64,
    pub host_delay: Duration,
    pub max_response_bytes: usize,
    pub allowed_content_types: Vec<String>,
}

impl AppConfig {
    pub fn from_args(args: &CliArgs) -> Result<Self> {
        Self::from_file_path(&args.config_path)
    }

    pub fn from_file_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        info!(config_file = %path.display(), "Loading configuration from JSON");
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Falha ao ler arquivo de configuração {}", path.display()))?;
        let raw: FileConfig = serde_json::from_str(&contents)
            .with_context(|| format!("JSON inválido em {}", path.display()))?;
        let cfg = Self::try_from(raw)?;
        Ok(cfg)
    }

    fn validate_duration(field: &str, duration: Duration) -> Result<()> {
        if duration.is_zero() {
            bail!("{field} deve ser maior que zero");
        }
        Ok(())
    }

    fn ensure_seeds(seed_values: Option<Vec<String>>) -> Result<Vec<Url>> {
        let values = seed_values.unwrap_or_default();
        let candidates = if values.is_empty() {
            vec![DEFAULT_SEED.to_string()]
        } else {
            values
        };

        let mut seeds = Vec::with_capacity(candidates.len());
        for seed in candidates {
            let url = Url::parse(&seed).with_context(|| format!("URL seed inválida: {seed}"))?;
            if !matches!(url.scheme(), "http" | "https") {
                bail!("Seed deve usar http ou https: {seed}");
            }
            seeds.push(url);
        }
        Ok(seeds)
    }

    fn fallback() -> Self {
        Self {
            database_url: DEFAULT_DATABASE_URL.to_string(),
            seed_urls: vec![Url::parse(DEFAULT_SEED).expect("seed padrão válida")],
            user_agent: DEFAULT_USER_AGENT.to_string(),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            politeness_delay: DEFAULT_POLITENESS_DELAY,
            default_priority: DEFAULT_PRIORITY,
            retry_max_attempts: DEFAULT_RETRY_MAX_ATTEMPTS,
            retry_backoff_secs: DEFAULT_RETRY_BACKOFF_SECS,
            host_delay: DEFAULT_HOST_DELAY,
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            allowed_content_types: DEFAULT_CONTENT_TYPES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

impl TryFrom<FileConfig> for AppConfig {
    type Error = anyhow::Error;

    fn try_from(raw: FileConfig) -> Result<Self> {
        let database_url = raw
            .database_url
            .unwrap_or_else(|| DEFAULT_DATABASE_URL.to_string());
        let seed_urls = Self::ensure_seeds(raw.seeds)?;
        let user_agent = raw
            .user_agent
            .unwrap_or_else(|| DEFAULT_USER_AGENT.to_string());

        let request_timeout = Duration::from_secs(
            raw.request_timeout_secs
                .unwrap_or(DEFAULT_REQUEST_TIMEOUT_SECS),
        );
        Self::validate_duration("request_timeout_secs", request_timeout)?;

        let politeness_delay = Duration::from_secs(
            raw.politeness_delay_secs
                .unwrap_or(DEFAULT_POLITENESS_DELAY_SECS),
        );
        Self::validate_duration("politeness_delay_secs", politeness_delay)?;

        let default_priority = raw.default_priority.unwrap_or(DEFAULT_PRIORITY);
        let retry_max_attempts = raw.retry_max_attempts.unwrap_or(DEFAULT_RETRY_MAX_ATTEMPTS);
        let retry_backoff_secs = raw.retry_backoff_secs.unwrap_or(DEFAULT_RETRY_BACKOFF_SECS);
        if retry_max_attempts == 0 {
            bail!("retry_max_attempts deve ser maior que zero");
        }
        if retry_backoff_secs == 0 {
            bail!("retry_backoff_secs deve ser maior que zero");
        }

        let host_delay =
            Duration::from_secs(raw.host_delay_secs.unwrap_or(DEFAULT_HOST_DELAY_SECS));
        Self::validate_duration("host_delay_secs", host_delay)?;

        let max_response_bytes = raw
            .max_response_bytes
            .unwrap_or(DEFAULT_MAX_RESPONSE_BYTES as u64)
            .max(1) as usize;

        let allowed_content_types = raw
            .allowed_content_types
            .unwrap_or_else(|| {
                DEFAULT_CONTENT_TYPES
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            })
            .into_iter()
            .map(|t| t.to_ascii_lowercase())
            .collect::<Vec<_>>();

        Ok(Self {
            database_url,
            seed_urls,
            user_agent,
            request_timeout,
            politeness_delay,
            default_priority,
            retry_max_attempts,
            retry_backoff_secs,
            host_delay,
            max_response_bytes,
            allowed_content_types,
        })
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::fallback()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_json(raw: &str) -> Result<AppConfig> {
        let cfg: FileConfig = serde_json::from_str(raw).unwrap();
        AppConfig::try_from(cfg)
    }

    #[test]
    fn uses_default_seed_when_none_provided() {
        let json = r#"{"database_url":"sqlite::memory:"}"#;
        let config = from_json(json).unwrap();
        assert_eq!(config.seed_urls.len(), 1);
        assert_eq!(config.seed_urls[0].as_str(), DEFAULT_SEED);
    }

    #[test]
    fn rejects_non_http_seed() {
        let json = r#"{"seeds":["ftp://example.com"]}"#;
        assert!(from_json(json).is_err());
    }

    #[test]
    fn rejects_zero_durations() {
        let json = r#"{"request_timeout_secs":0}"#;
        assert!(from_json(json).is_err());

        let json = r#"{"politeness_delay_secs":0}"#;
        assert!(from_json(json).is_err());
    }

    #[test]
    fn validates_retry_configuration() {
        let json = r#"{"retry_max_attempts":0}"#;
        assert!(from_json(json).is_err());

        let json = r#"{"retry_backoff_secs":0}"#;
        assert!(from_json(json).is_err());
    }

    #[test]
    fn validates_host_delay_and_content_types() {
        let json = r#"{"host_delay_secs":0}"#;
        assert!(from_json(json).is_err());

        let json = r#"{"allowed_content_types":["Text/HTML","application/json"],"max_response_bytes":100}"#;
        let cfg = from_json(json).unwrap();
        assert_eq!(
            cfg.allowed_content_types,
            vec!["text/html", "application/json"]
        );
        assert_eq!(cfg.max_response_bytes, 100);
    }
}

#[derive(Debug, Deserialize)]
struct FileConfig {
    #[serde(default)]
    database_url: Option<String>,
    #[serde(default)]
    seeds: Option<Vec<String>>,
    #[serde(default)]
    user_agent: Option<String>,
    #[serde(default)]
    request_timeout_secs: Option<u64>,
    #[serde(default)]
    politeness_delay_secs: Option<u64>,
    #[serde(default)]
    default_priority: Option<i32>,
    #[serde(default)]
    retry_max_attempts: Option<u32>,
    #[serde(default)]
    retry_backoff_secs: Option<u64>,
    #[serde(default)]
    host_delay_secs: Option<u64>,
    #[serde(default)]
    max_response_bytes: Option<u64>,
    #[serde(default)]
    allowed_content_types: Option<Vec<String>>,
}
