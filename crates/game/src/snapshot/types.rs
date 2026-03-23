use rkyv::{Archive, Deserialize, Serialize};

use super::delta::EntityDelta;
use super::entity::{EntityState, NetId};

/// Full simulation snapshot — the server's canonical world state at a tick.
#[derive(Debug, Clone)]
pub struct SimulationSnapshot {
    pub tick: u32,
    pub server_time_ms: u64,
    pub entities: Vec<EntityState>,
}

/// Network snapshot — delta-compressed, per-client, sent over the wire.
///
/// When `baseline_tick == 0` and `is_delta == false`, `entities` contains full state.
/// Otherwise, `deltas` contains field-level changes against the baseline.
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct WorldSnapshot {
    pub tick: u32,
    pub server_time_ms: u64,
    pub last_command_ack: u32,
    pub baseline_tick: u32,
    pub is_delta: bool,
    /// Full entity states (used when `is_delta == false`).
    pub entities: Vec<EntityState>,
    /// Delta-compressed entities (used when `is_delta == true`).
    pub deltas: Vec<EntityDelta>,
    /// IDs of entities removed since baseline.
    pub removed_entity_ids: Vec<NetId>,
}

impl WorldSnapshot {
    pub fn new(tick: u32, server_time_ms: u64) -> Self {
        Self {
            tick,
            server_time_ms,
            last_command_ack: 0,
            baseline_tick: 0,
            is_delta: false,
            entities: Vec::new(),
            deltas: Vec::new(),
            removed_entity_ids: Vec::new(),
        }
    }

    pub fn new_delta(tick: u32, server_time_ms: u64, baseline_tick: u32) -> Self {
        Self {
            tick,
            server_time_ms,
            last_command_ack: 0,
            baseline_tick,
            is_delta: true,
            entities: Vec::new(),
            deltas: Vec::new(),
            removed_entity_ids: Vec::new(),
        }
    }
}

/// Client-side snapshot for interpolation of remote entities.
#[derive(Debug, Clone)]
pub struct ClientSnapshot {
    pub tick: u32,
    pub server_time_ms: u64,
    pub receive_time_ms: u64,
    pub entities: Vec<EntityState>,
}
