use rapier3d::dynamics::{ImpulseJointSet, IslandManager, MultibodyJointSet, RigidBodySet};
use rapier3d::geometry::ColliderSet;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct PhysicsSnapshot {
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    pub islands: IslandManager,
    pub impulse_joints: ImpulseJointSet,
    pub multibody_joints: MultibodyJointSet,
}

impl PhysicsSnapshot {
    pub fn empty() -> Self {
        Self {
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            islands: IslandManager::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
        }
    }
}

pub struct PhysicsHistory {
    snapshots: Vec<Option<PhysicsSnapshot>>,
    ticks: Vec<u32>,
    capacity: usize,
}

impl PhysicsHistory {
    pub fn new(capacity: usize) -> Self {
        Self {
            snapshots: (0..capacity).map(|_| None).collect(),
            ticks: vec![u32::MAX; capacity],
            capacity,
        }
    }

    pub fn push(&mut self, tick: u32, snapshot: PhysicsSnapshot) {
        let index = (tick as usize) % self.capacity;
        self.snapshots[index] = Some(snapshot);
        self.ticks[index] = tick;
    }

    pub fn get(&self, tick: u32) -> Option<&PhysicsSnapshot> {
        let index = (tick as usize) % self.capacity;
        if self.ticks[index] == tick {
            self.snapshots[index].as_ref()
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        for slot in &mut self.snapshots {
            *slot = None;
        }
        for tick in &mut self.ticks {
            *tick = u32::MAX;
        }
    }

    pub fn latest_before(&self, tick: u32) -> Option<(u32, &PhysicsSnapshot)> {
        let mut best: Option<(u32, &PhysicsSnapshot)> = None;

        for i in 0..self.capacity {
            if self.ticks[i] < tick {
                if let Some(snap) = &self.snapshots[i] {
                    match best {
                        None => best = Some((self.ticks[i], snap)),
                        Some((best_tick, _)) if self.ticks[i] > best_tick => {
                            best = Some((self.ticks[i], snap));
                        }
                        _ => {}
                    }
                }
            }
        }

        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_o1_lookup() {
        let mut history = PhysicsHistory::new(64);

        for tick in 0..100u32 {
            history.push(tick, PhysicsSnapshot::empty());
        }

        assert!(history.get(50).is_some());
        assert!(history.get(30).is_none());
    }

    #[test]
    fn latest_before() {
        let mut history = PhysicsHistory::new(64);

        history.push(10, PhysicsSnapshot::empty());
        history.push(20, PhysicsSnapshot::empty());
        history.push(30, PhysicsSnapshot::empty());

        let (tick, _) = history.latest_before(25).unwrap();
        assert_eq!(tick, 20);
    }
}
