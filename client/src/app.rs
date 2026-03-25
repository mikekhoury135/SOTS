use std::sync::Arc;

use winit::{
    application::ApplicationHandler,
    event::{ElementState, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::PhysicalKey,
    window::{Window, WindowAttributes, WindowId},
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
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return; // already initialised (called again on mobile resume)
        }

        let attrs = WindowAttributes::default()
            .with_title("SOTS — Super Optimized Tactical Shooter")
            .with_inner_size(winit::dpi::PhysicalSize::new(1280u32, 720u32));

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("Failed to create window"),
        );

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

            WindowEvent::KeyboardInput { event: key_event, .. } => {
                if let PhysicalKey::Code(code) = key_event.physical_key {
                    let mut input = self.shared.input.lock();
                    match key_event.state {
                        ElementState::Pressed => input.press(code),
                        ElementState::Released => input.release(code),
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                let game = self.shared.game.lock().clone();
                if state.renderer.render(&game) {
                    state.renderer.reconfigure();
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Drive continuous rendering: request a redraw every iteration.
        if let Some(state) = &self.state {
            state.window.request_redraw();
        }
    }
}
