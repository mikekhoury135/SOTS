use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowAttributes, WindowId},
};

use crate::{renderer::Renderer, state::SharedState};

pub struct App {
    pub shared: Arc<SharedState>,
    state: Option<RunState>,
}

struct RunState {
    window: Arc<Window>,
    renderer: Renderer,
}

impl App {
    pub fn new(shared: Arc<SharedState>) -> Self {
        Self {
            shared,
            state: None,
        }
    }

    /// Grab the cursor for FPS mouse look.
    fn grab_cursor(window: &Window) {
        // Try Locked first (ideal for FPS — hides & locks), fall back to Confined.
        if window
            .set_cursor_grab(CursorGrabMode::Locked)
            .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
            .is_ok()
        {
            window.set_cursor_visible(false);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("SOTS — Super Optimized Tactical Shooter")
            .with_inner_size(winit::dpi::PhysicalSize::new(1280u32, 720u32));

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("Failed to create window"),
        );

        // Capture the cursor for FPS mouse-look
        Self::grab_cursor(&window);

        let renderer = pollster::block_on(Renderer::new(window.clone()))
            .expect("Failed to initialise wgpu renderer");

        self.state = Some(RunState { window, renderer });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = &mut self.state else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(new_size) => {
                state.renderer.resize(new_size);
            }

            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if let PhysicalKey::Code(code) = key_event.physical_key {
                    if key_event.state == ElementState::Pressed {
                        match code {
                            KeyCode::Escape => {
                                event_loop.exit();
                                return;
                            }
                            KeyCode::F3 => {
                                let mut dbg = self.shared.debug.lock();
                                dbg.show_overlay = !dbg.show_overlay;
                                return;
                            }
                            KeyCode::F4 => {
                                let mut dbg = self.shared.debug.lock();
                                dbg.cycle_latency();
                                tracing::info!(
                                    "Simulated latency: {} ms",
                                    dbg.simulated_latency_ms
                                );
                                return;
                            }
                            _ => {}
                        }
                    }

                    let mut input = self.shared.input.lock();
                    match key_event.state {
                        ElementState::Pressed => input.press(code),
                        ElementState::Released => input.release(code),
                    }
                }
            }

            WindowEvent::MouseInput {
                state: btn_state,
                button,
                ..
            } => {
                if button == MouseButton::Left {
                    let mut input = self.shared.input.lock();
                    input.set_shoot(btn_state == ElementState::Pressed);
                }
            }

            WindowEvent::Focused(true) => {
                // Re-grab cursor when the window regains focus
                Self::grab_cursor(&state.window);
            }

            WindowEvent::RedrawRequested => {
                let game = self.shared.game.lock().clone();
                let debug = self.shared.debug.lock().clone();
                if state.renderer.render(&game, &debug) {
                    state.renderer.reconfigure();
                }
            }

            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            let mut input = self.shared.input.lock();
            input.accumulate_yaw(delta.0);
            input.accumulate_pitch(delta.1); // positive delta.1 = mouse down = look down
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}
