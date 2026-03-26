use glam::Vec3;
use parking_lot::Mutex;
use shared::types::{PlayerId, PlayerState};
use winit::keyboard::KeyCode;

use crate::input::InputState;

// ── Input snapshot (written by winit, read by network tick) ─────────────────

/// Keyboard/mouse state written by the winit event loop.
pub struct InputSnapshot {
    inner: InputState,
    pub movement: u8,
    /// Accumulated mouse X delta since last tick (consumed by the network loop).
    pub yaw_delta_accum: f32,
}

impl InputSnapshot {
    pub fn press(&mut self, key: KeyCode) {
        self.inner.press(key);
        self.movement = self.inner.movement;
    }

    pub fn release(&mut self, key: KeyCode) {
        self.inner.release(key);
        self.movement = self.inner.movement;
    }

    pub fn set_shoot(&mut self, active: bool) {
        self.inner.set_shoot(active);
        self.movement = self.inner.movement;
    }

    /// Accumulate raw mouse X movement (called from DeviceEvent::MouseMotion).
    pub fn accumulate_yaw(&mut self, dx: f64) {
        self.yaw_delta_accum += dx as f32;
    }

    /// Drain the accumulated yaw delta (called once per network tick).
    pub fn take_yaw_delta(&mut self) -> f32 {
        let d = self.yaw_delta_accum;
        self.yaw_delta_accum = 0.0;
        d
    }
}

// ── Debug settings (toggled by F3/F4) ───────────────────────────────────────

#[derive(Clone)]
pub struct DebugSettings {
    /// F3: show debug overlay (ghost position, RTT, tick numbers)
    pub show_overlay: bool,
    /// F4: simulated outbound latency in ms (cycles 0 → 50 → 100 → 200 → 0)
    pub simulated_latency_ms: u32,
}

impl DebugSettings {
    pub fn new() -> Self {
        Self {
            show_overlay: false,
            simulated_latency_ms: 0,
        }
    }

    pub fn cycle_latency(&mut self) {
        self.simulated_latency_ms = match self.simulated_latency_ms {
            0 => 50,
            50 => 100,
            100 => 200,
            _ => 0,
        };
    }
}

// ── Game view (written by network/prediction, read by renderer) ─────────────

/// Game world view written by the network task, read by the renderer.
#[derive(Clone)]
pub struct GameView {
    pub player_id: Option<PlayerId>,
    /// Server-authoritative player states (what the server last told us).
    pub players: Vec<PlayerState>,
    /// Client-predicted position for the local player (may differ from server).
    pub predicted_pos: Vec3,
    /// Client-predicted yaw for the camera direction.
    pub predicted_yaw: f32,
    /// Server-confirmed position for the local player (for debug ghost).
    pub server_pos: Vec3,
    /// Estimated round-trip time in milliseconds.
    pub rtt_ms: f32,
    /// Current client tick.
    pub client_tick: u16,
    /// Last server tick we received.
    pub server_tick: u16,
    /// Number of unacknowledged inputs in the prediction buffer.
    pub pending_inputs: usize,
}

impl GameView {
    pub fn new() -> Self {
        Self {
            player_id: None,
            players: Vec::new(),
            predicted_pos: Vec3::ZERO,
            predicted_yaw: 0.0,
            server_pos: Vec3::ZERO,
            rtt_ms: 0.0,
            client_tick: 0,
            server_tick: 0,
            pending_inputs: 0,
        }
    }
}

// ── Shared state ─────────────────────────────────────────────────────────────

/// State shared between the winit main thread and the background network thread.
pub struct SharedState {
    pub input: Mutex<InputSnapshot>,
    pub game: Mutex<GameView>,
    pub debug: Mutex<DebugSettings>,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            input: Mutex::new(InputSnapshot {
                inner: InputState::new(),
                movement: 0,
                yaw_delta_accum: 0.0,
            }),
            game: Mutex::new(GameView::new()),
            debug: Mutex::new(DebugSettings::new()),
        }
    }
}
