use parking_lot::Mutex;
use shared::types::{PlayerId, PlayerState};
use winit::keyboard::KeyCode;

use crate::input::InputState;

/// Keyboard/mouse state written by the winit event loop.
pub struct InputSnapshot {
    inner: InputState,
    pub movement: u8,
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
}

/// Game world view written by the network task, read by the renderer.
#[derive(Clone)]
pub struct GameView {
    pub player_id: Option<PlayerId>,
    pub players: Vec<PlayerState>,
}

/// State shared between the winit main thread and the background network thread.
pub struct SharedState {
    pub input: Mutex<InputSnapshot>,
    pub game: Mutex<GameView>,
}

impl SharedState {
    pub fn new() -> Self {
        Self {
            input: Mutex::new(InputSnapshot {
                inner: InputState::new(),
                movement: 0,
            }),
            game: Mutex::new(GameView {
                player_id: None,
                players: Vec::new(),
            }),
        }
    }
}
