mod config;
mod game;
mod network;

use anyhow::Result;
use tracing::info;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = config::ServerConfig::default();
    info!(port = config.port, tick_rate = config.tick_rate, "SOTS server starting");

    // Phase 1 will wire up:
    // 1. IO recv thread(s) — raw UDP via socket2
    // 2. Game loop thread — synchronous, driven by spin_sleep
    // 3. IO send via tokio workers
    //
    // Architecture:
    //   [IO Recv] --crossbeam--> [Game Loop] --crossbeam--> [IO Send]

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        info!(
            "Server listening on 0.0.0.0:{}  (tick rate: {} Hz)",
            config.port, config.tick_rate
        );
        info!("Phase 0 stub — no networking yet. Press Ctrl+C to exit.");

        tokio::signal::ctrl_c().await?;
        info!("Shutting down.");
        Ok(())
    })
}
