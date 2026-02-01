use std::collections::HashSet;

use winit::keyboard::KeyCode;

use crate::net::InputState;

#[derive(Default)]
pub struct Input {
    keys_held: HashSet<KeyCode>,
    mouse_delta: (f64, f64),
    scroll_jump_pending: bool,
    pub cursor_captured: bool,
}

impl Input {
    /// Convert current input state to network InputState for sending to server.
    /// Requires camera yaw/pitch to encode view angles.
    pub fn to_net_input(&self, yaw: f32, pitch: f32) -> InputState {
        let mut move_dir = [0.0f32; 3];

        // X: right/left
        if self.is_right_held() {
            move_dir[0] += 1.0;
        }
        if self.is_left_held() {
            move_dir[0] -= 1.0;
        }

        // Y: up/down (jump/crouch for vertical movement intent)
        if self.is_jump_held() {
            move_dir[1] += 1.0;
        }
        if self.is_crouch_held() {
            move_dir[1] -= 1.0;
        }

        // Z: forward/backward
        if self.is_forward_held() {
            move_dir[2] += 1.0;
        }
        if self.is_backward_held() {
            move_dir[2] -= 1.0;
        }

        // Normalize if non-zero
        let len_sq =
            move_dir[0] * move_dir[0] + move_dir[1] * move_dir[1] + move_dir[2] * move_dir[2];
        if len_sq > 1.0 {
            let len = len_sq.sqrt();
            move_dir[0] /= len;
            move_dir[1] /= len;
            move_dir[2] /= len;
        }

        InputState {
            move_direction: move_dir,
            view_yaw: yaw,
            view_pitch: pitch,
            sprint: self.is_shift_held(),
            jump: self.is_jump_held() || self.scroll_jump_pending,
            jump_held: self.is_jump_held(),
            crouch: self.is_crouch_held(),
            fire1: false,
            fire2: false,
            use_key: self.is_key_held(KeyCode::KeyE),
            reload: self.is_key_held(KeyCode::KeyR),
        }
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

    pub fn trigger_scroll_jump(&mut self) {
        self.scroll_jump_pending = true;
    }

    pub fn consume_scroll_jump(&mut self) -> bool {
        std::mem::take(&mut self.scroll_jump_pending)
    }
}
