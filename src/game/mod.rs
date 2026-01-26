mod input;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use glam::Vec3;

pub use input::Input;

use crate::render::Camera;

const BASE_MOVE_SPEED: f32 = 3.0;
const SPRINT_MULTIPLIER: f32 = 3.0;
const MOUSE_SENSITIVITY: f32 = 0.0002;

/// Shared state between the render loop and game logic.
/// Updated at frame rate for immediate visual feedback (client-side prediction).
pub struct SharedState {
    pub input: Input,
    /// The visual camera - updated at frame rate for immediate feedback
    pub camera: Camera,
    /// Last frame time for delta calculations
    last_frame_time: Option<Instant>,
}

impl SharedState {
    pub fn new(aspect: f32) -> Self {
        Self {
            input: Input::new(),
            camera: Camera::new(aspect),
            last_frame_time: None,
        }
    }

    /// Process frame-rate updates for immediate visual feedback.
    /// This implements client-side prediction - movement and look
    /// are applied immediately for responsive feel.
    pub fn frame_update(&mut self) {
        let now = Instant::now();
        let dt = self
            .last_frame_time
            .map(|t| now.duration_since(t).as_secs_f32())
            .unwrap_or(0.0)
            .min(0.1); // Cap delta to avoid huge jumps
        self.last_frame_time = Some(now);

        // Process mouse look immediately for low-latency camera control
        self.process_mouse_look();

        // Process movement immediately (client-side prediction)
        self.process_movement_prediction(dt);
    }

    /// Process mouse input immediately for responsive camera control.
    fn process_mouse_look(&mut self) {
        if !self.input.cursor_captured {
            self.input.consume_mouse_delta();
            return;
        }

        let (dx, dy) = self.input.consume_mouse_delta();
        if dx != 0.0 || dy != 0.0 {
            self.camera.rotate(
                dx as f32 * MOUSE_SENSITIVITY,
                -dy as f32 * MOUSE_SENSITIVITY,
            );
        }
    }

    /// Apply movement prediction immediately at frame rate.
    /// This gives the player immediate feedback on movement input.
    fn process_movement_prediction(&mut self, dt: f32) {
        let speed = self.calculate_move_speed(dt);
        if speed == 0.0 {
            return;
        }

        let up = Vec3::Y;
        let forward = self.camera.forward_xz();
        let right = self.camera.right_xz();

        // Build movement vector from input
        let mut movement = Vec3::ZERO;

        if self.input.is_forward_held() {
            movement += forward;
        }
        if self.input.is_backward_held() {
            movement -= forward;
        }
        if self.input.is_left_held() {
            movement -= right;
        }
        if self.input.is_right_held() {
            movement += right;
        }
        if self.input.is_jump_held() {
            movement += up;
        }
        if self.input.is_crouch_held() {
            movement -= up;
        }

        // Normalize diagonal movement to prevent faster diagonal speed
        if movement.length_squared() > 0.0 {
            movement = movement.normalize();
            self.camera.position += movement * speed;
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

/// Game state manager - no longer uses a separate thread.
/// Instead, updates happen at frame rate for immediate feedback.
pub struct GameThread {
    shared: Arc<Mutex<SharedState>>,
    running: Arc<AtomicBool>,
}

impl GameThread {
    pub fn new(aspect: f32) -> Self {
        let shared = Arc::new(Mutex::new(SharedState::new(aspect)));
        let running = Arc::new(AtomicBool::new(true));

        Self { shared, running }
    }

    pub fn shared(&self) -> &Arc<Mutex<SharedState>> {
        &self.shared
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

impl Drop for GameThread {
    fn drop(&mut self) {
        self.stop();
    }
}
