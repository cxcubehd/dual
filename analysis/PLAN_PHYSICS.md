# Rapier 3D Physics Integration Plan

This document outlines the architecture strategy for integrating Rapier 3D physics into the multiplayer FPS proof-of-concept, focusing on state management, snapshot/restore capabilities, and temporal manipulation for prediction and lag compensation.

---

## Table of Contents

1. [Overview & Challenges](#1-overview--challenges)
2. [Rapier 3D Architecture Primer](#2-rapier-3d-architecture-primer)
3. [State Storage Strategies](#3-state-storage-strategies)
4. [Snapshot & Restore Implementation](#4-snapshot--restore-implementation)
5. [Temporal Manipulation](#5-temporal-manipulation)
6. [Integration with Existing Architecture](#6-integration-with-existing-architecture)
7. [Proof of Concept Implementation Steps](#7-proof-of-concept-implementation-steps)

---

## 1. Overview & Challenges

### The Problem

In a server-authoritative multiplayer FPS, physics state must be:

1. **Deterministic** - Same inputs produce same outputs
2. **Snapshotable** - Capture complete state at any tick
3. **Restorable** - Rewind to any historical state
4. **Steppable** - Advance simulation by exact timesteps

### Key Use Cases

| Use Case | Who | Description |
|----------|-----|-------------|
| **Client Prediction** | Client | Run physics locally for immediate feedback |
| **Reconciliation** | Client | Rewind to server tick, replay inputs forward |
| **Lag Compensation** | Server | Rewind world to client's view-time for hit detection |
| **Authoritative Sim** | Server | Single source of truth physics simulation |

### Rapier-Specific Challenges

1. **Rapier's world is not trivially serializable** - Contains handles, internal caches, islands
2. **No built-in snapshot/restore** - Must implement custom solution
3. **Determinism requires care** - Must use same stepping parameters everywhere
4. **Handle-based API** - Entity references are handles, not direct pointers

---

## 2. Rapier 3D Architecture Primer

### Core Components

```rust
use rapier3d::prelude::*;

// The physics pipeline components
struct PhysicsWorld {
    // Core simulation state (MUST be snapshotted)
    rigid_body_set: RigidBodySet,
    collider_set: ColliderSet,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    
    // Simulation infrastructure (can be recreated)
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    ccd_solver: CCDSolver,
    query_pipeline: QueryPipeline,
    
    // Configuration (constant)
    gravity: Vector<Real>,
    integration_parameters: IntegrationParameters,
}
```

### Handle System

Rapier uses handles to reference entities:

```rust
// Handles are essentially typed indices
let body_handle: RigidBodyHandle = rigid_body_set.insert(rigid_body);
let collider_handle: ColliderHandle = collider_set.insert_with_parent(
    collider, 
    body_handle, 
    &mut rigid_body_set
);

// Your game entities need to map to these handles
struct GameEntity {
    entity_id: EntityId,
    body_handle: RigidBodyHandle,
    collider_handles: Vec<ColliderHandle>,
}
```

---

## 3. State Storage Strategies

### Strategy A: Full World Clone (Recommended for PoC)

**Concept**: Store complete clones of the physics sets at each tick.

```rust
/// Snapshot of all physics state at a specific tick
#[derive(Clone)]
pub struct PhysicsSnapshot {
    pub tick: Tick,
    pub rigid_body_set: RigidBodySet,
    pub collider_set: ColliderSet,
    pub impulse_joint_set: ImpulseJointSet,
    pub multibody_joint_set: MultibodyJointSet,
    // Note: Island manager, broad/narrow phase are NOT stored
    // They will be rebuilt or updated on restore
}

impl PhysicsSnapshot {
    pub fn capture(world: &PhysicsWorld, tick: Tick) -> Self {
        Self {
            tick,
            rigid_body_set: world.rigid_body_set.clone(),
            collider_set: world.collider_set.clone(),
            impulse_joint_set: world.impulse_joint_set.clone(),
            multibody_joint_set: world.multibody_joint_set.clone(),
        }
    }
}
```

**Pros**:
- Simple to implement
- Guaranteed correctness
- Handles stay valid after restore

**Cons**:
- Memory intensive (~KB per snapshot × entities)
- Clone cost each tick

**Memory Estimation**:
- ~200 bytes per RigidBody
- ~150 bytes per Collider
- 10 players + 50 projectiles = ~21 KB per snapshot
- 128 snapshots = ~2.7 MB (acceptable for PoC)

---

### Strategy B: Delta Snapshots (Optimization)

**Concept**: Store only changed bodies between ticks.

```rust
pub struct DeltaSnapshot {
    pub tick: Tick,
    pub base_tick: Tick,
    pub changed_bodies: HashMap<RigidBodyHandle, RigidBodyState>,
    pub added_bodies: Vec<(RigidBodyHandle, RigidBody)>,
    pub removed_bodies: Vec<RigidBodyHandle>,
}

#[derive(Clone)]
pub struct RigidBodyState {
    pub position: Isometry<Real>,
    pub linvel: Vector<Real>,
    pub angvel: Vector<Real>,
}
```

**Pros**:
- Much smaller memory footprint
- Faster to create for mostly-static worlds

**Cons**:
- More complex to implement
- Requires base snapshot + chain of deltas
- Reconciliation becomes more complex

**Recommendation**: Start with Strategy A, optimize to B if memory becomes an issue.

---

### Strategy C: Minimal State Extraction (For Network)

**Concept**: Extract only what's needed for network transmission.

```rust
/// Minimal state for network transmission
#[derive(Archive, Deserialize, Serialize, Clone)]
pub struct NetworkPhysicsState {
    pub tick: Tick,
    pub entities: Vec<NetworkEntityState>,
}

#[derive(Archive, Deserialize, Serialize, Clone)]
pub struct NetworkEntityState {
    pub entity_id: EntityId,
    pub position: [f32; 3],
    pub rotation: [f32; 4],  // Quaternion
    pub linear_velocity: [f32; 3],
    pub angular_velocity: [f32; 3],
}

impl NetworkPhysicsState {
    pub fn extract(world: &PhysicsWorld, entity_map: &EntityHandleMap) -> Self {
        let entities = entity_map.iter().map(|(entity_id, handle)| {
            let body = &world.rigid_body_set[*handle];
            NetworkEntityState {
                entity_id: *entity_id,
                position: body.translation().into(),
                rotation: body.rotation().into(),
                linear_velocity: body.linvel().into(),
                angular_velocity: body.angvel().into(),
            }
        }).collect();
        
        Self { tick: world.current_tick, entities }
    }
    
    pub fn apply(&self, world: &mut PhysicsWorld, entity_map: &EntityHandleMap) {
        for state in &self.entities {
            if let Some(handle) = entity_map.get(&state.entity_id) {
                if let Some(body) = world.rigid_body_set.get_mut(*handle) {
                    body.set_translation(state.position.into(), true);
                    body.set_rotation(state.rotation.into(), true);
                    body.set_linvel(state.linear_velocity.into(), true);
                    body.set_angvel(state.angular_velocity.into(), true);
                }
            }
        }
    }
}
```

---

## 4. Snapshot & Restore Implementation

### 4.1 Snapshot Ring Buffer

```rust
use std::collections::VecDeque;

pub struct PhysicsHistory {
    snapshots: VecDeque<PhysicsSnapshot>,
    max_history: usize,
}

impl PhysicsHistory {
    pub fn new(max_history: usize) -> Self {
        Self {
            snapshots: VecDeque::with_capacity(max_history),
            max_history,
        }
    }
    
    /// Store a snapshot, evicting oldest if at capacity
    pub fn push(&mut self, snapshot: PhysicsSnapshot) {
        if self.snapshots.len() >= self.max_history {
            self.snapshots.pop_front();
        }
        self.snapshots.push_back(snapshot);
    }
    
    /// Get snapshot at or before the given tick
    pub fn get(&self, tick: Tick) -> Option<&PhysicsSnapshot> {
        self.snapshots.iter().rev().find(|s| s.tick <= tick)
    }
    
    /// Get exact snapshot for a tick
    pub fn get_exact(&self, tick: Tick) -> Option<&PhysicsSnapshot> {
        self.snapshots.iter().find(|s| s.tick == tick)
    }
    
    /// Remove all snapshots older than the given tick
    pub fn prune_before(&mut self, tick: Tick) {
        while self.snapshots.front().map_or(false, |s| s.tick < tick) {
            self.snapshots.pop_front();
        }
    }
}
```

### 4.2 World Restoration

```rust
impl PhysicsWorld {
    /// Restore world state from a snapshot
    pub fn restore_from(&mut self, snapshot: &PhysicsSnapshot) {
        // Replace the simulation sets
        self.rigid_body_set = snapshot.rigid_body_set.clone();
        self.collider_set = snapshot.collider_set.clone();
        self.impulse_joint_set = snapshot.impulse_joint_set.clone();
        self.multibody_joint_set = snapshot.multibody_joint_set.clone();
        
        // CRITICAL: Clear cached data that references old state
        self.island_manager = IslandManager::new();
        self.broad_phase = DefaultBroadPhase::new();
        self.narrow_phase = NarrowPhase::new();
        
        // Rebuild query pipeline for raycasts
        self.query_pipeline = QueryPipeline::new();
        self.query_pipeline.update(&self.collider_set);
    }
    
    /// Fast restore that only updates dynamic body states
    /// Use when world structure hasn't changed (no adds/removes)
    pub fn restore_states_only(&mut self, snapshot: &PhysicsSnapshot) {
        for (handle, body) in snapshot.rigid_body_set.iter() {
            if let Some(current_body) = self.rigid_body_set.get_mut(handle) {
                current_body.set_position(*body.position(), true);
                current_body.set_linvel(*body.linvel(), true);
                current_body.set_angvel(*body.angvel(), true);
            }
        }
        // Update query pipeline for accurate raycasts
        self.query_pipeline.update(&self.collider_set);
    }
}
```

---

## 5. Temporal Manipulation

### 5.1 Stepping Forward (Simulation)

```rust
impl PhysicsWorld {
    /// Advance physics by one fixed timestep
    pub fn step(&mut self) {
        self.physics_pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_body_set,
            &mut self.collider_set,
            &mut self.impulse_joint_set,
            &mut self.multibody_joint_set,
            &mut self.ccd_solver,
            None, // query_filter_hook
            &(),  // physics_hooks
        );
        
        // Update query pipeline after step (for raycasts)
        self.query_pipeline.update(&self.collider_set);
    }
    
    /// Apply player input then step
    pub fn step_with_input(&mut self, input: &PlayerInput, player_handle: RigidBodyHandle) {
        // Apply forces/impulses based on input
        self.apply_player_input(input, player_handle);
        // Step simulation
        self.step();
    }
}
```

### 5.2 Rewinding (Reconciliation)

```rust
/// Client-side reconciliation when server snapshot arrives
pub fn reconcile(
    world: &mut PhysicsWorld,
    history: &PhysicsHistory,
    input_history: &InputHistory,
    server_snapshot: &NetworkPhysicsState,
    current_tick: Tick,
) -> bool {
    // 1. Find our local snapshot at the server's tick
    let Some(local_snapshot) = history.get_exact(server_snapshot.tick) else {
        log::warn!("No local snapshot for tick {}", server_snapshot.tick);
        return false;
    };
    
    // 2. Compare states - check if reconciliation needed
    if states_close_enough(&local_snapshot, server_snapshot) {
        return false; // No correction needed
    }
    
    log::debug!(
        "Reconciliation needed at tick {} (current: {})",
        server_snapshot.tick,
        current_tick
    );
    
    // 3. Restore to server's authoritative state
    world.restore_from(local_snapshot);
    server_snapshot.apply(world, &entity_map);
    
    // 4. Replay all inputs from server_tick+1 to current_tick
    for replay_tick in (server_snapshot.tick + 1)..=current_tick {
        if let Some(input) = input_history.get(replay_tick) {
            world.step_with_input(&input, local_player_handle);
        } else {
            world.step(); // No input recorded, just step
        }
    }
    
    true // Reconciliation was performed
}

fn states_close_enough(local: &PhysicsSnapshot, server: &NetworkPhysicsState) -> bool {
    const POSITION_THRESHOLD: f32 = 0.01; // 1cm
    const VELOCITY_THRESHOLD: f32 = 0.1;  // 0.1 m/s
    
    // Compare each entity...
    // Implementation depends on your entity mapping
    true
}
```

### 5.3 Lag Compensation (Server Hit Detection)

```rust
/// Server-side lag compensation for hit detection
pub struct LagCompensator {
    history: PhysicsHistory,
    /// Temporary world used for rewinding (avoids mutating live world)
    rewind_world: PhysicsWorld,
}

impl LagCompensator {
    /// Perform a raycast as the world appeared at a past tick
    pub fn raycast_at_tick(
        &mut self,
        tick: Tick,
        ray_origin: Point<Real>,
        ray_dir: Vector<Real>,
        max_dist: Real,
        shooter_handle: ColliderHandle,
    ) -> Option<HitResult> {
        // 1. Get the snapshot at the requested tick
        let Some(snapshot) = self.history.get(tick) else {
            log::warn!("No snapshot available for tick {}", tick);
            return None;
        };
        
        // 2. Restore rewind world to that state
        self.rewind_world.restore_from(snapshot);
        
        // 3. Perform raycast excluding the shooter
        let ray = Ray::new(ray_origin, ray_dir);
        let filter = QueryFilter::default()
            .exclude_collider(shooter_handle);
        
        let hit = self.rewind_world.query_pipeline.cast_ray(
            &self.rewind_world.rigid_body_set,
            &self.rewind_world.collider_set,
            &ray,
            max_dist,
            true, // solid
            filter,
        );
        
        hit.map(|(handle, toi)| HitResult {
            collider: handle,
            distance: toi,
            point: ray.point_at(toi),
        })
    }
}

pub struct HitResult {
    pub collider: ColliderHandle,
    pub distance: Real,
    pub point: Point<Real>,
}
```

---

## 6. Integration with Existing Architecture

### 6.1 Modified Simulation Loop

Incorporating physics into the simulation thread from [REPORT_DETAIL.md](REPORT_DETAIL.md):

```rust
pub struct SimulationLoop {
    // Existing fields...
    
    // Physics additions
    physics_world: PhysicsWorld,
    physics_history: PhysicsHistory,
    entity_handle_map: HashMap<EntityId, RigidBodyHandle>,
}

impl SimulationLoop {
    fn tick(&mut self, input: &PlayerInput) {
        // 1. Apply input to physics
        if let Some(handle) = self.entity_handle_map.get(&self.local_player_id) {
            self.physics_world.apply_player_input(input, *handle);
        }
        
        // 2. Step physics
        self.physics_world.step();
        
        // 3. Snapshot for history (every tick for reconciliation)
        let snapshot = PhysicsSnapshot::capture(&self.physics_world, self.current_tick);
        self.physics_history.push(snapshot);
        
        // 4. Update game state from physics
        self.sync_game_state_from_physics();
        
        self.current_tick += 1;
    }
    
    fn handle_server_snapshot(&mut self, server_state: NetworkPhysicsState) {
        // Reconciliation logic as shown above
        if reconcile(
            &mut self.physics_world,
            &self.physics_history,
            &self.input_history,
            &server_state,
            self.current_tick,
        ) {
            // Snapshot the corrected state
            let snapshot = PhysicsSnapshot::capture(&self.physics_world, self.current_tick);
            self.physics_history.push(snapshot);
        }
        
        // Prune old history (server has acknowledged up to this tick)
        self.physics_history.prune_before(server_state.tick.saturating_sub(32));
        self.input_history.prune_before(server_state.tick);
    }
}
```

### 6.2 Entity Handle Mapping

Maintain bidirectional mapping between game entities and physics handles:

```rust
pub struct EntityHandleMap {
    entity_to_body: HashMap<EntityId, RigidBodyHandle>,
    body_to_entity: HashMap<RigidBodyHandle, EntityId>,
    entity_to_colliders: HashMap<EntityId, Vec<ColliderHandle>>,
}

impl EntityHandleMap {
    pub fn register(
        &mut self,
        entity_id: EntityId,
        body_handle: RigidBodyHandle,
        collider_handles: Vec<ColliderHandle>,
    ) {
        self.entity_to_body.insert(entity_id, body_handle);
        self.body_to_entity.insert(body_handle, entity_id);
        self.entity_to_colliders.insert(entity_id, collider_handles);
    }
    
    pub fn unregister(&mut self, entity_id: EntityId) {
        if let Some(body_handle) = self.entity_to_body.remove(&entity_id) {
            self.body_to_entity.remove(&body_handle);
        }
        self.entity_to_colliders.remove(&entity_id);
    }
    
    pub fn get_body(&self, entity_id: EntityId) -> Option<RigidBodyHandle> {
        self.entity_to_body.get(&entity_id).copied()
    }
    
    pub fn get_entity(&self, body_handle: RigidBodyHandle) -> Option<EntityId> {
        self.body_to_entity.get(&body_handle).copied()
    }
}
```

### 6.3 WorldSnapshot Integration

Extend the existing `WorldSnapshot` to include physics:

```rust
#[derive(Archive, Deserialize, Serialize, Clone, Debug)]
pub struct WorldSnapshot {
    pub tick: Tick,
    pub players: Vec<PlayerState>,
    pub projectiles: Vec<ProjectileState>,
    
    // Physics state for network (minimal representation)
    pub physics_states: Vec<NetworkEntityState>,
}

// Local-only full physics snapshot (not serialized over network)
pub struct LocalWorldState {
    pub tick: Tick,
    pub game_snapshot: WorldSnapshot,
    pub physics_snapshot: PhysicsSnapshot,
}
```

---

## 7. Proof of Concept Implementation Steps

### Phase 1: Basic Physics World (Day 1)

1. **Create `PhysicsWorld` struct** with all Rapier components
2. **Implement basic stepping** with fixed timestep
3. **Create simple test scene** (ground plane + dynamic box)
4. **Verify determinism** - same inputs = same outputs

```rust
// tests/physics_determinism.rs
#[test]
fn test_physics_determinism() {
    let mut world1 = PhysicsWorld::new();
    let mut world2 = PhysicsWorld::new();
    
    // Add same bodies to both
    let handle1 = world1.add_player(Vec3::new(0.0, 2.0, 0.0));
    let handle2 = world2.add_player(Vec3::new(0.0, 2.0, 0.0));
    
    // Same inputs
    let input = PlayerInput { forward: true, ..default() };
    
    // Step both 100 times
    for _ in 0..100 {
        world1.step_with_input(&input, handle1);
        world2.step_with_input(&input, handle2);
    }
    
    // Compare positions
    let pos1 = world1.rigid_body_set[handle1].translation();
    let pos2 = world2.rigid_body_set[handle2].translation();
    
    assert!((pos1 - pos2).magnitude() < 0.0001);
}
```

### Phase 2: Snapshot/Restore (Day 2)

1. **Implement `PhysicsSnapshot`** with clone-based capture
2. **Implement `PhysicsHistory`** ring buffer
3. **Implement `restore_from`** method
4. **Test**: snapshot at tick 50, step to 100, restore to 50, verify state matches

```rust
#[test]
fn test_snapshot_restore() {
    let mut world = PhysicsWorld::new();
    let handle = world.add_player(Vec3::ZERO);
    
    // Step to tick 50
    for _ in 0..50 {
        world.step();
    }
    
    let snapshot = PhysicsSnapshot::capture(&world, 50);
    let pos_at_50 = *world.rigid_body_set[handle].translation();
    
    // Step to tick 100
    for _ in 0..50 {
        world.step();
    }
    
    // Restore to tick 50
    world.restore_from(&snapshot);
    let pos_restored = *world.rigid_body_set[handle].translation();
    
    assert!((pos_at_50 - pos_restored).magnitude() < 0.0001);
}
```

### Phase 3: Reconciliation (Day 3)

1. **Implement input history** storage
2. **Implement reconciliation logic** (rewind + replay)
3. **Test**: simulate prediction mismatch, verify correction

### Phase 4: Lag Compensation (Day 4)

1. **Implement `LagCompensator`** with separate rewind world
2. **Implement `raycast_at_tick`** method
3. **Test**: store history, perform historical raycasts

### Phase 5: Integration (Day 5)

1. **Wire into simulation loop**
2. **Connect to network layer**
3. **End-to-end test** with simulated latency

---

## Appendix A: Configuration Constants

```rust
// crates/protocol/src/physics_config.rs

/// Physics simulation tick rate (should match game tick rate)
pub const PHYSICS_TICK_RATE: f32 = 60.0;

/// Fixed timestep for physics
pub const PHYSICS_DT: f32 = 1.0 / PHYSICS_TICK_RATE;

/// How many physics snapshots to keep for reconciliation
pub const PHYSICS_HISTORY_SIZE: usize = 128;

/// Maximum RTT we support for lag compensation (ms)
pub const MAX_LAG_COMPENSATION_MS: u64 = 200;

/// Position difference threshold for triggering reconciliation
pub const RECONCILE_POSITION_THRESHOLD: f32 = 0.01;

/// Velocity difference threshold for triggering reconciliation  
pub const RECONCILE_VELOCITY_THRESHOLD: f32 = 0.1;

/// Gravity vector
pub const GRAVITY: [f32; 3] = [0.0, -9.81, 0.0];
```

---

## Appendix B: Common Pitfalls

1. **Forgetting to rebuild query pipeline** after restore - raycasts will use stale data
2. **Storing island manager in snapshot** - it contains indices that become invalid
3. **Not using `set_position(..., true)`** - the `true` wakes the body; without it, sleeping bodies won't respond
4. **Mixing physics handles with entity IDs** - always use the mapping
5. **Non-determinism from floating point** - ensure same compilation flags, same order of operations

---

## Appendix C: Alternative Approaches Considered

### Approach: Two Separate Worlds

Run two physics worlds - one for authoritative state, one for prediction.

**Rejected because**: Doubles memory, complexity, and keeping them structurally in sync is error-prone.

### Approach: Rapier's built-in serialization

Use `serde` feature to serialize entire world.

**Rejected because**: Overhead is high, and it includes data we don't need. Custom snapshot is more efficient and gives us control.

### Approach: Command Pattern Replay

Instead of storing state, store commands (inputs) and replay from the beginning.

**Rejected because**: CPU cost grows linearly with game duration. Only viable for short matches.

---

## Summary

For a proof-of-concept:

1. **Use Strategy A (Full Clone)** - simple, correct, memory is acceptable
2. **Store snapshots per-tick** in a ring buffer of size 128
3. **Restore by cloning sets** and rebuilding caches
4. **Reconciliation**: restore → apply server state → replay inputs
5. **Lag compensation**: separate rewind world + historical raycast

This approach prioritizes correctness and simplicity over optimization, which is appropriate for a PoC. Optimization can come later once the architecture is proven.
