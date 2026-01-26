mod input;
mod time;

use glam::Vec3;

pub use input::Input;
pub use time::GameTime;

use crate::render::Camera;

const BASE_MOVE_SPEED: f32 = 3.0;
const SPRINT_MULTIPLIER: f32 = 3.0;
const MOUSE_SENSITIVITY: f32 = 0.0002;

pub struct Game {
    pub camera: Camera,
    pub input: Input,
}

impl Game {
    pub fn new(aspect: f32) -> Self {
        Self {
            camera: Camera::new(aspect),
            input: Input::new(),
        }
    }

    pub fn fixed_process(&mut self, dt: f32) {
        let speed = self.calculate_move_speed(dt);
        self.process_movement(speed);
    }

    pub fn process(&mut self, _dt: f32) {
        self.process_mouse_look();
    }

    fn calculate_move_speed(&self, dt: f32) -> f32 {
        let multiplier = if self.input.is_shift_held() {
            SPRINT_MULTIPLIER
        } else {
            1.0
        };
        BASE_MOVE_SPEED * multiplier * dt
    }

    fn process_movement(&mut self, speed: f32) {
        let up = Vec3::Y;
        let forward = self.camera.forward_xz();
        let right = self.camera.right_xz();

        if self.input.is_forward_held() {
            self.camera.position += forward * speed;
        }
        if self.input.is_backward_held() {
            self.camera.position -= forward * speed;
        }
        if self.input.is_left_held() {
            self.camera.position -= right * speed;
        }
        if self.input.is_right_held() {
            self.camera.position += right * speed;
        }
        if self.input.is_jump_held() {
            self.camera.position += up * speed;
        }
        if self.input.is_crouch_held() {
            self.camera.position -= up * speed;
        }
    }

    fn process_mouse_look(&mut self) {
        if !self.input.cursor_captured {
            self.input.consume_mouse_delta();
            return;
        }

        let (dx, dy) = self.input.consume_mouse_delta();
        self.camera.rotate(
            dx as f32 * MOUSE_SENSITIVITY,
            -dy as f32 * MOUSE_SENSITIVITY,
        );
    }
}
