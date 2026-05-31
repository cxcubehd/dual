use std::collections::HashMap;

use glam::Vec3;

use crate::{EntityState, WorldSnapshot};

use super::config::InterpolationConfig;
use super::entity::InterpolatedEntity;
use super::math::interpolate_entity_states;
use super::stats::InterpolationStats;
use super::time::current_time_ms;

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
    knowledge_tick: u32,
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
            knowledge_tick: 0,
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

        if let Some(full_snapshot) = self.expand_snapshot(snapshot) {
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
    }

    pub(super) fn get_snapshot_by_tick(&self, tick: u32) -> Option<&WorldSnapshot> {
        for ts in self.snapshots.iter().rev() {
            if ts.snapshot.tick == tick {
                return Some(&ts.snapshot);
            }
        }
        None
    }

    fn expand_snapshot(&mut self, snapshot: WorldSnapshot) -> Option<WorldSnapshot> {
        if !snapshot.is_delta {
            self.known_entities.clear();
            for entity in &snapshot.entities {
                self.known_entities.insert(entity.entity_id, entity.clone());
            }
            for removed_id in &snapshot.removed_entity_ids {
                self.known_entities.remove(removed_id);
            }
            self.knowledge_tick = snapshot.tick;
            return Some(snapshot);
        }

        if snapshot.baseline_tick != self.knowledge_tick {
            if let Some(baseline) = self.get_snapshot_by_tick(snapshot.baseline_tick) {
                let entities = baseline.entities.clone();
                self.known_entities.clear();
                for entity in entities {
                    self.known_entities.insert(entity.entity_id, entity);
                }
                self.knowledge_tick = snapshot.baseline_tick;
            } else {
                return None;
            }
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

        self.knowledge_tick = snapshot.tick;
        Some(full_snapshot)
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
        let cutoff = self.render_time_ms - self.config.snapshot_retention_ms;
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
            knowledge_tick: self.knowledge_tick,
        }
    }
}
