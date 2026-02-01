use crate::physics::{PhysicsHistory, PhysicsWorld};
use crate::snapshot::World;

pub struct FixedTimestep {
    tick_rate: u32,
    dt: f32,
    accumulator: f32,
}

impl FixedTimestep {
    pub fn new(tick_rate: u32) -> Self {
        Self {
            tick_rate,
            dt: 1.0 / tick_rate as f32,
            accumulator: 0.0,
        }
    }

    pub fn tick_rate(&self) -> u32 {
        self.tick_rate
    }

    pub fn dt(&self) -> f32 {
        self.dt
    }

    pub fn accumulate(&mut self, delta: f32) {
        self.accumulator += delta.min(0.25);
    }

    pub fn should_tick(&self) -> bool {
        self.accumulator >= self.dt
    }

    pub fn consume_tick(&mut self) -> bool {
        if self.accumulator >= self.dt {
            self.accumulator -= self.dt;
            true
        } else {
            false
        }
    }

    pub fn alpha(&self) -> f32 {
        self.accumulator / self.dt
    }

    pub fn reset(&mut self) {
        self.accumulator = 0.0;
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
    fn fixed_timestep_accumulation() {
        let mut ts = FixedTimestep::new(60);

        ts.accumulate(1.0 / 30.0);
        assert!(ts.should_tick());
        assert!(ts.consume_tick());
        assert!(ts.consume_tick());
        assert!(!ts.consume_tick());
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
