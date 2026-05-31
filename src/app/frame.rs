use dual::ConnectionState;

use crate::render::RenderError;

use super::{App, AppState};

impl App {
    pub(super) fn handle_redraw(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
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

            match client.update(dt, Some(&input_state)) {
                Ok(ticks_processed) => {
                    if ticks_processed {
                        game.input.consume_scroll_jump();
                    }
                }
                Err(e) => {
                    log::error!("Network error: {}", e);
                }
            }

            if client.state() == ConnectionState::Disconnected {
                self.state = AppState::Disconnected;
                log::info!("Disconnected from server, returning to menu");
                event_loop.exit();
                return;
            }

            game.camera.position = client.predicted_position();

            Self::update_player_cubes(&mut self.player_cube_indices, client, renderer);
            Self::update_dynamic_props(&mut self.dynamic_prop_indices, client, renderer);
        }

        renderer.update_camera(&game.camera);
        renderer.update_debug_overlay(self.debug_stats.fps(), self.debug_stats.tick_rate());

        match renderer.render() {
            Ok(()) => {}
            Err(RenderError::Lost | RenderError::Outdated) => renderer.resize(renderer.size),
            Err(RenderError::Timeout | RenderError::Occluded) => {}
            Err(RenderError::Validation) => log::error!("Render validation error"),
        }

        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
