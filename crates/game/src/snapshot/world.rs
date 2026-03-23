use std::collections::HashMap;

use glam::Vec3;

use super::delta::DeltaEncoder;
use super::entity::{Entity, EntityHandle, EntityKind, EntityState};
use super::types::WorldSnapshot;

#[derive(Debug)]
pub struct World {
    tick: u32,
    start_time_ms: u64,
    entities: HashMap<u32, Entity>,
    next_entity_id: u32,
    removed_entities: Vec<u32>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        Self {
            tick: 0,
            start_time_ms: current_time_ms(),
            entities: HashMap::new(),
            next_entity_id: 1,
            removed_entities: Vec::new(),
        }
    }

    pub fn tick(&self) -> u32 {
        self.tick
    }

    pub fn set_tick(&mut self, tick: u32) {
        self.tick = tick;
    }

    pub fn advance_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        self.removed_entities.clear();
        for entity in self.entities.values_mut() {
            entity.dirty = false;
        }
    }

    pub fn server_time_ms(&self) -> u64 {
        current_time_ms().saturating_sub(self.start_time_ms)
    }

    // -----------------------------------------------------------------------
    // Entity management
    // -----------------------------------------------------------------------

    pub fn spawn(&mut self, kind: EntityKind) -> EntityHandle {
        let id = self.allocate_id();
        let entity = Entity::new(id, kind);
        self.entities.insert(id, entity);
        EntityHandle(id)
    }

    pub fn spawn_player(&mut self, spawn_position: Vec3) -> EntityHandle {
        let id = self.allocate_id();
        let entity = Entity::player(id, spawn_position);
        self.entities.insert(id, entity);
        EntityHandle(id)
    }

    pub fn spawn_with_id(&mut self, id: u32, kind: EntityKind) -> EntityHandle {
        let entity = Entity::new(id, kind);
        self.entities.insert(id, entity);
        if id >= self.next_entity_id {
            self.next_entity_id = id + 1;
        }
        EntityHandle(id)
    }

    pub fn despawn(&mut self, handle: EntityHandle) -> Option<Entity> {
        let entity = self.entities.remove(&handle.0);
        if entity.is_some() {
            self.removed_entities.push(handle.0);
        }
        entity
    }

    pub fn get(&self, handle: EntityHandle) -> Option<&Entity> {
        self.entities.get(&handle.0)
    }

    pub fn get_mut(&mut self, handle: EntityHandle) -> Option<&mut Entity> {
        self.entities.get_mut(&handle.0)
    }

    pub fn get_by_id(&self, id: u32) -> Option<&Entity> {
        self.entities.get(&id)
    }

    pub fn get_by_id_mut(&mut self, id: u32) -> Option<&mut Entity> {
        self.entities.get_mut(&id)
    }

    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    pub fn entities_mut(&mut self) -> impl Iterator<Item = &mut Entity> {
        self.entities.values_mut()
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn removed_entities(&self) -> &[u32] {
        &self.removed_entities
    }

    // -----------------------------------------------------------------------
    // Snapshot generation
    // -----------------------------------------------------------------------

    /// Collect all entity states into a flat Vec (used as baseline for delta).
    pub fn collect_entity_states(&self) -> Vec<EntityState> {
        self.entities.values().map(Entity::to_entity_state).collect()
    }

    /// Generate a full (non-delta) snapshot.
    pub fn snapshot(&self, last_command_ack: u32) -> WorldSnapshot {
        let entities = self.collect_entity_states();
        WorldSnapshot {
            tick: self.tick,
            server_time_ms: self.server_time_ms(),
            last_command_ack,
            baseline_tick: 0,
            is_delta: false,
            entities,
            deltas: Vec::new(),
            removed_entity_ids: self.removed_entities.clone(),
        }
    }

    /// Generate a delta-compressed snapshot relative to a baseline.
    pub fn delta_snapshot(
        &self,
        baseline: &WorldSnapshot,
        last_command_ack: u32,
    ) -> WorldSnapshot {
        let current = self.collect_entity_states();
        let (deltas, despawned) = DeltaEncoder::encode(&baseline.entities, &current);

        WorldSnapshot {
            tick: self.tick,
            server_time_ms: self.server_time_ms(),
            last_command_ack,
            baseline_tick: baseline.tick,
            is_delta: true,
            entities: Vec::new(),
            deltas,
            removed_entity_ids: despawned,
        }
    }

    fn allocate_id(&mut self) -> u32 {
        let id = self.next_entity_id;
        self.next_entity_id += 1;
        id
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_generation() {
        let mut world = World::new();
        let player = world.spawn_player(Vec3::new(0.0, 1.0, 0.0));
        world.spawn(EntityKind::Item);

        let snapshot = world.snapshot(0);

        assert_eq!(snapshot.tick, 0);
        assert_eq!(snapshot.entities.len(), 2);
        assert!(snapshot.entities.iter().any(|e| e.entity_id == player.id()));
    }

    #[test]
    fn delta_only_changed() {
        let mut world = World::new();
        let player1 = world.spawn_player(Vec3::new(0.0, 1.0, 0.0));
        let _player2 = world.spawn_player(Vec3::new(5.0, 1.0, 0.0));

        let baseline = world.snapshot(0);
        world.advance_tick();

        if let Some(entity) = world.get_mut(player1) {
            entity.position = Vec3::new(1.0, 1.0, 0.0);
            entity.dirty = true;
        }

        let delta = world.delta_snapshot(&baseline, 0);
        assert!(delta.is_delta);
        assert_eq!(delta.baseline_tick, 0);
        assert_eq!(delta.deltas.len(), 1);
        assert_eq!(delta.deltas[0].net_id, player1.id());
    }

    #[test]
    fn delta_includes_removed() {
        let mut world = World::new();
        let _player1 = world.spawn_player(Vec3::new(0.0, 1.0, 0.0));
        let player2 = world.spawn_player(Vec3::new(5.0, 1.0, 0.0));

        let baseline = world.snapshot(0);
        world.advance_tick();
        world.despawn(player2);

        let delta = world.delta_snapshot(&baseline, 0);
        assert!(delta.is_delta);
        assert_eq!(delta.removed_entity_ids.len(), 1);
        assert_eq!(delta.removed_entity_ids[0], player2.id());
    }
}
