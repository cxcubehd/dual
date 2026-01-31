use std::collections::HashMap;

use glam::{Quat, Vec3};

use dual::{Entity, EntityState, EntityType, WorldSnapshot};

pub const DEFAULT_INTERPOLATION_DELAY_MS: f64 = 100.0;

#[derive(Debug, Clone)]
pub struct InterpolationConfig {
    pub target_delay_ms: f64,
    pub min_buffer_snapshots: usize,
    pub max_buffer_snapshots: usize,
    pub time_correction_rate: f64,
    pub extrapolation_limit_ms: f64,
}

impl Default for InterpolationConfig {
    fn default() -> Self {
        Self {
            target_delay_ms: DEFAULT_INTERPOLATION_DELAY_MS,
            min_buffer_snapshots: 3,
            max_buffer_snapshots: 64,
            time_correction_rate: 0.1,
            extrapolation_limit_ms: 250.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InterpolatedEntity {
    pub id: u32,
    pub entity_type: EntityType,
    pub position: Vec3,
    pub velocity: Vec3,
    pub orientation: Quat,
    pub animation_state: u8,
    pub animation_time: f32,
    pub flags: u16,
}

impl From<&Entity> for InterpolatedEntity {
    fn from(entity: &Entity) -> Self {
        Self {
            id: entity.id,
            entity_type: entity.entity_type,
            position: entity.position,
            velocity: entity.velocity,
            orientation: entity.orientation,
            animation_state: entity.animation_state,
            animation_time: entity.animation_time,
            flags: entity.flags,
        }
    }
}

impl InterpolatedEntity {
    pub fn from_network_state(state: &EntityState) -> Self {
        let entity = Entity::from_network_state(state);
        Self::from(&entity)
    }
}

#[derive(Debug)]
struct TimedSnapshot {
    snapshot: WorldSnapshot,
    server_time_ms: f64,
}

#[derive(Debug)]
pub struct InterpolationEngine {
    config: InterpolationConfig,
    snapshots: Vec<TimedSnapshot>,
    server_time_offset_ms: f64,
    render_time_ms: f64,
    interpolated_entities: HashMap<u32, InterpolatedEntity>,
    known_entities: HashMap<u32, EntityState>,
    ready: bool,
    latest_server_tick: u32,
    last_snapshot_time_ms: f64,
    is_extrapolating: bool,
}

impl InterpolationEngine {
    pub fn new(config: InterpolationConfig) -> Self {
        Self {
            config,
            snapshots: Vec::new(),
            server_time_offset_ms: 0.0,
            render_time_ms: 0.0,
            interpolated_entities: HashMap::new(),
            known_entities: HashMap::new(),
            ready: false,
            latest_server_tick: 0,
            last_snapshot_time_ms: 0.0,
            is_extrapolating: false,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(InterpolationConfig::default())
    }

    pub fn push_snapshot(&mut self, snapshot: WorldSnapshot) {
        let server_time = snapshot.server_time_ms as f64;

        if snapshot.tick > self.latest_server_tick {
            self.latest_server_tick = snapshot.tick;
        }

        self.last_snapshot_time_ms = current_time_ms();
        self.is_extrapolating = false;

        let local_time = current_time_ms();
        let new_offset = server_time - local_time;

        if self.snapshots.is_empty() {
            self.server_time_offset_ms = new_offset;
            self.render_time_ms = server_time - self.config.target_delay_ms;
        } else {
            let correction =
                (new_offset - self.server_time_offset_ms) * self.config.time_correction_rate;
            self.server_time_offset_ms += correction;
        }

        let full_snapshot = self.expand_snapshot(snapshot);

        let timed = TimedSnapshot {
            snapshot: full_snapshot,
            server_time_ms: server_time,
        };

        let insert_pos = self
            .snapshots
            .iter()
            .position(|s| s.server_time_ms > server_time)
            .unwrap_or(self.snapshots.len());
        self.snapshots.insert(insert_pos, timed);

        while self.snapshots.len() > self.config.max_buffer_snapshots {
            self.snapshots.remove(0);
        }

        if !self.ready && self.snapshots.len() >= self.config.min_buffer_snapshots {
            self.ready = true;
        }
    }

    fn expand_snapshot(&mut self, snapshot: WorldSnapshot) -> WorldSnapshot {
        if !snapshot.is_delta {
            for entity in &snapshot.entities {
                self.known_entities.insert(entity.entity_id, entity.clone());
            }
            for removed_id in &snapshot.removed_entity_ids {
                self.known_entities.remove(removed_id);
            }
            return snapshot;
        }

        for entity in &snapshot.entities {
            self.known_entities.insert(entity.entity_id, entity.clone());
        }

        for removed_id in &snapshot.removed_entity_ids {
            self.known_entities.remove(removed_id);
        }

        let mut full_snapshot = WorldSnapshot::new(snapshot.tick, snapshot.server_time_ms);
        full_snapshot.last_command_ack = snapshot.last_command_ack;
        full_snapshot.entities = self.known_entities.values().cloned().collect();

        full_snapshot
    }

    pub fn update(&mut self, delta_time: f32) {
        if !self.ready || self.snapshots.is_empty() {
            return;
        }

        let local_time = current_time_ms();
        let target_render_time =
            local_time + self.server_time_offset_ms - self.config.target_delay_ms;

        let time_diff = target_render_time - self.render_time_ms;
        let max_correction = (delta_time as f64 * 1000.0) * 1.5;
        let correction = time_diff.clamp(-max_correction, max_correction);
        self.render_time_ms +=
            (delta_time as f64 * 1000.0) + correction * self.config.time_correction_rate;

        self.cleanup_old_snapshots();

        if self.snapshots.len() < 2 {
            self.extrapolate_from_latest(delta_time);
            return;
        }

        if let Some((from_idx, to_idx, t)) = self.find_interpolation_indices() {
            self.is_extrapolating = t > 1.0;
            self.interpolate_at_indices(from_idx, to_idx, t);
        } else {
            self.extrapolate_from_latest(delta_time);
        }
    }

    fn extrapolate_from_latest(&mut self, delta_time: f32) {
        let time_since_last_snapshot = current_time_ms() - self.last_snapshot_time_ms;

        if time_since_last_snapshot > self.config.extrapolation_limit_ms {
            return;
        }

        self.is_extrapolating = true;

        if let Some(latest) = self.snapshots.last() {
            for state in &latest.snapshot.entities {
                let entity_id = state.entity_id;
                let velocity = Vec3::from(state.decode_velocity());

                if let Some(existing) = self.interpolated_entities.get_mut(&entity_id) {
                    existing.position += velocity * delta_time;
                } else {
                    let mut entity = InterpolatedEntity::from_network_state(state);
                    entity.position += velocity * delta_time;
                    self.interpolated_entities.insert(entity_id, entity);
                }
            }
        }
    }

    fn find_interpolation_indices(&self) -> Option<(usize, usize, f32)> {
        if self.snapshots.len() < 2 {
            return None;
        }

        for i in 0..self.snapshots.len() - 1 {
            let from = &self.snapshots[i];
            let to = &self.snapshots[i + 1];

            if from.server_time_ms <= self.render_time_ms
                && to.server_time_ms >= self.render_time_ms
            {
                let duration = to.server_time_ms - from.server_time_ms;
                let t = if duration > 0.0 {
                    ((self.render_time_ms - from.server_time_ms) / duration) as f32
                } else {
                    0.0
                };
                return Some((i, i + 1, t.clamp(0.0, 1.0)));
            }
        }

        if self.render_time_ms < self.snapshots[0].server_time_ms {
            return Some((0, 0, 0.0));
        }

        let len = self.snapshots.len();
        let prev = &self.snapshots[len - 2];
        let last = &self.snapshots[len - 1];
        let duration = last.server_time_ms - prev.server_time_ms;
        let t = if duration > 0.0 {
            ((self.render_time_ms - prev.server_time_ms) / duration) as f32
        } else {
            1.0
        };
        Some((len - 2, len - 1, t.clamp(0.0, 2.0).min(1.5)))
    }

    fn interpolate_at_indices(&mut self, from_idx: usize, to_idx: usize, t: f32) {
        let from = &self.snapshots[from_idx].snapshot;
        let to = &self.snapshots[to_idx].snapshot;

        let to_entities: HashMap<u32, &EntityState> =
            to.entities.iter().map(|e| (e.entity_id, e)).collect();

        self.interpolated_entities.clear();

        for from_state in &from.entities {
            let entity_id = from_state.entity_id;
            let interpolated = if let Some(to_state) = to_entities.get(&entity_id) {
                interpolate_entity_states(from_state, to_state, t)
            } else {
                InterpolatedEntity::from_network_state(from_state)
            };
            self.interpolated_entities.insert(entity_id, interpolated);
        }

        for to_state in &to.entities {
            if !self.interpolated_entities.contains_key(&to_state.entity_id) {
                let interpolated = InterpolatedEntity::from_network_state(to_state);
                self.interpolated_entities
                    .insert(to_state.entity_id, interpolated);
            }
        }
    }

    fn cleanup_old_snapshots(&mut self) {
        let cutoff = self.render_time_ms - 500.0;
        self.snapshots.retain(|s| s.server_time_ms > cutoff);
    }

    pub fn get_entity(&self, entity_id: u32) -> Option<&InterpolatedEntity> {
        self.interpolated_entities.get(&entity_id)
    }

    pub fn entities(&self) -> impl Iterator<Item = &InterpolatedEntity> {
        self.interpolated_entities.values()
    }

    pub fn is_ready(&self) -> bool {
        self.ready
    }

    pub fn reset(&mut self) {
        self.snapshots.clear();
        self.server_time_offset_ms = 0.0;
        self.render_time_ms = 0.0;
        self.interpolated_entities.clear();
        self.known_entities.clear();
        self.ready = false;
        self.latest_server_tick = 0;
        self.last_snapshot_time_ms = 0.0;
        self.is_extrapolating = false;
    }

    pub fn debug_stats(&self) -> InterpolationStats {
        InterpolationStats {
            buffer_size: self.snapshots.len(),
            render_time_ms: self.render_time_ms,
            server_time_offset_ms: self.server_time_offset_ms,
            latest_server_tick: self.latest_server_tick,
            entity_count: self.interpolated_entities.len(),
            is_ready: self.ready,
            is_extrapolating: self.is_extrapolating,
        }
    }
}

fn interpolate_entity_states(from: &EntityState, to: &EntityState, t: f32) -> InterpolatedEntity {
    let from_pos = Vec3::from(from.position);
    let to_pos = Vec3::from(to.position);
    let position = from_pos.lerp(to_pos, t);

    let from_vel = Vec3::from(from.decode_velocity());
    let to_vel = Vec3::from(to.decode_velocity());
    let velocity = from_vel.lerp(to_vel, t);

    let from_quat = decode_quat(from);
    let to_quat = decode_quat(to);
    let orientation = if from_quat.dot(to_quat) < 0.0 {
        from_quat.slerp(-to_quat, t)
    } else {
        from_quat.slerp(to_quat, t)
    };

    let from_anim = from.animation_frame as f32 / 255.0;
    let to_anim = to.animation_frame as f32 / 255.0;
    let animation_time = lerp_wrapped(from_anim, to_anim, t);

    InterpolatedEntity {
        id: from.entity_id,
        entity_type: EntityType::from(from.entity_type),
        position,
        velocity,
        orientation,
        animation_state: if t < 0.5 {
            from.animation_state
        } else {
            to.animation_state
        },
        animation_time,
        flags: if t < 0.5 { from.flags } else { to.flags },
    }
}

fn decode_quat(state: &EntityState) -> Quat {
    let arr = state.decode_orientation();
    Quat::from_xyzw(arr[0], arr[1], arr[2], arr[3]).normalize()
}

fn lerp_wrapped(from: f32, to: f32, t: f32) -> f32 {
    if (to - from).abs() > 0.5 {
        if to < from {
            (from + (to + 1.0 - from) * t) % 1.0
        } else {
            (from + 1.0 + (to - from - 1.0) * t) % 1.0
        }
    } else {
        from + (to - from) * t
    }
}

fn current_time_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
        * 1000.0
}

#[derive(Debug, Clone)]
pub struct InterpolationStats {
    pub buffer_size: usize,
    pub render_time_ms: f64,
    pub server_time_offset_ms: f64,
    pub latest_server_tick: u32,
    pub entity_count: usize,
    pub is_ready: bool,
    pub is_extrapolating: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_snapshot(tick: u32, time_ms: u64, entity_count: usize) -> WorldSnapshot {
        let mut snapshot = WorldSnapshot::new(tick, time_ms);
        for i in 0..entity_count {
            let mut state = EntityState::new(i as u32, 0);
            state.position = [tick as f32 * 10.0 + i as f32, 0.0, 0.0];
            snapshot.entities.push(state);
        }
        snapshot
    }

    #[test]
    fn test_interpolation_engine_initialization() {
        let mut engine = InterpolationEngine::with_defaults();
        assert!(!engine.is_ready());

        engine.push_snapshot(create_test_snapshot(0, 0, 2));
        engine.push_snapshot(create_test_snapshot(1, 16, 2));
        engine.push_snapshot(create_test_snapshot(2, 32, 2));

        assert!(engine.is_ready());
    }

    #[test]
    fn test_lerp_interpolation() {
        let mut from = EntityState::new(1, 0);
        from.position = [0.0, 0.0, 0.0];

        let mut to = EntityState::new(1, 0);
        to.position = [10.0, 20.0, 30.0];

        let result = interpolate_entity_states(&from, &to, 0.5);

        assert!((result.position.x - 5.0).abs() < 0.001);
        assert!((result.position.y - 10.0).abs() < 0.001);
        assert!((result.position.z - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_slerp_interpolation() {
        let mut from = EntityState::new(1, 0);
        from.encode_orientation([0.0, 0.0, 0.0, 1.0]);

        let mut to = EntityState::new(1, 0);
        let half_angle = std::f32::consts::FRAC_PI_4;
        to.encode_orientation([0.0, half_angle.sin(), 0.0, half_angle.cos()]);

        let result = interpolate_entity_states(&from, &to, 0.5);

        let expected_half = std::f32::consts::FRAC_PI_8;
        assert!((result.orientation.y - expected_half.sin()).abs() < 0.1);
    }

    #[test]
    fn test_hermite_interpolation() {
        let p0 = Vec3::new(-1.0, 0.0, 0.0);
        let p1 = Vec3::new(0.0, 0.0, 0.0);
        let p2 = Vec3::new(1.0, 0.0, 0.0);
        let p3 = Vec3::new(2.0, 0.0, 0.0);

        let mid = hermite_interpolate(p0, p1, p2, p3, 0.5);

        assert!((mid.x - 0.5).abs() < 0.1);
    }
}

pub fn hermite_interpolate(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let t2 = t * t;
    let t3 = t2 * t;

    let c0 = -0.5 * t3 + t2 - 0.5 * t;
    let c1 = 1.5 * t3 - 2.5 * t2 + 1.0;
    let c2 = -1.5 * t3 + 2.0 * t2 + 0.5 * t;
    let c3 = 0.5 * t3 - 0.5 * t2;

    p0 * c0 + p1 * c1 + p2 * c2 + p3 * c3
}
