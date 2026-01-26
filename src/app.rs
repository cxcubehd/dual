use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

use crate::camera::Camera;
use crate::input::Input;
use crate::renderer::Renderer;

const BASE_MOVE_SPEED: f32 = 3.0;
const SPRINT_MULTIPLIER: f32 = 3.0;
const MOUSE_SENSITIVITY: f32 = 0.0002;

pub struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    input: Input,
    last_frame: Instant,
    is_fullscreen: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            window: None,
            renderer: None,
            camera: Camera::new(1.0),
            input: Input::new(),
            last_frame: Instant::now(),
            is_fullscreen: false,
        }
    }

    fn update(&mut self) {
        let dt = self.calculate_delta_time();
        let speed = self.calculate_move_speed(dt);

        self.process_movement(speed);
        self.process_mouse_look();
    }

    fn calculate_delta_time(&mut self) -> f32 {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        dt
    }

    fn calculate_move_speed(&self, dt: f32) -> f32 {
        let multiplier = if self.input.is_shift_held() { SPRINT_MULTIPLIER } else { 1.0 };
        BASE_MOVE_SPEED * multiplier * dt
    }

    fn process_movement(&mut self, speed: f32) {
        let forward = self.camera.forward();
        let right = self.camera.right();
        let up = self.camera.up();

        if self.input.is_key_held(KeyCode::KeyW) {
            self.camera.position += forward * speed;
        }
        if self.input.is_key_held(KeyCode::KeyS) {
            self.camera.position -= forward * speed;
        }
        if self.input.is_key_held(KeyCode::KeyA) {
            self.camera.position -= right * speed;
        }
        if self.input.is_key_held(KeyCode::KeyD) {
            self.camera.position += right * speed;
        }
        if self.input.is_key_held(KeyCode::Space) {
            self.camera.position += up * speed;
        }
        if self.input.is_ctrl_held() {
            self.camera.position -= up * speed;
        }
    }

    fn process_mouse_look(&mut self) {
        if !self.input.cursor_captured {
            self.input.consume_mouse_delta();
            return;
        }

        let (dx, dy) = self.input.consume_mouse_delta();
        self.camera.rotate(dx as f32 * MOUSE_SENSITIVITY, -dy as f32 * MOUSE_SENSITIVITY);
    }

    fn set_cursor_captured(&mut self, captured: bool) {
        self.input.cursor_captured = captured;

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

        self.is_fullscreen = !self.is_fullscreen;
        let mode = if self.is_fullscreen {
            Some(Fullscreen::Borderless(None))
        } else {
            None
        };
        window.set_fullscreen(mode);
    }

    fn handle_key_pressed(&mut self, key: KeyCode) {
        match key {
            KeyCode::Escape => self.set_cursor_captured(false),
            KeyCode::F11 => self.toggle_fullscreen(),
            _ => {
                self.input.keys_held.insert(key);
            }
        }
    }

    fn handle_key_released(&mut self, key: KeyCode, event_loop: &ActiveEventLoop) {
        if key == KeyCode::F12 && self.input.is_shift_held() {
            event_loop.exit();
        }
        self.input.keys_held.remove(&key);
    }

    fn handle_resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(size);
            self.camera.aspect = size.width as f32 / size.height as f32;
        }
    }

    fn handle_redraw(&mut self, event_loop: &ActiveEventLoop) {
        self.update();

        if let Some(renderer) = &mut self.renderer {
            renderer.update_camera(&self.camera);

            match renderer.render() {
                Ok(()) => {}
                Err(wgpu::SurfaceError::Lost) => renderer.resize(renderer.size),
                Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                Err(e) => eprintln!("Render error: {e:?}"),
            }
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
            .with_title("3D Cube")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));

        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        self.window = Some(window.clone());

        let rt = tokio::runtime::Runtime::new().unwrap();
        let renderer = rt.block_on(Renderer::new(window)).unwrap();
        self.camera.aspect = renderer.size.width as f32 / renderer.size.height as f32;
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
                if !self.input.cursor_captured {
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
            self.input.accumulate_mouse_delta(delta);
        }
    }
}
