mod events;
mod frame;
mod input;
mod scene_sync;
mod window;

use std::sync::Arc;

use dual::NetworkClient;
use winit::window::Window;

use crate::debug::DebugStats;
use crate::game::GameState;
use crate::render::Renderer;

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
    dynamic_prop_indices: Vec<usize>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self::with_network_client(None)
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
            dynamic_prop_indices: Vec::new(),
        }
    }

    fn shutdown_network(&mut self) {
        if let Some(client) = &mut self.network_client {
            client.shutdown();
        }
    }
}
