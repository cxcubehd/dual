use std::collections::HashMap;

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

use super::protocol::{EntityState, WorldSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum EntityType {
    Player = 0,
    Projectile = 1,
    Item = 2,
    Static = 3,
    Trigger = 4,
}

impl From<u8> for EntityType {
    fn from(value: u8) -> Self {
        match value {
            0 => EntityType::Player,
            1 => EntityType::Projectile,
            2 => EntityType::Item,
            3 => EntityType::Static,
            4 => EntityType::Trigger,
            _ => EntityType::Static,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: u32,
    pub entity_type: EntityType,
    pub position: Vec3,
    pub velocity: Vec3,
    pub orientation: Quat,
    pub animation_state: u8,
    pub animation_time: f32,
    pub flags: u16,
    pub dirty: bool,
}

impl Entity {
    pub fn new(id: u32, entity_type: EntityType) -> Self {
        Self {
            id,
            entity_type,
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            animation_state: 0,
            animation_time: 0.0,
            flags: 0,
            dirty: true,
        }
    }

    pub fn new_player(id: u32, spawn_position: Vec3) -> Self {
        Self {
            id,
            entity_type: EntityType::Player,
            position: spawn_position,
            velocity: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            animation_state: 0,
            animation_time: 0.0,
            flags: 0,
            dirty: true,
        }
    }

    pub fn to_network_state(&self) -> EntityState {
        let mut state = EntityState::new(self.id, self.entity_type as u8);
        state.position = self.position.into();
        state.encode_velocity(self.velocity.into());

        let quat = self.orientation;
        state.encode_orientation([quat.x, quat.y, quat.z, quat.w]);

        state.animation_state = self.animation_state;
        state.animation_frame = (self.animation_time.fract() * 255.0) as u8;
        state.flags = self.flags;

        state
    }

    pub fn from_network_state(state: &EntityState) -> Self {
        let vel = state.decode_velocity();
        let quat = state.decode_orientation();

        Self {
            id: state.entity_id,
            entity_type: EntityType::from(state.entity_type),
            position: Vec3::from(state.position),
            velocity: Vec3::from(vel),
            orientation: Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]).normalize(),
            animation_state: state.animation_state,
            animation_time: state.animation_frame as f32 / 255.0,
            flags: state.flags,
            dirty: false,
        }
    }
}

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
            start_time_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            entities: HashMap::new(),
            next_entity_id: 1,
            removed_entities: Vec::new(),
        }
    }

    pub fn tick(&self) -> u32 {
        self.tick
    }

    pub fn advance_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        self.removed_entities.clear();

        for entity in self.entities.values_mut() {
            entity.dirty = false;
        }
    }

    pub fn server_time_ms(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        now.saturating_sub(self.start_time_ms)
    }

    pub fn spawn_entity(&mut self, entity_type: EntityType) -> u32 {
        let id = self.next_entity_id;
        self.next_entity_id += 1;

        let entity = Entity::new(id, entity_type);
        self.entities.insert(id, entity);
        id
    }

    pub fn spawn_player(&mut self, spawn_position: Vec3) -> u32 {
        let id = self.next_entity_id;
        self.next_entity_id += 1;

        let entity = Entity::new_player(id, spawn_position);
        self.entities.insert(id, entity);
        id
    }

    pub fn despawn_entity(&mut self, id: u32) -> Option<Entity> {
        let entity = self.entities.remove(&id);
        if entity.is_some() {
            self.removed_entities.push(id);
        }
        entity
    }

    pub fn get_entity(&self, id: u32) -> Option<&Entity> {
        self.entities.get(&id)
    }

    pub fn get_entity_mut(&mut self, id: u32) -> Option<&mut Entity> {
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

    pub fn generate_snapshot(&self, last_command_ack: u32) -> WorldSnapshot {
        let mut snapshot = WorldSnapshot::new(self.tick, self.server_time_ms());
        snapshot.last_command_ack = last_command_ack;

        for entity in self.entities.values() {
            snapshot.entities.push(entity.to_network_state());
        }

        snapshot.removed_entity_ids = self.removed_entities.clone();

        snapshot
    }

    pub fn generate_delta_snapshot(&self, last_command_ack: u32) -> WorldSnapshot {
        let mut snapshot = WorldSnapshot::new(self.tick, self.server_time_ms());
        snapshot.last_command_ack = last_command_ack;

        for entity in self.entities.values() {
            if entity.dirty {
                snapshot.entities.push(entity.to_network_state());
            }
        }

        snapshot.removed_entity_ids = self.removed_entities.clone();

        snapshot
    }

    pub fn generate_delta_from_baseline(
        &self,
        baseline: &WorldSnapshot,
        last_command_ack: u32,
    ) -> WorldSnapshot {
        let mut snapshot =
            WorldSnapshot::new_delta(self.tick, self.server_time_ms(), baseline.tick);
        snapshot.last_command_ack = last_command_ack;

        let baseline_entities: std::collections::HashMap<u32, &EntityState> =
            baseline.entities.iter().map(|e| (e.entity_id, e)).collect();

        for entity in self.entities.values() {
            let current_state = entity.to_network_state();

            if let Some(baseline_state) = baseline_entities.get(&entity.id) {
                if !entity_states_equal(&current_state, baseline_state) {
                    snapshot.entities.push(current_state);
                }
            } else {
                snapshot.entities.push(current_state);
            }
        }

        for baseline_entity in &baseline.entities {
            if !self.entities.contains_key(&baseline_entity.entity_id) {
                snapshot.removed_entity_ids.push(baseline_entity.entity_id);
            }
        }

        snapshot
    }

    pub fn removed_entities(&self) -> &[u32] {
        &self.removed_entities
    }
}

#[derive(Debug)]
pub struct SnapshotBuffer {
    snapshots: Vec<Option<WorldSnapshot>>,
    capacity: usize,
}

impl SnapshotBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            snapshots: (0..capacity).map(|_| None).collect(),
            capacity,
        }
    }

    pub fn push(&mut self, snapshot: WorldSnapshot) {
        let index = (snapshot.tick as usize) % self.capacity;
        self.snapshots[index] = Some(snapshot);
    }

    pub fn get_by_tick(&self, tick: u32) -> Option<&WorldSnapshot> {
        let index = (tick as usize) % self.capacity;
        self.snapshots[index]
            .as_ref()
            .filter(|snap| snap.tick == tick)
    }

    pub fn get_interpolation_pair(&self) -> Option<(&WorldSnapshot, &WorldSnapshot)> {
        let mut snapshots: Vec<&WorldSnapshot> =
            self.snapshots.iter().filter_map(|s| s.as_ref()).collect();

        snapshots.sort_by_key(|s| s.tick);

        if snapshots.len() >= 2 {
            let len = snapshots.len();
            Some((snapshots[len - 2], snapshots[len - 1]))
        } else {
            None
        }
    }

    pub fn latest(&self) -> Option<&WorldSnapshot> {
        self.snapshots
            .iter()
            .filter_map(|s| s.as_ref())
            .max_by_key(|s| s.tick)
    }

    pub fn clear(&mut self) {
        for slot in &mut self.snapshots {
            *slot = None;
        }
    }

    pub fn len(&self) -> usize {
        self.snapshots.iter().filter(|s| s.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn entity_states_equal(a: &EntityState, b: &EntityState) -> bool {
    a.entity_id == b.entity_id
        && a.entity_type == b.entity_type
        && a.position == b.position
        && a.velocity == b.velocity
        && a.orientation == b.orientation
        && a.animation_state == b.animation_state
        && a.flags == b.flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_network_roundtrip() {
        let mut entity = Entity::new_player(42, Vec3::new(10.0, 5.0, -3.0));
        entity.velocity = Vec3::new(2.5, -1.0, 0.5);
        entity.orientation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);

        let network_state = entity.to_network_state();
        let reconstructed = Entity::from_network_state(&network_state);

        assert_eq!(entity.id, reconstructed.id);
        assert!((entity.position - reconstructed.position).length() < 0.001);
        assert!((entity.velocity - reconstructed.velocity).length() < 0.02);
    }

    #[test]
    fn test_snapshot_buffer_o1_lookup() {
        let mut buffer = SnapshotBuffer::new(64);

        for tick in 0..100 {
            buffer.push(WorldSnapshot::new(tick, tick as u64 * 50));
        }

        assert!(buffer.get_by_tick(50).is_some());
        assert_eq!(buffer.get_by_tick(50).unwrap().tick, 50);
        assert!(buffer.get_by_tick(30).is_none());
    }

    #[test]
    fn test_world_snapshot_generation() {
        let mut world = World::new();

        let player_id = world.spawn_player(Vec3::new(0.0, 1.0, 0.0));
        world.spawn_entity(EntityType::Item);

        let snapshot = world.generate_snapshot(0);

        assert_eq!(snapshot.tick, 0);
        assert_eq!(snapshot.entities.len(), 2);
        assert!(snapshot.entities.iter().any(|e| e.entity_id == player_id));
    }

    #[test]
    fn test_delta_snapshot_only_changed_entities() {
        let mut world = World::new();

        let player1 = world.spawn_player(Vec3::new(0.0, 1.0, 0.0));
        let _player2 = world.spawn_player(Vec3::new(5.0, 1.0, 0.0));

        let baseline = world.generate_snapshot(0);
        assert_eq!(baseline.entities.len(), 2);

        world.advance_tick();

        if let Some(entity) = world.get_entity_mut(player1) {
            entity.position = Vec3::new(1.0, 1.0, 0.0);
            entity.dirty = true;
        }

        let delta = world.generate_delta_from_baseline(&baseline, 0);

        assert!(delta.is_delta);
        assert_eq!(delta.baseline_tick, 0);
        assert_eq!(delta.entities.len(), 1);
        assert_eq!(delta.entities[0].entity_id, player1);
        assert!(delta.removed_entity_ids.is_empty());
    }

    #[test]
    fn test_delta_snapshot_includes_removed_entities() {
        let mut world = World::new();

        let _player1 = world.spawn_player(Vec3::new(0.0, 1.0, 0.0));
        let player2 = world.spawn_player(Vec3::new(5.0, 1.0, 0.0));

        let baseline = world.generate_snapshot(0);

        world.advance_tick();
        world.despawn_entity(player2);

        let delta = world.generate_delta_from_baseline(&baseline, 0);

        assert!(delta.is_delta);
        assert_eq!(delta.entities.len(), 0);
        assert_eq!(delta.removed_entity_ids.len(), 1);
        assert_eq!(delta.removed_entity_ids[0], player2);
    }

    #[test]
    fn test_delta_snapshot_new_entities_included() {
        let mut world = World::new();

        let _player1 = world.spawn_player(Vec3::new(0.0, 1.0, 0.0));

        let baseline = world.generate_snapshot(0);
        assert_eq!(baseline.entities.len(), 1);

        world.advance_tick();
        let player2 = world.spawn_player(Vec3::new(5.0, 1.0, 0.0));

        let delta = world.generate_delta_from_baseline(&baseline, 0);

        assert!(delta.is_delta);
        assert_eq!(delta.entities.len(), 1);
        assert_eq!(delta.entities[0].entity_id, player2);
    }
}
