use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use crate::camera::Camera;
use crate::input::Input;
use crate::renderer::Renderer;

pub struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    input: Input,
    last_frame: Instant,
}

impl App {
    pub fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            camera: Camera::new(1.0),
            input: Input::new(),
            last_frame: Instant::now(),
        }
    }

    fn update(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;

        let speed = 3.0 * dt;
        let sensitivity = 0.0005;

        // Movement
        if self.input.is_key_held(KeyCode::KeyW) {
            self.camera.position += self.camera.forward() * speed;
        }
        if self.input.is_key_held(KeyCode::KeyS) {
            self.camera.position -= self.camera.forward() * speed;
        }
        if self.input.is_key_held(KeyCode::KeyA) {
            self.camera.position -= self.camera.right() * speed;
        }
        if self.input.is_key_held(KeyCode::KeyD) {
            self.camera.position += self.camera.right() * speed;
        }

        // Mouse look (only when captured)
        if self.input.cursor_captured {
            let (dx, dy) = self.input.mouse_delta;
            self.camera.yaw += dx as f32 * sensitivity;
            self.camera.pitch -= dy as f32 * sensitivity;
            self.camera.pitch = self.camera.pitch.clamp(-1.5, 1.5);
        }

        self.input.reset_mouse_delta();
    }

    fn set_cursor_captured(&mut self, captured: bool) {
        self.input.cursor_captured = captured;
        if let Some(window) = &self.window {
            if captured {
                let _ = window.set_cursor_grab(CursorGrabMode::Locked)
                    .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined));
                window.set_cursor_visible(false);
            } else {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
                window.set_cursor_visible(true);
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attrs = Window::default_attributes()
                .with_title("3D Cube")
                .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

            let window = Arc::new(event_loop.create_window(window_attrs).unwrap());
            self.window = Some(window.clone());

            let rt = tokio::runtime::Runtime::new().unwrap();
            let renderer = rt.block_on(Renderer::new(window.clone())).unwrap();
            self.camera.aspect = renderer.size.width as f32 / renderer.size.height as f32;
            self.renderer = Some(renderer);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(physical_size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(physical_size);
                    self.camera.aspect = physical_size.width as f32 / physical_size.height as f32;
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(key) = event.physical_key {
                    match event.state {
                        ElementState::Pressed => {
                            if key == KeyCode::Escape {
                                self.set_cursor_captured(false);
                            } else {
                                self.input.keys_held.insert(key);
                            }
                        }
                        ElementState::Released => {
                            self.input.keys_held.remove(&key);
                        }
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if !self.input.cursor_captured {
                    self.set_cursor_captured(true);
                }
            }
            WindowEvent::RedrawRequested => {
                self.update();

                if let Some(renderer) = &mut self.renderer {
                    renderer.update_camera(&self.camera);
                    match renderer.render() {
                        Ok(_) => {}
                        Err(wgpu::SurfaceError::Lost) => renderer.resize(renderer.size),
                        Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                        Err(e) => eprintln!("Render error: {:?}", e),
                    }
                }

                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
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
            self.input.mouse_delta.0 += delta.0;
            self.input.mouse_delta.1 += delta.1;
        }
    }
}
