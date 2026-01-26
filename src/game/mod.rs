mod input;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use glam::Vec3;

pub use input::Input;

use crate::render::Camera;

const BASE_MOVE_SPEED: f32 = 3.0;
const SPRINT_MULTIPLIER: f32 = 3.0;
const MOUSE_SENSITIVITY: f32 = 0.0002;
const TICK_RATE: u32 = 20;

pub struct SharedState {
    pub input: Input,
    pub camera: Camera,
}

pub struct GameThread {
    shared: Arc<Mutex<SharedState>>,
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl GameThread {
    pub fn new(aspect: f32) -> Self {
        let shared = Arc::new(Mutex::new(SharedState {
            input: Input::new(),
            camera: Camera::new(aspect),
        }));
        let running = Arc::new(AtomicBool::new(true));

        let handle = {
            let shared = Arc::clone(&shared);
            let running = Arc::clone(&running);
            thread::spawn(move || game_loop(shared, running))
        };

        Self {
            shared,
            running,
            handle: Some(handle),
        }
    }

    pub fn shared(&self) -> &Arc<Mutex<SharedState>> {
        &self.shared
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for GameThread {
    fn drop(&mut self) {
        self.stop();
    }
}

fn game_loop(shared: Arc<Mutex<SharedState>>, running: Arc<AtomicBool>) {
    let tick_duration = Duration::from_secs_f64(1.0 / TICK_RATE as f64);
    let dt = tick_duration.as_secs_f32();

    while running.load(Ordering::SeqCst) {
        let tick_start = Instant::now();

        {
            let mut state = shared.lock().unwrap();
            fixed_process(&mut state, dt);
            process(&mut state);
        }

        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            thread::sleep(tick_duration - elapsed);
        }
    }
}

fn fixed_process(state: &mut SharedState, dt: f32) {
    let speed = calculate_move_speed(&state.input, dt);
    process_movement(state, speed);
}

fn process(state: &mut SharedState) {
    process_mouse_look(state);
}

fn calculate_move_speed(input: &Input, dt: f32) -> f32 {
    let multiplier = if input.is_shift_held() {
        SPRINT_MULTIPLIER
    } else {
        1.0
    };
    BASE_MOVE_SPEED * multiplier * dt
}

fn process_movement(state: &mut SharedState, speed: f32) {
    let up = Vec3::Y;
    let forward = state.camera.forward_xz();
    let right = state.camera.right_xz();

    if state.input.is_forward_held() {
        state.camera.position += forward * speed;
    }
    if state.input.is_backward_held() {
        state.camera.position -= forward * speed;
    }
    if state.input.is_left_held() {
        state.camera.position -= right * speed;
    }
    if state.input.is_right_held() {
        state.camera.position += right * speed;
    }
    if state.input.is_jump_held() {
        state.camera.position += up * speed;
    }
    if state.input.is_crouch_held() {
        state.camera.position -= up * speed;
    }
}

fn process_mouse_look(state: &mut SharedState) {
    if !state.input.cursor_captured {
        state.input.consume_mouse_delta();
        return;
    }

    let (dx, dy) = state.input.consume_mouse_delta();
    state.camera.rotate(
        dx as f32 * MOUSE_SENSITIVITY,
        -dy as f32 * MOUSE_SENSITIVITY,
    );
}
