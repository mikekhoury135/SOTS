// Hide the console window in release builds on Windows.
// In debug builds the console stays visible so tracing logs are readable.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod input;
mod network;
mod renderer;

use anyhow::Result;
use tracing::info;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("SOTS client starting");

    // Phase 3 will wire up:
    // 1. winit event loop (window creation + input polling)
    // 2. wgpu renderer (device, surface, swap chain, render pipeline)
    // 3. Async UDP network task (tokio) alongside the winit loop
    // 4. Client-side prediction module
    //
    // Architecture:
    //   [winit event loop] --> [input module] --> InputFrame
    //   [network task]     <-- InputFrame / --> StateUpdate
    //   [renderer]         <-- interpolated game state

    info!("Phase 0 stub — no window or networking yet.");
    info!("Target: Windows native (wgpu + winit)");

    Ok(())
}
