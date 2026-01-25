use std::collections::HashSet;
use winit::keyboard::KeyCode;

pub struct Input {
    pub keys_held: HashSet<KeyCode>,
    pub mouse_delta: (f64, f64),
    pub cursor_captured: bool,
}

impl Input {
    pub fn new() -> Self {
        Self {
            keys_held: HashSet::new(),
            mouse_delta: (0.0, 0.0),
            cursor_captured: false,
        }
    }

    pub fn is_key_held(&self, key: KeyCode) -> bool {
        self.keys_held.contains(&key)
    }

    pub fn reset_mouse_delta(&mut self) {
        self.mouse_delta = (0.0, 0.0);
    }
}
