# Physics Integration

Integration plan for Rapier 3D physics engine.

## Requirements

Physics state must be:
- **Deterministic** - Same inputs produce same outputs
- **Snapshotable** - Capture complete state at any tick
- **Restorable** - Rewind to any historical state
- **Steppable** - Advance by exact timesteps

## Architecture

### Core Components

```rust
struct PhysicsWorld {
    rigid_body_set: RigidBodySet,
    collider_set: ColliderSet,
    impulse_joint_set: ImpulseJointSet,
    multibody_joint_set: MultibodyJointSet,
    
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    ccd_solver: CCDSolver,
    query_pipeline: QueryPipeline,
    
    gravity: Vector<Real>,
    integration_parameters: IntegrationParameters,
}
```

### Snapshot Strategy

Use full clone for correctness. Optimize later if needed.

```rust
pub struct PhysicsSnapshot {
    pub tick: Tick,
    pub rigid_body_set: RigidBodySet,
    pub collider_set: ColliderSet,
    pub impulse_joint_set: ImpulseJointSet,
    pub multibody_joint_set: MultibodyJointSet,
}
```

**Memory estimate:**
- ~200 bytes per RigidBody
- ~150 bytes per Collider
- 10 players + 50 projectiles = ~21 KB per snapshot
- 128 snapshots = ~2.7 MB (acceptable)

### History Management

Ring buffer with O(1) tick lookup:

```rust
pub struct PhysicsHistory {
    snapshots: Vec<Option<PhysicsSnapshot>>,
    capacity: usize,
}

impl PhysicsHistory {
    pub fn push(&mut self, snapshot: PhysicsSnapshot) {
        let index = (snapshot.tick as usize) % self.capacity;
        self.snapshots[index] = Some(snapshot);
    }
    
    pub fn get(&self, tick: Tick) -> Option<&PhysicsSnapshot> {
        let index = (tick as usize) % self.capacity;
        self.snapshots[index].as_ref().filter(|s| s.tick == tick)
    }
}
```

### Restoration

```rust
impl PhysicsWorld {
    pub fn restore_from(&mut self, snapshot: &PhysicsSnapshot) {
        self.rigid_body_set = snapshot.rigid_body_set.clone();
        self.collider_set = snapshot.collider_set.clone();
        self.impulse_joint_set = snapshot.impulse_joint_set.clone();
        self.multibody_joint_set = snapshot.multibody_joint_set.clone();
        
        // Rebuild caches
        self.island_manager = IslandManager::new();
        self.broad_phase = DefaultBroadPhase::new();
        self.narrow_phase = NarrowPhase::new();
        self.query_pipeline = QueryPipeline::new();
        self.query_pipeline.update(&self.collider_set);
    }
}
```

## Reconciliation Flow

1. Receive server snapshot at tick T
2. Compare local state at T with server state
3. If diverged beyond threshold:
   - Restore physics to server state
   - Replay all inputs from T+1 to current tick
4. Prune history before T

```rust
pub fn reconcile(
    world: &mut PhysicsWorld,
    history: &PhysicsHistory,
    input_history: &InputHistory,
    server_state: &NetworkPhysicsState,
    current_tick: Tick,
) -> bool {
    let Some(local) = history.get(server_state.tick) else {
        return false;
    };
    
    if states_match(local, server_state) {
        return false;
    }
    
    world.restore_from(local);
    server_state.apply(world);
    
    for tick in (server_state.tick + 1)..=current_tick {
        if let Some(input) = input_history.get(tick) {
            world.step_with_input(input);
        } else {
            world.step();
        }
    }
    
    true
}
```

## Lag Compensation

Separate rewind world for historical raycasts:

```rust
pub struct LagCompensator {
    history: PhysicsHistory,
    rewind_world: PhysicsWorld,
}

impl LagCompensator {
    pub fn raycast_at_tick(
        &mut self,
        tick: Tick,
        ray: Ray,
        max_dist: f32,
        exclude: ColliderHandle,
    ) -> Option<HitResult> {
        let snapshot = self.history.get(tick)?;
        self.rewind_world.restore_from(snapshot);
        
        self.rewind_world.query_pipeline.cast_ray(
            &self.rewind_world.rigid_body_set,
            &self.rewind_world.collider_set,
            &ray,
            max_dist,
            true,
            QueryFilter::default().exclude_collider(exclude),
        )
    }
}
```

## Entity Mapping

Bidirectional mapping between game entities and physics handles:

```rust
pub struct EntityHandleMap {
    entity_to_body: HashMap<EntityId, RigidBodyHandle>,
    body_to_entity: HashMap<RigidBodyHandle, EntityId>,
}
```

## Configuration

```rust
pub const PHYSICS_TICK_RATE: f32 = 60.0;
pub const PHYSICS_DT: f32 = 1.0 / PHYSICS_TICK_RATE;
pub const PHYSICS_HISTORY_SIZE: usize = 128;
pub const MAX_LAG_COMPENSATION_MS: u64 = 200;
pub const RECONCILE_POSITION_THRESHOLD: f32 = 0.01;
pub const GRAVITY: [f32; 3] = [0.0, -9.81, 0.0];
```

## Implementation Steps

### Day 1: Basic Physics World
- Create `PhysicsWorld` struct
- Implement fixed timestep stepping
- Test scene: ground plane + dynamic box
- Verify determinism

### Day 2: Snapshot/Restore
- Implement `PhysicsSnapshot`
- Implement `PhysicsHistory`
- Implement `restore_from`
- Test: snapshot → step → restore → verify

### Day 3: Reconciliation
- Implement input history
- Implement reconciliation logic
- Test: simulate mismatch, verify correction

### Day 4: Lag Compensation
- Implement `LagCompensator`
- Implement `raycast_at_tick`
- Test: historical raycasts

### Day 5: Integration
- Wire into simulation loop
- Connect to network layer
- End-to-end test with simulated latency

## Pitfalls

- Rebuild query pipeline after restore (stale raycast data)
- Do not store island manager in snapshot (invalid indices)
- Use `set_position(..., true)` to wake sleeping bodies
- Maintain entity-handle mapping separately from physics
- Ensure identical compilation flags for determinism
