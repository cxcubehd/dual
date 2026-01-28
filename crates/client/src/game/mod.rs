mod input;

use std::time::Instant;

use glam::Vec3;

pub use input::Input;

use crate::render::Camera;

const BASE_MOVE_SPEED: f32 = 3.0;
const SPRINT_MULTIPLIER: f32 = 3.0;
const MOUSE_SENSITIVITY: f64 = 0.0002;

pub struct GameState {
    pub input: Input,
    pub camera: Camera,
    last_frame_time: Option<Instant>,
}

impl GameState {
    pub fn new(aspect: f32) -> Self {
        Self {
            input: Input::default(),
            camera: Camera::new(aspect),
            last_frame_time: None,
        }
    }

    pub fn update(&mut self) -> f32 {
        let now = Instant::now();
        let dt = self
            .last_frame_time
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(0.0)
            .min(0.1);
        self.last_frame_time = Some(now);

        self.process_mouse_look();
        self.process_movement(dt);

        dt
    }

    fn process_mouse_look(&mut self) {
        if !self.input.cursor_captured {
            self.input.consume_mouse_delta();
            return;
        }

        let (dx, dy) = self.input.consume_mouse_delta();
        if dx != 0.0 || dy != 0.0 {
            self.camera.rotate(
                dx * MOUSE_SENSITIVITY,
                -dy * MOUSE_SENSITIVITY,
            );
        }
    }

    fn process_movement(&mut self, dt: f32) {
        let speed = self.calculate_move_speed(dt);
        if speed == 0.0 {
            return;
        }

        let mut movement = Vec3::ZERO;

        if self.input.is_forward_held() {
            movement += self.camera.forward_xz();
        }
        if self.input.is_backward_held() {
            movement -= self.camera.forward_xz();
        }
        if self.input.is_left_held() {
            movement -= self.camera.right_xz();
        }
        if self.input.is_right_held() {
            movement += self.camera.right_xz();
        }
        if self.input.is_jump_held() {
            movement += Vec3::Y;
        }
        if self.input.is_crouch_held() {
            movement -= Vec3::Y;
        }

        if movement.length_squared() > 0.0 {
            self.camera.position += movement.normalize() * speed;
        }
    }

    fn calculate_move_speed(&self, dt: f32) -> f32 {
        let multiplier = if self.input.is_shift_held() {
            SPRINT_MULTIPLIER
        } else {
            1.0
        };
        BASE_MOVE_SPEED * multiplier * dt
    }
}
