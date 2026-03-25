mod config;
mod game;
mod network;

use anyhow::Result;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = config::ServerConfig::default();
    info!(
        port = config.port,
        tick_rate = config.tick_rate,
        "SOTS server starting"
    );

    network::run_server(config).await
}
