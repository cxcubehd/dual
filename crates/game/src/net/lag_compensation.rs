use glam::{Quat, Vec3};

use crate::snapshot::NetId;

/// A single position record stored for lag compensation rewind.
#[derive(Debug, Clone, Copy)]
pub struct LagRecord {
    pub tick: u32,
    pub server_time_ms: u64,
    pub position: Vec3,
    pub orientation: Quat,
}

/// Per-entity ring buffer of position history for server-side lag compensation.
///
/// The server stores up to `sv_maxunlag` (default 1s) of history per entity.
/// When processing a weapon fire, the server rewinds hitboxes to what the
/// shooter saw, performs hit detection, then restores everything.
#[derive(Debug)]
pub struct LagCompensationHistory {
    records: Vec<Option<LagRecord>>,
    capacity: usize,
    mask: usize,
    latest_tick: u32,
}

impl LagCompensationHistory {
    /// Create history with capacity rounded up to next power of two.
    /// For 128 tick / 1s max rewind, use capacity >= 128.
    pub fn new(min_capacity: usize) -> Self {
        let capacity = min_capacity.next_power_of_two().max(2);
        let mut records = Vec::with_capacity(capacity);
        records.resize_with(capacity, || None);
        Self {
            records,
            capacity,
            mask: capacity - 1,
            latest_tick: 0,
        }
    }

    fn index(&self, tick: u32) -> usize {
        (tick as usize) & self.mask
    }

    /// Record position at a tick.
    pub fn record(&mut self, tick: u32, server_time_ms: u64, position: Vec3, orientation: Quat) {
        let idx = self.index(tick);
        self.records[idx] = Some(LagRecord {
            tick,
            server_time_ms,
            position,
            orientation,
        });
        self.latest_tick = tick;
    }

    /// Find the record at or just before the given time, and the one after, for interpolation.
    pub fn find_bracketing(
        &self,
        target_time_ms: u64,
    ) -> Option<(&LagRecord, &LagRecord)> {
        let mut before: Option<&LagRecord> = None;
        let mut after: Option<&LagRecord> = None;

        for record in self.records.iter().flatten() {
            if record.server_time_ms <= target_time_ms {
                match before {
                    None => before = Some(record),
                    Some(b) if record.server_time_ms > b.server_time_ms => before = Some(record),
                    _ => {}
                }
            } else {
                match after {
                    None => after = Some(record),
                    Some(a) if record.server_time_ms < a.server_time_ms => after = Some(record),
                    _ => {}
                }
            }
        }

        match (before, after) {
            (Some(b), Some(a)) => Some((b, a)),
            _ => None,
        }
    }

    /// Get exact record at tick, if it exists.
    pub fn get(&self, tick: u32) -> Option<&LagRecord> {
        let idx = self.index(tick);
        self.records[idx].as_ref().filter(|r| r.tick == tick)
    }

    /// Interpolate position at a target time between two bracketing records.
    pub fn interpolate_at(&self, target_time_ms: u64) -> Option<(Vec3, Quat)> {
        let (before, after) = self.find_bracketing(target_time_ms)?;
        let range = (after.server_time_ms - before.server_time_ms) as f32;
        if range < 0.001 {
            return Some((before.position, before.orientation));
        }
        let t = (target_time_ms - before.server_time_ms) as f32 / range;
        let t = t.clamp(0.0, 1.0);
        Some((
            before.position.lerp(after.position, t),
            before.orientation.slerp(after.orientation, t),
        ))
    }
}

/// Manages lag compensation history for all tracked entities.
#[derive(Debug)]
pub struct LagCompensationManager {
    histories: std::collections::HashMap<NetId, LagCompensationHistory>,
    capacity_per_entity: usize,
}

impl LagCompensationManager {
    /// Create with a given history depth per entity (e.g. 128 for 1s at 128 tick).
    pub fn new(capacity_per_entity: usize) -> Self {
        Self {
            histories: std::collections::HashMap::new(),
            capacity_per_entity,
        }
    }

    /// Record a position for an entity.
    pub fn record(
        &mut self,
        entity_id: NetId,
        tick: u32,
        server_time_ms: u64,
        position: Vec3,
        orientation: Quat,
    ) {
        let history = self
            .histories
            .entry(entity_id)
            .or_insert_with(|| LagCompensationHistory::new(self.capacity_per_entity));
        history.record(tick, server_time_ms, position, orientation);
    }

    /// Compute rewound positions for all entities at a target time.
    /// Returns Vec of (entity_id, rewound_position, rewound_orientation).
    pub fn rewind_all(&self, target_time_ms: u64) -> Vec<(NetId, Vec3, Quat)> {
        self.histories
            .iter()
            .filter_map(|(&id, history)| {
                history
                    .interpolate_at(target_time_ms)
                    .map(|(pos, ori)| (id, pos, ori))
            })
            .collect()
    }

    /// Remove history for a despawned entity.
    pub fn remove(&mut self, entity_id: NetId) {
        self.histories.remove(&entity_id);
    }
}
