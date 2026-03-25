use winit::keyboard::KeyCode;

use shared::types::movement;

/// Manages WASD key state and converts it to the movement bitfield used in InputFrame.
pub struct InputState {
    pub movement: u8,
}

impl InputState {
    pub fn new() -> Self {
        Self { movement: 0 }
    }

    pub fn press(&mut self, key: KeyCode) {
        match key {
            KeyCode::KeyW | KeyCode::ArrowUp => self.movement |= movement::FORWARD,
            KeyCode::KeyS | KeyCode::ArrowDown => self.movement |= movement::BACKWARD,
            KeyCode::KeyA | KeyCode::ArrowLeft => self.movement |= movement::LEFT,
            KeyCode::KeyD | KeyCode::ArrowRight => self.movement |= movement::RIGHT,
            _ => {}
        }
    }

    pub fn release(&mut self, key: KeyCode) {
        match key {
            KeyCode::KeyW | KeyCode::ArrowUp => self.movement &= !movement::FORWARD,
            KeyCode::KeyS | KeyCode::ArrowDown => self.movement &= !movement::BACKWARD,
            KeyCode::KeyA | KeyCode::ArrowLeft => self.movement &= !movement::LEFT,
            KeyCode::KeyD | KeyCode::ArrowRight => self.movement &= !movement::RIGHT,
            _ => {}
        }
    }
}
