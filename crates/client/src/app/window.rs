use winit::window::{CursorGrabMode, Fullscreen};

use super::App;

impl App {
    pub(super) fn set_cursor_captured(&mut self, captured: bool) {
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

    pub(super) fn toggle_fullscreen(&mut self) {
        let Some(window) = &self.window else { return };

        self.fullscreen = !self.fullscreen;
        window.set_fullscreen(self.fullscreen.then(|| Fullscreen::Borderless(None)));
    }

    pub(super) fn is_menu_visible(&mut self) -> bool {
        self.renderer
            .as_mut()
            .is_some_and(|r| r.menu_overlay().visible())
    }

    pub(super) fn handle_resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        if let Some(renderer) = &mut self.renderer {
            renderer.resize(size);
        }
        if let Some(game) = &mut self.game {
            game.camera.aspect = size.width as f32 / size.height as f32;
        }
    }
}
