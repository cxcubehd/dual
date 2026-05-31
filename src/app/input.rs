use winit::event_loop::ActiveEventLoop;
use winit::keyboard::KeyCode;

use crate::render::MenuOption;

use super::{App, AppState};

impl App {
    pub(super) fn handle_key_pressed(&mut self, key: KeyCode, event_loop: &ActiveEventLoop) {
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
                        self.shutdown_network();
                        self.state = AppState::Disconnected;
                        event_loop.exit();
                    }
                    MenuOption::Quit => {
                        self.shutdown_network();
                        event_loop.exit();
                    }
                }
            }
            _ => {}
        }
    }

    pub(super) fn handle_key_released(&mut self, key: KeyCode, event_loop: &ActiveEventLoop) {
        if let Some(game) = &self.game {
            if key == KeyCode::F12 && game.input.is_shift_held() {
                self.shutdown_network();
                event_loop.exit();
                return;
            }
        }
        if let Some(game) = &mut self.game {
            game.input.set_key(key, false);
        }
    }
}
