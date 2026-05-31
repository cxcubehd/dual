use std::collections::VecDeque;
use std::time::Instant;

const SAMPLE_COUNT: usize = 60;

pub struct DebugStats {
    frame_times: VecDeque<f32>,
    tick_times: VecDeque<Instant>,
    fps: f32,
    tick_rate: f32,
}

impl Default for DebugStats {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugStats {
    pub fn new() -> Self {
        Self {
            frame_times: VecDeque::with_capacity(SAMPLE_COUNT),
            tick_times: VecDeque::with_capacity(SAMPLE_COUNT),
            fps: 0.0,
            tick_rate: 0.0,
        }
    }

    pub fn record_frame(&mut self, dt: f32) {
        if dt <= 0.0 {
            return;
        }

        if self.frame_times.len() >= SAMPLE_COUNT {
            self.frame_times.pop_front();
        }
        self.frame_times.push_back(dt);

        let avg_dt: f32 = self.frame_times.iter().sum::<f32>() / self.frame_times.len() as f32;
        self.fps = 1.0 / avg_dt;
    }

    pub fn record_tick(&mut self) {
        let now = Instant::now();

        if self.tick_times.len() >= SAMPLE_COUNT {
            self.tick_times.pop_front();
        }
        self.tick_times.push_back(now);

        if self.tick_times.len() >= 2 {
            let oldest = self.tick_times.front().unwrap();
            let elapsed = now.duration_since(*oldest).as_secs_f32();
            if elapsed > 0.0 {
                self.tick_rate = (self.tick_times.len() - 1) as f32 / elapsed;
            }
        }
    }

    pub fn fps(&self) -> f32 {
        self.fps
    }

    pub fn tick_rate(&self) -> f32 {
        self.tick_rate
    }
}
