use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

use crate::game::GameState;
use crate::render::Renderer;

pub struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    game: Option<GameState>,
    fullscreen: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            game: None,
            fullscreen: false,
        }
    }

    fn set_cursor_captured(&mut self, captured: bool) {
        if let Some(game) = &mut self.game {
            game.input.cursor_captured = captured;
        }

        let Some(window) = &self.window else { return };

        if captured {
            let _ = window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
            window.set_cursor_visible(false);
        } else {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
        }
    }

    fn toggle_fullscreen(&mut self) {
        let Some(window) = &self.window else { return };

        self.fullscreen = !self.fullscreen;
        window.set_fullscreen(self.fullscreen.then(|| Fullscreen::Borderless(None)));
    }

    fn handle_key_pressed(&mut self, key: KeyCode) {
        match key {
            KeyCode::Escape => self.set_cursor_captured(false),
            KeyCode::F11 => self.toggle_fullscreen(),
            _ => {
                if let Some(game) = &mut self.game {
                    game.input.set_key(key, true);
                }
            }
        }
    }

    fn handle_key_released(&mut self, key: KeyCode, event_loop: &ActiveEventLoop) {
        if let Some(game) = &self.game {
            if key == KeyCode::F12 && game.input.is_shift_held() {
                event_loop.exit();
                return;
            }
        }
        if let Some(game) = &mut self.game {
            game.input.set_key(key, false);
        }
    }

    fn handle_resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(size);
        }
        if let Some(game) = &mut self.game {
            game.camera.aspect = size.width as f32 / size.height as f32;
        }
    }

    fn handle_redraw(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(renderer), Some(game)) = (&mut self.renderer, &mut self.game) else {
            return;
        };

        game.update();
        renderer.update_camera(&game.camera);

        match renderer.render() {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Lost) => renderer.resize(renderer.size),
            Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
            Err(e) => eprintln!("Render error: {e:?}"),
        }

        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

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
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.handle_resize(size),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => self.handle_key_pressed(key),
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
