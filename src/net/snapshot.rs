//! Snapshot System
//!
//! Manages discrete world-state snapshots generated at fixed tick intervals.
//! Provides the foundation for server-authoritative state synchronization.
//!
//! # Architecture
//! ```text
//! Server Tick Loop:
//! ┌──────────────────────────────────────────────────────────┐
//! │  T=0        T=1        T=2        T=3        T=4         │
//! │   │          │          │          │          │          │
//! │   ▼          ▼          ▼          ▼          ▼          │
//! │  [S0]  →   [S1]  →   [S2]  →   [S3]  →   [S4]            │
//! │             │                    │                        │
//! │             └──────┬─────────────┘                        │
//! │                    ▼                                      │
//! │            Network Transmission                           │
//! └──────────────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;

use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

use super::protocol::{EntityState, WorldSnapshot};

/// Entity type identifiers
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

/// High-level entity representation for server simulation.
///
/// This is the authoritative state that exists on the server.
/// It gets serialized to `EntityState` for network transmission.
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
    /// Tracks if entity state changed this tick (for delta compression)
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

    /// Creates a player entity at the given spawn position
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

    /// Convert to network-optimized EntityState
    pub fn to_network_state(&self) -> EntityState {
        let mut state = EntityState::new(self.id, self.entity_type as u8);
        state.position = self.position.into();
        state.encode_velocity(self.velocity.into());

        let quat = self.orientation;
        state.encode_orientation([quat.x, quat.y, quat.z, quat.w]);

        state.animation_state = self.animation_state;
        // Normalize animation time to 0-255 (assuming animations loop at 1.0)
        state.animation_frame = ((self.animation_time.fract() * 255.0) as u8);
        state.flags = self.flags;

        state
    }

    /// Update from network state (for client-side reconstruction)
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

/// World state container for server-side simulation.
///
/// Maintains all entities and generates snapshots at each tick.
#[derive(Debug)]
pub struct World {
    /// Current simulation tick
    tick: u32,
    /// Server start time in milliseconds
    start_time_ms: u64,
    /// All entities indexed by ID
    entities: HashMap<u32, Entity>,
    /// Next available entity ID
    next_entity_id: u32,
    /// Entity IDs that were removed this tick (for delta updates)
    removed_entities: Vec<u32>,
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

    /// Current simulation tick
    pub fn tick(&self) -> u32 {
        self.tick
    }

    /// Advance to the next tick
    pub fn advance_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        self.removed_entities.clear();

        // Clear dirty flags
        for entity in self.entities.values_mut() {
            entity.dirty = false;
        }
    }

    /// Get server time in milliseconds since start
    pub fn server_time_ms(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        now.saturating_sub(self.start_time_ms)
    }

    /// Spawn a new entity and return its ID
    pub fn spawn_entity(&mut self, entity_type: EntityType) -> u32 {
        let id = self.next_entity_id;
        self.next_entity_id += 1;

        let entity = Entity::new(id, entity_type);
        self.entities.insert(id, entity);
        id
    }

    /// Spawn a player entity at the given position
    pub fn spawn_player(&mut self, spawn_position: Vec3) -> u32 {
        let id = self.next_entity_id;
        self.next_entity_id += 1;

        let entity = Entity::new_player(id, spawn_position);
        self.entities.insert(id, entity);
        id
    }

    /// Remove an entity
    pub fn despawn_entity(&mut self, id: u32) -> Option<Entity> {
        let entity = self.entities.remove(&id);
        if entity.is_some() {
            self.removed_entities.push(id);
        }
        entity
    }

    /// Get entity by ID
    pub fn get_entity(&self, id: u32) -> Option<&Entity> {
        self.entities.get(&id)
    }

    /// Get mutable entity by ID
    pub fn get_entity_mut(&mut self, id: u32) -> Option<&mut Entity> {
        self.entities.get_mut(&id)
    }

    /// Iterate over all entities
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    /// Iterate over all entities mutably
    pub fn entities_mut(&mut self) -> impl Iterator<Item = &mut Entity> {
        self.entities.values_mut()
    }

    /// Generate a full world snapshot for network transmission
    pub fn generate_snapshot(&self, last_command_ack: u32) -> WorldSnapshot {
        let mut snapshot = WorldSnapshot::new(self.tick, self.server_time_ms());
        snapshot.last_command_ack = last_command_ack;

        for entity in self.entities.values() {
            snapshot.entities.push(entity.to_network_state());
        }

        snapshot
    }

    /// Generate a delta snapshot containing only changed entities.
    ///
    /// Used for bandwidth optimization when clients already have baseline state.
    pub fn generate_delta_snapshot(&self, last_command_ack: u32) -> WorldSnapshot {
        let mut snapshot = WorldSnapshot::new(self.tick, self.server_time_ms());
        snapshot.last_command_ack = last_command_ack;

        for entity in self.entities.values() {
            if entity.dirty {
                snapshot.entities.push(entity.to_network_state());
            }
        }

        snapshot
    }

    /// Get IDs of entities removed this tick
    pub fn removed_entities(&self) -> &[u32] {
        &self.removed_entities
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot history buffer for maintaining recent world states.
///
/// Used on both server (for lag compensation) and client (for interpolation source).
#[derive(Debug)]
pub struct SnapshotBuffer {
    /// Ring buffer of snapshots
    snapshots: Vec<Option<WorldSnapshot>>,
    /// Current write position
    write_pos: usize,
    /// Capacity of the buffer
    capacity: usize,
}

impl SnapshotBuffer {
    /// Create a new snapshot buffer with the given capacity.
    ///
    /// # Arguments
    /// * `capacity` - Number of snapshots to store. Should be at least
    ///   `(max_latency_ms / tick_interval_ms) + interpolation_ticks`
    pub fn new(capacity: usize) -> Self {
        Self {
            snapshots: (0..capacity).map(|_| None).collect(),
            write_pos: 0,
            capacity,
        }
    }

    /// Store a snapshot
    pub fn push(&mut self, snapshot: WorldSnapshot) {
        self.snapshots[self.write_pos] = Some(snapshot);
        self.write_pos = (self.write_pos + 1) % self.capacity;
    }

    /// Find a snapshot by tick number
    pub fn get_by_tick(&self, tick: u32) -> Option<&WorldSnapshot> {
        self.snapshots
            .iter()
            .find_map(|s| s.as_ref().filter(|snap| snap.tick == tick))
    }

    /// Get the two most recent snapshots for interpolation.
    ///
    /// Returns (older, newer) snapshots if available.
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

    /// Get the most recent snapshot
    pub fn latest(&self) -> Option<&WorldSnapshot> {
        self.snapshots
            .iter()
            .filter_map(|s| s.as_ref())
            .max_by_key(|s| s.tick)
    }

    /// Get snapshot at specific offset from latest (0 = latest, 1 = one before, etc.)
    pub fn get_relative(&self, offset: usize) -> Option<&WorldSnapshot> {
        let mut snapshots: Vec<&WorldSnapshot> =
            self.snapshots.iter().filter_map(|s| s.as_ref()).collect();

        snapshots.sort_by_key(|s| std::cmp::Reverse(s.tick));

        snapshots.get(offset).copied()
    }

    /// Clear all snapshots
    pub fn clear(&mut self) {
        for slot in &mut self.snapshots {
            *slot = None;
        }
    }

    /// Number of valid snapshots in buffer
    pub fn len(&self) -> usize {
        self.snapshots.iter().filter(|s| s.is_some()).count()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
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
    fn test_snapshot_buffer() {
        let mut buffer = SnapshotBuffer::new(4);

        for tick in 0..6 {
            buffer.push(WorldSnapshot::new(tick, tick as u64 * 50));
        }

        // Should have ticks 2, 3, 4, 5 (oldest ones overwritten)
        assert!(buffer.get_by_tick(0).is_none());
        assert!(buffer.get_by_tick(1).is_none());
        assert!(buffer.get_by_tick(2).is_some());
        assert!(buffer.get_by_tick(5).is_some());

        let latest = buffer.latest().unwrap();
        assert_eq!(latest.tick, 5);
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
}
