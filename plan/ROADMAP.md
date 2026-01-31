# Development Roadmap

## Current State

Pre-alpha. Core subsystems implemented but not integrated:

| Subsystem | Status | Notes |
|-----------|--------|-------|
| Graphics | Working | wgpu renderer, skybox, models |
| Input | Working | Keyboard/mouse, capture |
| Camera | Working | FPS controls, noclip flight |
| Network Protocol | Implemented | Not connected to game loop |
| Interpolation | Implemented | Entity interpolation ready |
| Client Prediction | Missing | Required for playable netcode |
| Physics | Missing | Rapier integration planned |

## Phase 1: Core Integration

**Goal:** Connect existing networking to game loop.

### Tasks

1. **Integrate NetworkClient into GameState**
   - Add `NetworkClient` to game state
   - Connect to server on startup
   - Send inputs over network

2. **Integrate Server Snapshots**
   - Receive snapshots from server
   - Update remote entity positions
   - Display other players

3. **Fix Protocol Issues**
   - Update `DEFAULT_TICK_RATE` from 1 to 60/120
   - Fix view angle encoding (yaw overflow at 2π)
   - Add entity removal to snapshots

### Deliverable

Two clients can connect to a server and see each other move.

## Phase 2: Client Prediction

**Goal:** Responsive local player movement.

### Tasks

1. **Input History**
   - Store inputs with tick numbers
   - Retain unacknowledged inputs

2. **Local Simulation**
   - Predict local player movement
   - Apply inputs immediately

3. **Reconciliation**
   - Compare prediction with server state
   - Rewind and replay on mismatch

4. **Input Redundancy**
   - Send last N inputs per packet
   - Handle packet loss gracefully

### Deliverable

Local movement feels instant. Prediction errors correct smoothly.

## Phase 3: Physics Integration

**Goal:** Rapier 3D for collision and movement.

### Tasks

1. **PhysicsWorld Setup**
   - Initialize Rapier components
   - Fixed timestep stepping
   - Determinism verification

2. **Snapshot/Restore**
   - Clone-based physics snapshots
   - Ring buffer history (128 ticks)
   - Fast state restoration

3. **Reconciliation with Physics**
   - Restore physics state
   - Replay inputs through physics
   - Smooth correction

4. **Lag Compensation**
   - Separate rewind world
   - Historical raycasts
   - Hit detection at client-perceived time

### Deliverable

Players collide with world geometry. Hit detection works fairly across latencies.

## Phase 4: Gameplay

**Goal:** Playable FPS prototype.

### Tasks

1. **Player Spawning**
   - Spawn points
   - Respawn system

2. **Weapons**
   - Hitscan weapons
   - Projectile weapons
   - Ammo/reload

3. **Health/Damage**
   - Damage calculation
   - Death/respawn
   - Kill feed

4. **Basic Map**
   - Static geometry
   - Collision meshes

### Deliverable

Playable deathmatch prototype.

## Phase 5: Polish

**Goal:** Production-quality experience.

### Tasks

- Audio system
- Particle effects
- UI/HUD
- Server browser
- Match system
- Anti-cheat basics
- Performance optimization

## Technical Debt

Items to address throughout development:

| Issue | Priority | Phase |
|-------|----------|-------|
| SnapshotBuffer O(1) lookup | Medium | 1 |
| Reliable message channel | Medium | 2 |
| Delta compression | Low | 3 |
| PVS (visibility culling) | Low | 5 |
| Encryption | Low | 5 |
