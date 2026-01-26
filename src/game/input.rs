use std::collections::HashSet;
use winit::keyboard::KeyCode;

#[derive(Default)]
pub struct Input {
    pub keys_held: HashSet<KeyCode>,
    pub mouse_delta: (f64, f64),
    pub cursor_captured: bool,
}

impl Input {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_key(&mut self, key: KeyCode, pressed: bool) {
        if pressed {
            self.keys_held.insert(key);
        } else {
            self.keys_held.remove(&key);
        }
    }

    pub fn is_key_held(&self, key: KeyCode) -> bool {
        self.keys_held.contains(&key)
    }

    pub fn is_shift_held(&self) -> bool {
        self.is_key_held(KeyCode::ShiftLeft) || self.is_key_held(KeyCode::ShiftRight)
    }

    pub fn is_ctrl_held(&self) -> bool {
        self.is_key_held(KeyCode::ControlLeft) || self.is_key_held(KeyCode::ControlRight)
    }

    pub fn is_forward_held(&self) -> bool {
        self.is_key_held(KeyCode::KeyW)
    }

    pub fn is_backward_held(&self) -> bool {
        self.is_key_held(KeyCode::KeyS)
    }

    pub fn is_left_held(&self) -> bool {
        self.is_key_held(KeyCode::KeyA)
    }

    pub fn is_right_held(&self) -> bool {
        self.is_key_held(KeyCode::KeyD)
    }

    pub fn is_jump_held(&self) -> bool {
        self.is_key_held(KeyCode::Space)
    }

    pub fn is_crouch_held(&self) -> bool {
        self.is_ctrl_held()
    }

    pub fn accumulate_mouse_delta(&mut self, delta: (f64, f64)) {
        self.mouse_delta.0 += delta.0;
        self.mouse_delta.1 += delta.1;
    }

    pub fn consume_mouse_delta(&mut self) -> (f64, f64) {
        std::mem::take(&mut self.mouse_delta)
    }
}
