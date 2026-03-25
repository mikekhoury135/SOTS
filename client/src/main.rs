// Hide the console window in release builds on Windows.
// In debug builds the console stays visible so tracing logs are readable.
#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod app;
mod input;
mod network;
mod renderer;
mod state;

use std::sync::Arc;

use anyhow::Result;
use winit::event_loop::{ControlFlow, EventLoop};

use app::App;
use state::SharedState;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Server address from first CLI arg, default to localhost
    let server_addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:7777".to_string());

    tracing::info!("Connecting to server at {server_addr}");

    let shared = Arc::new(SharedState::new());

    // Network task runs in a background thread with its own tokio runtime.
    // The winit event loop must own the main thread (required on Windows/macOS).
    let shared_net = Arc::clone(&shared);
    std::thread::Builder::new()
        .name("net".into())
        .spawn(move || {
            tokio::runtime::Runtime::new()
                .expect("tokio runtime")
                .block_on(network::run_client(server_addr, shared_net));
        })?;

    // Run winit event loop on the main thread.
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(shared);
    event_loop.run_app(&mut app)?;

    Ok(())
}
