use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

use crate::game::GameState;
use crate::render::Renderer;

use super::App;

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("Dual")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        self.window = Some(window.clone());

        let rt = tokio::runtime::Runtime::new().unwrap();
        let renderer = rt.block_on(Renderer::new(window)).unwrap();

        let aspect = renderer.size.width as f32 / renderer.size.height as f32;
        self.game = Some(GameState::new(aspect));
        self.renderer = Some(renderer);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.shutdown_network();
                event_loop.exit();
            }
            WindowEvent::Resized(size) => self.handle_resize(size),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => self.handle_key_pressed(key, event_loop),
                        ElementState::Released => self.handle_key_released(key, event_loop),
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let cursor_captured = self.game.as_ref().is_some_and(|g| g.input.cursor_captured);
                if !cursor_captured {
                    self.set_cursor_captured(true);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(game) = &mut self.game {
                    if game.input.cursor_captured {
                        let scroll_up = match delta {
                            MouseScrollDelta::LineDelta(_, y) => y > 0.0,
                            MouseScrollDelta::PixelDelta(pos) => pos.y > 0.0,
                        };
                        if scroll_up {
                            game.input.trigger_scroll_jump();
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => self.handle_redraw(event_loop),
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if let Some(game) = &mut self.game {
                game.input.accumulate_mouse_delta(delta);
            }
        }
    }
}
