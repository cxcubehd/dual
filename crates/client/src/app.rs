use std::sync::Arc;

use dual::ConnectionState;
use glam::{Mat4, Vec3};
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

use crate::debug::DebugStats;
use crate::game::GameState;
use crate::net::NetworkClient;
use crate::render::{MenuOption, Renderer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Playing,
    Disconnected,
}

pub struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    game: Option<GameState>,
    network_client: Option<NetworkClient>,
    debug_stats: DebugStats,
    fullscreen: bool,
    state: AppState,
    player_cube_indices: Vec<usize>,
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
            network_client: None,
            debug_stats: DebugStats::new(),
            fullscreen: false,
            state: AppState::Playing,
            player_cube_indices: Vec::new(),
        }
    }

    pub fn with_network_client(client: Option<NetworkClient>) -> Self {
        Self {
            window: None,
            renderer: None,
            game: None,
            network_client: client,
            debug_stats: DebugStats::new(),
            fullscreen: false,
            state: AppState::Playing,
            player_cube_indices: Vec::new(),
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

    fn is_menu_visible(&mut self) -> bool {
        self.renderer
            .as_mut()
            .is_some_and(|r| r.menu_overlay().visible())
    }

    fn handle_key_pressed(&mut self, key: KeyCode, event_loop: &ActiveEventLoop) {
        if self.is_menu_visible() {
            self.handle_menu_key(key, event_loop);
            return;
        }

        match key {
            KeyCode::Escape => {
                self.set_cursor_captured(false);
                if let Some(renderer) = &mut self.renderer {
                    renderer.menu_overlay().show();
                }
            }
            KeyCode::F11 => self.toggle_fullscreen(),
            _ => {
                if let Some(game) = &mut self.game {
                    game.input.set_key(key, true);
                }
            }
        }
    }

    fn handle_menu_key(&mut self, key: KeyCode, event_loop: &ActiveEventLoop) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };

        match key {
            KeyCode::Escape => {
                renderer.menu_overlay().hide();
                self.set_cursor_captured(true);
            }
            KeyCode::ArrowUp | KeyCode::KeyW => {
                renderer.menu_overlay().move_up();
            }
            KeyCode::ArrowDown | KeyCode::KeyS => {
                renderer.menu_overlay().move_down();
            }
            KeyCode::Enter => {
                let option = renderer.menu_overlay().selected_option();
                match option {
                    MenuOption::Resume => {
                        renderer.menu_overlay().hide();
                        self.set_cursor_captured(true);
                    }
                    MenuOption::Disconnect => {
                        if let Some(client) = &mut self.network_client {
                            client.shutdown();
                        }
                        self.state = AppState::Disconnected;
                        event_loop.exit();
                    }
                    MenuOption::Quit => {
                        if let Some(client) = &mut self.network_client {
                            client.shutdown();
                        }
                        event_loop.exit();
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_key_released(&mut self, key: KeyCode, event_loop: &ActiveEventLoop) {
        if let Some(game) = &self.game {
            if key == KeyCode::F12 && game.input.is_shift_held() {
                if let Some(client) = &mut self.network_client {
                    client.shutdown();
                }
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

        let networked = self.network_client.is_some();
        let dt = game.update(networked);
        self.debug_stats.record_frame(dt);
        self.debug_stats.record_tick();

        if let Some(client) = &mut self.network_client {
            let input_state = game
                .input
                .to_net_input(game.camera.yaw as f32, game.camera.pitch as f32);

            if let Err(e) = client.update(dt, Some(&input_state)) {
                log::error!("Network error: {}", e);
            }

            if client.state() == ConnectionState::Disconnected {
                self.state = AppState::Disconnected;
                log::info!("Disconnected from server, returning to menu");
                event_loop.exit();
                return;
            }

            game.camera.position = client.predicted_position();

            Self::update_player_cubes(&mut self.player_cube_indices, client, renderer);
        }

        renderer.update_camera(&game.camera);
        renderer.update_debug_overlay(self.debug_stats.fps(), self.debug_stats.tick_rate());

        match renderer.render() {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Lost) => renderer.resize(renderer.size),
            Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
            Err(e) => log::error!("Render error: {:?}", e),
        }

        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn update_player_cubes(
        player_cube_indices: &mut Vec<usize>,
        client: &NetworkClient,
        renderer: &mut Renderer,
    ) {
        let my_entity_id = client.entity_id();

        let entities: Vec<_> = client
            .entities()
            .filter(|e| e.entity_type == dual::EntityType::Player)
            .collect();

        while player_cube_indices.len() < entities.len() {
            if let Ok(idx) = renderer.add_player_cube() {
                player_cube_indices.push(idx);
            } else {
                break;
            }
        }

        for (i, entity) in entities.iter().enumerate() {
            if let Some(&cube_idx) = player_cube_indices.get(i) {
                let is_local = my_entity_id.is_some_and(|id| entity.id == id);

                if !is_local {
                    let transform = Mat4::from_translation(entity.position)
                        * Mat4::from_quat(entity.orientation)
                        * Mat4::from_scale(Vec3::splat(0.4));
                    renderer.set_player_cube_transform(cube_idx, transform);
                    renderer.set_player_cube_visible(cube_idx, true);
                } else {
                    renderer.set_player_cube_visible(cube_idx, false);
                }
            }
        }

        for i in entities.len()..player_cube_indices.len() {
            if let Some(&cube_idx) = player_cube_indices.get(i) {
                renderer.set_player_cube_visible(cube_idx, false);
            }
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
            WindowEvent::CloseRequested => {
                if let Some(client) = &mut self.network_client {
                    client.shutdown();
                }
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
