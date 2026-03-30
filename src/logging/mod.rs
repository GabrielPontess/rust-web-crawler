use anyhow::Result;
use tracing_subscriber::fmt::time::SystemTime;
use tracing_subscriber::{EnvFilter, fmt};

pub fn init() -> Result<()> {
    if tracing::subscriber::set_global_default(build_subscriber()).is_err() {
        // someone already installed a subscriber (e.g., tests). Just continue.
        return Ok(());
    }
    Ok(())
}

fn build_subscriber() -> impl tracing::Subscriber + Send + Sync {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    fmt()
        .with_env_filter(env_filter)
        .with_timer(SystemTime)
        .with_target(false)
        .compact()
        .finish()
}
