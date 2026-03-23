use crate::physics::{PhysicsHistory, PhysicsWorld};
use crate::snapshot::World;

/// Fixed timestep using integer microseconds to avoid floating-point drift.
///
/// The accumulator stores leftover time in microseconds. Each tick consumes
/// exactly `tick_interval_us` microseconds, guaranteeing deterministic timing
/// regardless of frame rate or floating-point rounding.
pub struct FixedTimestep {
    tick_rate: u32,
    tick_interval_us: u64,
    accumulator_us: u64,
    /// Maximum accumulated time (250ms) to prevent spiral-of-death.
    max_accumulate_us: u64,
}

impl FixedTimestep {
    pub fn new(tick_rate: u32) -> Self {
        let tick_interval_us = 1_000_000 / tick_rate as u64;
        Self {
            tick_rate,
            tick_interval_us,
            accumulator_us: 0,
            max_accumulate_us: 250_000,
        }
    }

    pub fn tick_rate(&self) -> u32 {
        self.tick_rate
    }

    /// Tick interval as seconds (for physics and movement calculations).
    pub fn dt(&self) -> f32 {
        self.tick_interval_us as f32 / 1_000_000.0
    }

    /// Tick interval in microseconds.
    pub fn tick_interval_us(&self) -> u64 {
        self.tick_interval_us
    }

    /// Accumulate frame time in seconds. Capped to prevent spiral-of-death.
    pub fn accumulate(&mut self, delta_secs: f32) {
        let delta_us = (delta_secs * 1_000_000.0) as u64;
        self.accumulator_us = (self.accumulator_us + delta_us).min(self.max_accumulate_us);
    }

    /// Accumulate frame time in microseconds directly (preferred for precision).
    pub fn accumulate_us(&mut self, delta_us: u64) {
        self.accumulator_us = (self.accumulator_us + delta_us).min(self.max_accumulate_us);
    }

    pub fn should_tick(&self) -> bool {
        self.accumulator_us >= self.tick_interval_us
    }

    pub fn consume_tick(&mut self) -> bool {
        if self.accumulator_us >= self.tick_interval_us {
            self.accumulator_us -= self.tick_interval_us;
            true
        } else {
            false
        }
    }

    /// Interpolation alpha for rendering between ticks (0.0 to 1.0).
    pub fn alpha(&self) -> f32 {
        self.accumulator_us as f32 / self.tick_interval_us as f32
    }

    pub fn reset(&mut self) {
        self.accumulator_us = 0;
    }
}

pub struct SimulationState {
    pub world: World,
    pub physics: PhysicsWorld,
    pub physics_history: PhysicsHistory,
    pub timestep: FixedTimestep,
}

impl SimulationState {
    pub fn new(tick_rate: u32, history_capacity: usize) -> Self {
        Self {
            world: World::new(),
            physics: PhysicsWorld::new(),
            physics_history: PhysicsHistory::new(history_capacity),
            timestep: FixedTimestep::new(tick_rate),
        }
    }

    pub fn tick(&self) -> u32 {
        self.world.tick()
    }

    pub fn store_physics_snapshot(&mut self) {
        let tick = self.world.tick();
        let snapshot = self.physics.snapshot();
        self.physics_history.push(tick, snapshot);
    }

    pub fn rollback_to(&mut self, tick: u32) -> bool {
        if let Some(snapshot) = self.physics_history.get(tick) {
            self.physics.restore(snapshot);
            self.world.set_tick(tick);
            true
        } else {
            false
        }
    }
}

pub struct SimulationLoop<F> {
    state: SimulationState,
    tick_fn: F,
}

impl<F> SimulationLoop<F>
where
    F: FnMut(&mut SimulationState),
{
    pub fn new(tick_rate: u32, history_capacity: usize, tick_fn: F) -> Self {
        Self {
            state: SimulationState::new(tick_rate, history_capacity),
            tick_fn,
        }
    }

    pub fn state(&self) -> &SimulationState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut SimulationState {
        &mut self.state
    }

    pub fn update(&mut self, delta: f32) -> u32 {
        self.state.timestep.accumulate(delta);

        let mut ticks_run = 0;
        while self.state.timestep.consume_tick() {
            self.state.store_physics_snapshot();
            (self.tick_fn)(&mut self.state);
            self.state.physics.step();
            crate::physics::PhysicsSync::sync_physics_to_world(
                &self.state.physics,
                &mut self.state.world,
            );
            self.state.world.advance_tick();
            ticks_run += 1;
        }

        ticks_run
    }

    pub fn interpolation_alpha(&self) -> f32 {
        self.state.timestep.alpha()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_timestep_integer_accumulation() {
        let mut ts = FixedTimestep::new(128);
        // 128 tick = 7812.5us per tick, but integer division gives 7812
        assert_eq!(ts.tick_interval_us(), 7812);

        // Accumulate ~16ms (one 60fps frame) = 16000us
        ts.accumulate_us(16000);
        // Should get 2 ticks: 16000 / 7812 = 2.048
        assert!(ts.consume_tick());
        assert!(ts.consume_tick());
        assert!(!ts.consume_tick());
    }

    #[test]
    fn fixed_timestep_no_drift() {
        let mut ts = FixedTimestep::new(128);
        // Simulate 0.25 second of frames at 60fps (within 250ms cap)
        let mut total_ticks = 0;
        for _ in 0..15 {
            ts.accumulate_us(16667); // ~16.667ms per frame
            while ts.consume_tick() {
                total_ticks += 1;
            }
        }
        // 15 * 16667 = 250_005us, / 7812 = ~32 ticks
        assert!(total_ticks >= 31 && total_ticks <= 33, "got {} ticks", total_ticks);
    }

    #[test]
    fn spiral_of_death_prevention() {
        let mut ts = FixedTimestep::new(128);
        // Accumulate a huge delta (1 second)
        ts.accumulate(1.0);
        // Should be capped to 250ms = ~32 ticks max
        let mut count = 0;
        while ts.consume_tick() {
            count += 1;
        }
        assert!(count <= 33, "too many ticks: {}", count);
    }

    #[test]
    fn simulation_loop_ticks() {
        let mut tick_count = 0u32;
        let mut sim = SimulationLoop::new(60, 128, |_state| {
            tick_count += 1;
        });

        sim.update(1.0 / 30.0);
        assert_eq!(tick_count, 2);
    }
}
