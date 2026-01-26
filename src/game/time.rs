use std::time::{Duration, Instant};

pub struct GameTime {
    last_update: Instant,
    accumulator: Duration,
    fixed_timestep: Duration,
}

impl GameTime {
    pub fn new(tick_rate: u32) -> Self {
        Self {
            last_update: Instant::now(),
            accumulator: Duration::ZERO,
            fixed_timestep: Duration::from_secs_f64(1.0 / tick_rate as f64),
        }
    }

    pub fn update(&mut self) -> f32 {
        let now = Instant::now();
        let frame_time = now - self.last_update;
        self.last_update = now;

        let max_frame_time = self.fixed_timestep * 8;
        self.accumulator += frame_time.min(max_frame_time);

        frame_time.as_secs_f32()
    }

    pub fn should_fixed_update(&mut self) -> bool {
        if self.accumulator >= self.fixed_timestep {
            self.accumulator -= self.fixed_timestep;
            true
        } else {
            false
        }
    }

    pub fn fixed_dt(&self) -> f32 {
        self.fixed_timestep.as_secs_f32()
    }

    #[allow(dead_code)]
    pub fn alpha(&self) -> f32 {
        self.accumulator.as_secs_f32() / self.fixed_timestep.as_secs_f32()
    }
}
