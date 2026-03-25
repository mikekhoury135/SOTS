use shared::protocol::DEFAULT_PORT;
use shared::tick::TICK_RATE;

/// Server configuration. Will be loaded from server.toml in a later phase.
#[allow(dead_code)] // fields wired up in Phase 1
pub struct ServerConfig {
    pub port: u16,
    pub tick_rate: u32,
    pub max_players: u16,
    /// UDP receive buffer size in bytes.
    pub recv_buffer_size: usize,
    /// UDP send buffer size in bytes.
    pub send_buffer_size: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_PORT,
            tick_rate: TICK_RATE,
            max_players: 32,
            recv_buffer_size: 8 * 1024 * 1024, // 8 MB
            send_buffer_size: 8 * 1024 * 1024,
        }
    }
}
