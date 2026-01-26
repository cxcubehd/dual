//! Temporal Interpolation Engine
//!
//! Implements client-side jitter buffer and smooth interpolation between
//! server snapshots to provide visually smooth rendering independent of
//! network variance.
//!
//! # Interpolation Model
//! ```text
//! Server Ticks:  T-2       T-1       T0        T+1 (future, not received)
//!                 │         │         │
//!                 ▼         ▼         ▼
//!            ┌────────┐ ┌────────┐ ┌────────┐
//!            │  S-2   │ │  S-1   │ │  S0    │
//!            └────────┘ └────────┘ └────────┘
//!                         │         │
//!                         └────┬────┘
//!                              │
//!                              ▼
//!                    Interpolation (t ∈ [0,1])
//!                              │
//!                              ▼
//!                    ┌─────────────────┐
//!                    │ Rendered State  │
//!                    └─────────────────┘
//! ```
//!
//! The client renders at a fixed delay behind the latest server snapshot,
//! interpolating between $T_{-1}$ and $T_{0}$ to ensure smooth visuals.

use std::collections::HashMap;

use glam::{Quat, Vec3};

use super::protocol::{EntityState, WorldSnapshot};
use super::snapshot::{Entity, EntityType, SnapshotBuffer};

/// Default interpolation delay in number of ticks.
/// Higher values provide smoother interpolation but increase visual latency.
pub const DEFAULT_INTERPOLATION_DELAY_TICKS: u32 = 2;

/// Jitter buffer configuration
#[derive(Debug, Clone)]
pub struct JitterBufferConfig {
    /// Number of snapshots to buffer before starting playback
    pub min_buffer_size: usize,
    /// Maximum buffer size before dropping old snapshots
    pub max_buffer_size: usize,
    /// Interpolation delay in ticks
    pub interpolation_delay: u32,
    /// Tick duration in seconds (from server tick rate)
    pub tick_duration_secs: f32,
}

impl Default for JitterBufferConfig {
    fn default() -> Self {
        Self {
            min_buffer_size: 2,
            max_buffer_size: 32,
            interpolation_delay: DEFAULT_INTERPOLATION_DELAY_TICKS,
            tick_duration_secs: 1.0 / 20.0, // 20 tick server
        }
    }
}

/// Interpolated entity state ready for rendering.
///
/// Contains high-precision interpolated values suitable for
/// smooth visual representation.
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
    /// Create from a network EntityState
    pub fn from_network_state(state: &EntityState) -> Self {
        let entity = Entity::from_network_state(state);
        Self::from(&entity)
    }
}

/// Jitter buffer and interpolation engine.
///
/// Manages incoming snapshots and provides smooth interpolated state
/// for rendering. Implements:
/// - Snapshot buffering and ordering
/// - Adaptive timing based on network conditions
/// - Linear interpolation (LERP) for positions
/// - Spherical interpolation (SLERP) for orientations
#[derive(Debug)]
pub struct InterpolationEngine {
    /// Snapshot ring buffer
    buffer: SnapshotBuffer,
    /// Configuration
    config: JitterBufferConfig,
    /// Local render time in seconds
    render_time: f64,
    /// Tick we're currently interpolating from
    from_tick: Option<u32>,
    /// Tick we're currently interpolating to
    to_tick: Option<u32>,
    /// Interpolation factor [0, 1] between from_tick and to_tick
    interpolation_t: f32,
    /// Last known server tick
    latest_server_tick: u32,
    /// Cached interpolated entities
    interpolated_entities: HashMap<u32, InterpolatedEntity>,
    /// Has enough data to begin playback
    ready: bool,
}

impl InterpolationEngine {
    pub fn new(config: JitterBufferConfig) -> Self {
        Self {
            buffer: SnapshotBuffer::new(config.max_buffer_size),
            config,
            render_time: 0.0,
            from_tick: None,
            to_tick: None,
            interpolation_t: 0.0,
            latest_server_tick: 0,
            interpolated_entities: HashMap::new(),
            ready: false,
        }
    }

    /// Create with default configuration
    pub fn with_defaults() -> Self {
        Self::new(JitterBufferConfig::default())
    }

    /// Push a new snapshot from the server
    pub fn push_snapshot(&mut self, snapshot: WorldSnapshot) {
        // Update latest tick tracking
        if snapshot.tick > self.latest_server_tick {
            self.latest_server_tick = snapshot.tick;
        }

        self.buffer.push(snapshot);

        // Check if we have enough data to start playback
        if !self.ready && self.buffer.len() >= self.config.min_buffer_size {
            self.ready = true;
            self.initialize_interpolation();
        }
    }

    /// Initialize interpolation state when buffer is ready
    fn initialize_interpolation(&mut self) {
        if let Some((from, to)) = self.buffer.get_interpolation_pair() {
            self.from_tick = Some(from.tick);
            self.to_tick = Some(to.tick);
            self.interpolation_t = 0.0;

            // Set render time to be at the 'from' snapshot, delayed from server
            let target_tick = self
                .latest_server_tick
                .saturating_sub(self.config.interpolation_delay);
            self.render_time = target_tick as f64 * self.config.tick_duration_secs as f64;
        }
    }

    /// Update the interpolation state.
    ///
    /// Call this every frame with the frame delta time.
    pub fn update(&mut self, delta_time: f32) {
        if !self.ready {
            return;
        }

        // Advance render time
        self.render_time += delta_time as f64;

        // Calculate which ticks we should be interpolating between
        let tick_time = self.config.tick_duration_secs as f64;
        let current_tick_f = self.render_time / tick_time;
        let from_tick = current_tick_f.floor() as u32;
        let to_tick = from_tick + 1;

        // Calculate interpolation factor
        self.interpolation_t = (current_tick_f.fract()) as f32;

        // Clone snapshots to avoid borrow issues
        let from_snapshot = self.buffer.get_by_tick(from_tick).cloned();
        let to_snapshot = self.buffer.get_by_tick(to_tick).cloned();

        match (from_snapshot, to_snapshot) {
            (Some(ref from), Some(ref to)) => {
                self.from_tick = Some(from.tick);
                self.to_tick = Some(to.tick);
                self.interpolate_entities(from, to);
            }
            (Some(ref from), None) => {
                // Don't have the next snapshot yet, extrapolate or hold
                self.from_tick = Some(from.tick);
                self.to_tick = None;
                self.extrapolate_entities(from, delta_time);
            }
            (None, Some(ref to)) => {
                // Missed the from snapshot, snap to 'to'
                self.from_tick = None;
                self.to_tick = Some(to.tick);
                self.snap_to_snapshot(to);
            }
            (None, None) => {
                // No valid snapshots, try to find any usable pair
                if let Some((from, to)) = self.buffer.get_interpolation_pair() {
                    // Snap render time to available data
                    self.render_time = from.tick as f64 * tick_time;
                    self.from_tick = Some(from.tick);
                    self.to_tick = Some(to.tick);
                    self.interpolation_t = 0.0;
                    // Clone to avoid borrow
                    let from = from.clone();
                    let to = to.clone();
                    self.interpolate_entities(&from, &to);
                }
            }
        }
    }

    /// Perform linear/spherical interpolation between two snapshots
    fn interpolate_entities(&mut self, from: &WorldSnapshot, to: &WorldSnapshot) {
        let t = self.interpolation_t;

        // Build lookup for 'to' snapshot entities
        let to_entities: HashMap<u32, &EntityState> = to
            .entities
            .iter()
            .map(|e| (e.entity_id, e))
            .collect();

        // Interpolate each entity
        for from_state in &from.entities {
            let entity_id = from_state.entity_id;

            let interpolated = if let Some(to_state) = to_entities.get(&entity_id) {
                // Both snapshots have this entity - interpolate
                Self::interpolate_entity_states(from_state, to_state, t)
            } else {
                // Entity only in 'from' - might be despawning, just use from state
                InterpolatedEntity::from_network_state(from_state)
            };

            self.interpolated_entities.insert(entity_id, interpolated);
        }

        // Handle entities that only exist in 'to' (newly spawned)
        for to_state in &to.entities {
            if !self.interpolated_entities.contains_key(&to_state.entity_id) {
                let interpolated = InterpolatedEntity::from_network_state(to_state);
                self.interpolated_entities
                    .insert(to_state.entity_id, interpolated);
            }
        }
    }

    /// Interpolate between two entity states
    fn interpolate_entity_states(
        from: &EntityState,
        to: &EntityState,
        t: f32,
    ) -> InterpolatedEntity {
        // Position: Linear interpolation
        let from_pos = Vec3::from(from.position);
        let to_pos = Vec3::from(to.position);
        let position = from_pos.lerp(to_pos, t);

        // Velocity: Linear interpolation
        let from_vel = Vec3::from(from.decode_velocity());
        let to_vel = Vec3::from(to.decode_velocity());
        let velocity = from_vel.lerp(to_vel, t);

        // Orientation: Spherical linear interpolation (SLERP)
        let from_quat_arr = from.decode_orientation();
        let to_quat_arr = to.decode_orientation();
        let from_quat =
            Quat::from_xyzw(from_quat_arr[0], from_quat_arr[1], from_quat_arr[2], from_quat_arr[3])
                .normalize();
        let to_quat =
            Quat::from_xyzw(to_quat_arr[0], to_quat_arr[1], to_quat_arr[2], to_quat_arr[3])
                .normalize();
        let orientation = from_quat.slerp(to_quat, t);

        // Animation: Linear interpolation with wrap handling
        let from_anim = from.animation_frame as f32 / 255.0;
        let to_anim = to.animation_frame as f32 / 255.0;
        let animation_time = if (to_anim - from_anim).abs() > 0.5 {
            // Animation wrapped, interpolate the short way
            if to_anim < from_anim {
                let adjusted_to = to_anim + 1.0;
                (from_anim + (adjusted_to - from_anim) * t) % 1.0
            } else {
                let adjusted_from = from_anim + 1.0;
                (adjusted_from + (to_anim - adjusted_from) * t) % 1.0
            }
        } else {
            from_anim + (to_anim - from_anim) * t
        };

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

    /// Extrapolate entities when next snapshot hasn't arrived
    fn extrapolate_entities(&mut self, from: &WorldSnapshot, delta_time: f32) {
        for state in &from.entities {
            let mut entity = InterpolatedEntity::from_network_state(state);

            // Simple linear extrapolation based on velocity
            entity.position += entity.velocity * delta_time;

            // Advance animation time
            entity.animation_time = (entity.animation_time + delta_time) % 1.0;

            self.interpolated_entities.insert(entity.id, entity);
        }
    }

    /// Snap all entities to a single snapshot (no interpolation)
    fn snap_to_snapshot(&mut self, snapshot: &WorldSnapshot) {
        self.interpolated_entities.clear();
        for state in &snapshot.entities {
            let entity = InterpolatedEntity::from_network_state(state);
            self.interpolated_entities.insert(entity.id, entity);
        }
    }

    /// Get the current interpolated entity state
    pub fn get_entity(&self, entity_id: u32) -> Option<&InterpolatedEntity> {
        self.interpolated_entities.get(&entity_id)
    }

    /// Get all interpolated entities
    pub fn entities(&self) -> impl Iterator<Item = &InterpolatedEntity> {
        self.interpolated_entities.values()
    }

    /// Get the current interpolation factor [0, 1]
    pub fn interpolation_factor(&self) -> f32 {
        self.interpolation_t
    }

    /// Get the ticks being interpolated between
    pub fn interpolation_ticks(&self) -> (Option<u32>, Option<u32>) {
        (self.from_tick, self.to_tick)
    }

    /// Check if the engine has enough data and is ready for rendering
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Get current render time
    pub fn render_time(&self) -> f64 {
        self.render_time
    }

    /// Get latest known server tick
    pub fn latest_server_tick(&self) -> u32 {
        self.latest_server_tick
    }

    /// Clear all state (e.g., on disconnect)
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.render_time = 0.0;
        self.from_tick = None;
        self.to_tick = None;
        self.interpolation_t = 0.0;
        self.latest_server_tick = 0;
        self.interpolated_entities.clear();
        self.ready = false;
    }

    /// Get buffer statistics for debugging
    pub fn debug_stats(&self) -> InterpolationStats {
        InterpolationStats {
            buffer_size: self.buffer.len(),
            render_time: self.render_time,
            from_tick: self.from_tick,
            to_tick: self.to_tick,
            interpolation_t: self.interpolation_t,
            latest_server_tick: self.latest_server_tick,
            entity_count: self.interpolated_entities.len(),
            is_ready: self.ready,
        }
    }
}

/// Debug statistics for the interpolation engine
#[derive(Debug, Clone)]
pub struct InterpolationStats {
    pub buffer_size: usize,
    pub render_time: f64,
    pub from_tick: Option<u32>,
    pub to_tick: Option<u32>,
    pub interpolation_t: f32,
    pub latest_server_tick: u32,
    pub entity_count: usize,
    pub is_ready: bool,
}

/// Cubic Hermite spline interpolation for smoother motion.
///
/// Requires 4 control points: P0, P1, P2, P3
/// Interpolates between P1 and P2 using t ∈ [0, 1]
pub fn hermite_interpolate(p0: Vec3, p1: Vec3, p2: Vec3, p3: Vec3, t: f32) -> Vec3 {
    let t2 = t * t;
    let t3 = t2 * t;

    // Catmull-Rom coefficients
    let c0 = -0.5 * t3 + t2 - 0.5 * t;
    let c1 = 1.5 * t3 - 2.5 * t2 + 1.0;
    let c2 = -1.5 * t3 + 2.0 * t2 + 0.5 * t;
    let c3 = 0.5 * t3 - 0.5 * t2;

    p0 * c0 + p1 * c1 + p2 * c2 + p3 * c3
}

/// Squad (Spherical Quadrangle) interpolation for quaternions.
///
/// Provides C1 continuous rotation interpolation using 4 quaternions.
pub fn squad_interpolate(q0: Quat, q1: Quat, q2: Quat, q3: Quat, t: f32) -> Quat {
    // Compute intermediate quaternions
    let s1 = squad_intermediate(q0, q1, q2);
    let s2 = squad_intermediate(q1, q2, q3);

    // Double SLERP
    let slerp_q1_q2 = q1.slerp(q2, t);
    let slerp_s1_s2 = s1.slerp(s2, t);

    slerp_q1_q2.slerp(slerp_s1_s2, 2.0 * t * (1.0 - t))
}

/// Compute intermediate quaternion for Squad interpolation
fn squad_intermediate(q0: Quat, q1: Quat, q2: Quat) -> Quat {
    let q1_inv = q1.conjugate();

    let log_q0 = quat_log(q1_inv * q0);
    let log_q2 = quat_log(q1_inv * q2);

    let sum = Vec3::new(
        log_q0.x + log_q2.x,
        log_q0.y + log_q2.y,
        log_q0.z + log_q2.z,
    ) * -0.25;

    q1 * quat_exp(Quat::from_xyzw(sum.x, sum.y, sum.z, 0.0))
}

/// Quaternion logarithm (for unit quaternions)
fn quat_log(q: Quat) -> Quat {
    let v = Vec3::new(q.x, q.y, q.z);
    let v_len = v.length();

    if v_len < 1e-6 {
        Quat::from_xyzw(0.0, 0.0, 0.0, 0.0)
    } else {
        let theta = v_len.atan2(q.w);
        let v_normalized = v / v_len;
        Quat::from_xyzw(
            v_normalized.x * theta,
            v_normalized.y * theta,
            v_normalized.z * theta,
            0.0,
        )
    }
}

/// Quaternion exponential
fn quat_exp(q: Quat) -> Quat {
    let v = Vec3::new(q.x, q.y, q.z);
    let theta = v.length();

    if theta < 1e-6 {
        Quat::IDENTITY
    } else {
        let sin_theta = theta.sin();
        let cos_theta = theta.cos();
        let v_normalized = v / theta;
        Quat::from_xyzw(
            v_normalized.x * sin_theta,
            v_normalized.y * sin_theta,
            v_normalized.z * sin_theta,
            cos_theta,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_snapshot(tick: u32, entity_count: usize) -> WorldSnapshot {
        let mut snapshot = WorldSnapshot::new(tick, tick as u64 * 50);
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

        // Push enough snapshots to initialize
        engine.push_snapshot(create_test_snapshot(0, 2));
        engine.push_snapshot(create_test_snapshot(1, 2));

        assert!(engine.is_ready());
    }

    #[test]
    fn test_lerp_interpolation() {
        let mut from = EntityState::new(1, 0);
        from.position = [0.0, 0.0, 0.0];

        let mut to = EntityState::new(1, 0);
        to.position = [10.0, 20.0, 30.0];

        let result = InterpolationEngine::interpolate_entity_states(&from, &to, 0.5);

        assert!((result.position.x - 5.0).abs() < 0.001);
        assert!((result.position.y - 10.0).abs() < 0.001);
        assert!((result.position.z - 15.0).abs() < 0.001);
    }

    #[test]
    fn test_slerp_interpolation() {
        let mut from = EntityState::new(1, 0);
        from.encode_orientation([0.0, 0.0, 0.0, 1.0]); // Identity

        let mut to = EntityState::new(1, 0);
        // 90 degrees around Y axis
        let half_angle = std::f32::consts::FRAC_PI_4; // 45 degrees (half of 90)
        to.encode_orientation([0.0, half_angle.sin(), 0.0, half_angle.cos()]);

        let result = InterpolationEngine::interpolate_entity_states(&from, &to, 0.5);

        // Should be approximately 45 degrees around Y
        let expected_half = std::f32::consts::FRAC_PI_8; // 22.5 degrees
        assert!((result.orientation.y - expected_half.sin()).abs() < 0.1);
    }

    #[test]
    fn test_hermite_interpolation() {
        let p0 = Vec3::new(-1.0, 0.0, 0.0);
        let p1 = Vec3::new(0.0, 0.0, 0.0);
        let p2 = Vec3::new(1.0, 0.0, 0.0);
        let p3 = Vec3::new(2.0, 0.0, 0.0);

        let mid = hermite_interpolate(p0, p1, p2, p3, 0.5);

        // At t=0.5, should be approximately at (0.5, 0, 0)
        assert!((mid.x - 0.5).abs() < 0.1);
    }
}
