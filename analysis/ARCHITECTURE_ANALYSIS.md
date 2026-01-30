# Dual - Comprehensive Architecture Analysis

**Date:** January 26, 2026  
**Project:** Dual - Custom FPS Game Engine  
**Analysis Version:** 1.0

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Project Overview](#project-overview)
3. [Architectural Analysis](#architectural-analysis)
   - [Network Architecture](#network-architecture)
   - [Client-Server Model](#client-server-model)
   - [Game Loop Architecture](#game-loop-architecture)
4. [Detailed Component Analysis](#detailed-component-analysis)
   - [Protocol Layer](#protocol-layer)
   - [Transport Layer](#transport-layer)
   - [Snapshot System](#snapshot-system)
   - [Interpolation System](#interpolation-system)
   - [Server Logic](#server-logic)
   - [Client Logic](#client-logic)
5. [Comparison with CS:GO Architecture](#comparison-with-csgo-architecture)
6. [Issues and Concerns](#issues-and-concerns)
7. [Recommendations](#recommendations)
8. [Conclusion](#conclusion)

---

## Executive Summary

The **Dual** project is a Rust-based game engine with a well-structured networking foundation inspired by server-authoritative FPS games like CS:GO. The codebase demonstrates a solid understanding of fundamental networking concepts including:

- Client-server architecture with UDP transport
- Packet sequencing with ACK bitfields
- Entity state serialization with bandwidth optimization
- Snapshot-based world state synchronization
- Entity interpolation for smooth rendering

However, the project is in an **early stage of development** with several critical architectural components either missing or disconnected. The networking layer (`net/`) is well-designed but **not yet integrated** with the actual game loop (`app.rs`, `game/`). This analysis provides a thorough examination of the current state and recommendations for completion.

---

## Project Overview

### Tech Stack
| Component | Technology |
|-----------|------------|
| Language | Rust 2024 Edition |
| Graphics | wgpu 28.0.0 (WebGPU) |
| Windowing | winit 0.30.12 |
| Math | glam 0.31.0 |
| Serialization | rkyv 0.8 (zero-copy) |
| Async Runtime | tokio 1.x |
| Text Rendering | glyphon 0.10.0 |

### Module Structure
```
src/
├── main.rs          # Entry point
├── app.rs           # Window/event handling (ApplicationHandler)
├── game/            # Game state, input handling
│   ├── mod.rs
│   └── input.rs
├── net/             # Networking infrastructure
│   ├── mod.rs
│   ├── protocol.rs
│   ├── transport.rs
│   ├── server.rs
│   ├── client.rs
│   ├── snapshot.rs
│   └── interpolation.rs
├── render/          # Graphics rendering
│   ├── mod.rs
│   ├── camera.rs
│   ├── cube.rs
│   ├── debug_overlay.rs
│   └── vertex.rs
├── debug/           # Debug/stats infrastructure
│   ├── mod.rs
│   └── stats.rs
└── shaders/
    └── basic.wgsl
```

---

## Architectural Analysis

### Network Architecture

The networking layer follows industry-standard practices for real-time multiplayer games:

#### Protocol Design (`net/protocol.rs`)

**Strengths:**
1. **Magic Number & Version Check**: Packets begin with `PROTOCOL_MAGIC` (0x4455414C) and version number, preventing protocol mismatches.
   
2. **Sequence Number with Wraparound**: Proper handling of sequence number wraparound using `SEQUENCE_WRAP_THRESHOLD = u32::MAX / 2`:
   ```rust
   pub fn sequence_greater_than(s1: u32, s2: u32) -> bool {
       ((s1 > s2) && (s1 - s2 <= SEQUENCE_WRAP_THRESHOLD))
           || ((s1 < s2) && (s2 - s1 > SEQUENCE_WRAP_THRESHOLD))
   }
   ```

3. **Bandwidth-Efficient Encoding**: Entity states use compressed formats:
   - Velocity: `i16` scaled by 100 (max ±327.67 units/s)
   - Orientation: Quaternion as 4×`i16` scaled by 32767
   - Move direction: 3×`i8` scaled by 127
   - View angles: 2×`i16` scaled by 10000

4. **Zero-Copy Serialization**: Using `rkyv` for efficient serialization with `ArchivedPacket` access.

**Concerns:**
1. **View Angle Precision**: The encoding `(yaw * 10000.0) as i16` limits angles to ±3.2767 radians. This is **insufficient** for yaw which needs full 360° (2π ≈ 6.28 radians). This will cause **view angle clipping/wrapping issues**.

2. **MAX_PACKET_SIZE = 1200**: While safe for MTU, no fragmentation system exists for larger payloads (e.g., initial world state with many entities).

---

### Client-Server Model

#### Connection Handshake

The connection flow follows a challenge-response pattern:

```
Client                          Server
   |                               |
   |-- ConnectionRequest --------->|
   |   (client_salt)               |
   |                               |
   |<----- ConnectionChallenge ----|
   |   (server_salt, challenge)    |
   |                               |
   |-- ChallengeResponse --------->|
   |   (combined_salt)             |
   |                               |
   |<----- ConnectionAccepted -----|
   |   (client_id)                 |
```

**Strengths:**
- Salt-based challenge prevents connection spoofing
- Combined salt (`client_salt ^ server_salt`) validates both parties

**Concerns:**
- No encryption or authentication beyond salt exchange
- Vulnerable to replay attacks (no timestamp/nonce in challenge)

---

### Game Loop Architecture

#### Current Implementation (`app.rs`)

The game loop runs in `handle_redraw()`:
```rust
fn handle_redraw(&mut self, event_loop: &ActiveEventLoop) {
    let dt = game.update();           // Process input, update camera
    self.debug_stats.record_frame(dt);
    renderer.update_camera(&game.camera);
    renderer.render();
}
```

**Critical Issue:** The networking layer is **completely disconnected** from the game loop:
- `GameState` contains only local `Input` and `Camera`
- No `NetworkClient` or `NetworkServer` is instantiated
- The game runs in pure local/offline mode

---

## Detailed Component Analysis

### Protocol Layer

| Packet Type | Direction | Purpose | Status |
|-------------|-----------|---------|--------|
| `ConnectionRequest` | C→S | Initiate connection | ✅ Implemented |
| `ConnectionChallenge` | S→C | Anti-spoof challenge | ✅ Implemented |
| `ChallengeResponse` | C→S | Complete handshake | ✅ Implemented |
| `ConnectionAccepted` | S→C | Confirm connection | ✅ Implemented |
| `ConnectionDenied` | S→C | Reject connection | ✅ Implemented |
| `ClientCommand` | C→S | Input commands | ✅ Implemented |
| `WorldSnapshot` | S→C | World state | ✅ Implemented |
| `Ping` / `Pong` | Both | Latency measurement | ✅ Implemented |
| `Disconnect` | Both | Clean shutdown | ✅ Implemented |

**Missing Packet Types for Full FPS:**
- `DeltaSnapshot` - Only changed entities
- `ReliableMessage` - Chat, kill feed, etc.
- `VoiceData` - Voice chat
- `SpawnRequest` - Respawn request
- `HitRegistration` - Client-side hit detection hints

---

### Transport Layer (`net/transport.rs`)

#### ACK Tracking System

The implementation uses a **bitfield ACK system** similar to industry standards:

```rust
struct AckTracker {
    pending: VecDeque<PendingPacket>,
    srtt: f32,      // Smoothed RTT
    rtt_var: f32,   // RTT variance
}
```

**Strengths:**
1. **32-bit ACK Bitfield**: Acknowledges up to 33 packets (1 explicit + 32 in bitfield)
2. **SRTT Calculation**: Uses TCP-like smoothing (α=0.125, β=0.25)
3. **Duplicate Detection**: `ReceiveTracker` prevents processing duplicates

**Issues:**
1. **No Retransmission**: Packets are tracked but never retransmitted. The `pending` queue accumulates but unacked packets are silently dropped.
   
2. **Loss Detection Too Simple**: 
   ```rust
   self.stats.packet_loss_percent = (unacked / sent.max(1.0)) * 100.0;
   ```
   This conflates "in-flight" with "lost". Packets may just be in transit.

3. **No Congestion Control**: No bandwidth throttling or send rate adaptation.

---

### Snapshot System (`net/snapshot.rs`)

#### World State Management

```rust
pub struct World {
    tick: u32,
    entities: HashMap<u32, Entity>,
    next_entity_id: u32,
    removed_entities: Vec<u32>,
}
```

**Strengths:**
1. **Tick-Based Simulation**: Clean tick counter with `advance_tick()`
2. **Entity Dirty Flags**: Supports delta snapshots via `entity.dirty`
3. **Full + Delta Snapshots**: Both `generate_snapshot()` and `generate_delta_snapshot()` available

**Issues:**
1. **No Entity Removal in Snapshots**: `removed_entities` is tracked but never serialized into `WorldSnapshot`. Clients won't know when entities despawn.

2. **No Visibility System (PVS)**: All entities sent to all clients regardless of position. CS:GO uses Potentially Visible Sets to reduce bandwidth.

3. **SnapshotBuffer Linear Search**: `get_by_tick()` iterates through all slots:
   ```rust
   self.snapshots.iter().find_map(|s| s.as_ref().filter(|snap| snap.tick == tick))
   ```
   Should use `tick % capacity` for O(1) access.

---

### Interpolation System (`net/interpolation.rs`)

#### Entity Interpolation

The interpolation engine is **well-designed** with multiple techniques:

1. **Linear Interpolation (LERP)**: Position, velocity
2. **Spherical Linear Interpolation (SLERP)**: Quaternion orientation
3. **Hermite Spline Interpolation**: Available for smoother curves
4. **SQUAD Interpolation**: Advanced quaternion interpolation

```rust
fn interpolate_entity_states(from: &EntityState, to: &EntityState, t: f32) -> InterpolatedEntity {
    let position = from_pos.lerp(to_pos, t);
    let orientation = from_quat.slerp(to_quat, t);
    // ...
}
```

**Strengths:**
1. **Jitter Buffer**: Configurable buffer size (2-32 snapshots)
2. **Interpolation Delay**: Renders `N` ticks behind server for smooth playback
3. **Extrapolation Fallback**: When snapshots are late, extrapolates based on velocity
4. **Animation Time Wraparound**: Handles looping animations correctly

**Issues:**
1. **Render Time Drift**: If snapshots arrive faster/slower than expected, `render_time` may drift. No clock synchronization beyond initial estimate.

2. **No Cubic/Hermite for Position**: Uses linear lerp but has unused `hermite_interpolate()` function.

3. **Missing Entities Handling**: When entity appears in `to` but not `from`, it just snaps in rather than fading.

---

### Server Logic (`net/server.rs`)

#### Server Tick Loop

```rust
fn run(&mut self) {
    while self.running.load(Ordering::SeqCst) {
        // Fixed timestep accumulator
        while self.accumulator >= self.tick_duration {
            self.accumulator -= self.tick_duration;
            self.tick();
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn tick(&mut self) {
    self.process_commands();     // Apply client inputs
    self.simulate();             // Physics/game logic
    self.world.advance_tick();
    self.broadcast_snapshots();  // Send world state
}
```

**Strengths:**
1. **Fixed Timestep**: Proper accumulator pattern for deterministic simulation
2. **Command Queue**: Commands are queued and processed in order
3. **Authority Model**: Server applies movement, not clients

**Issues:**
1. **DEFAULT_TICK_RATE = 1**: The constant in `mod.rs` is set to 1 tick/second! This is a placeholder that needs to be updated to the target 120 Hz.

2. **Primitive Simulation Validation**: Commands are applied without deep bounds checking:
   ```rust
   fn apply_command(&mut self, entity_id: u32, command: &ClientCommand) {
       entity.velocity = world_move * speed;
       entity.position += entity.velocity * dt;
   }
   ```
   While anti-cheat is not a priority, basic simulation sanity (clamping movement to max allowed speed) is good practice for physics stability.

3. **No Lag Compensation**: Commands are applied at current server time, not client-perceived time. CS:GO rewinds the world to compensate for latency.

4. **Projectile Simulation Incomplete**: Gravity is applied but no collision detection:
   ```rust
   EntityType::Projectile => {
       entity.velocity.y -= 9.8 * dt;
       entity.position += entity.velocity * dt;
       if entity.position.y < 0.0 { /* floor clamp */ }
   }
   ```

---

### Client Logic (`net/client.rs`)

#### Client Update Loop

```rust
pub fn update(&mut self, delta_time: f32, input: Option<&InputState>) -> io::Result<()> {
    self.process_network()?;
    
    match self.state {
        ConnectionState::Connected => {
            self.interpolation.update(delta_time);
            
            if should_send_command {
                self.send_command(input)?;
            }
        }
        // ...
    }
}
```

**Issues:**
1. **No Client-Side Prediction**: The client has interpolation but no prediction. In CS:GO, clients simulate their own movement locally and reconcile with server snapshots. Currently:
   - Client sends input
   - Waits for server snapshot (RTT/2 + tick_rate delay)
   - Only then sees their movement
   
   This creates **perceivable input lag**.

2. **No Input Buffering**: Commands are sent every `command_interval` but there's no redundancy. If a packet is lost, that input is lost forever.

3. **Estimated Server Tick Calculation**:
   ```rust
   self.estimated_server_tick = snapshot.tick.saturating_add(self.config.interpolation_delay);
   ```
   This doesn't account for network latency. Should be:
   ```rust
   estimated_tick = snapshot.tick + (RTT_ms / tick_duration_ms) + interpolation_delay
   ```

---

## Comparison with CS:GO Architecture

| Feature | CS:GO | Dual | Status |
|---------|-------|------|--------|
| **Server Authority** | ✅ | ✅ | Correct model |
| **Tick Rate** | 64/128 | 120 (Target) | ✅ High fidelity target |
| **Client-Side Prediction** | ✅ | ❌ | **Missing** |
| **Lag Compensation** | ✅ | ❌ | **Missing** |
| **Entity Interpolation** | ✅ | ✅ | Well implemented |
| **Delta Compression** | ✅ | ⚠️ | Partial (dirty flags) |
| **Reliable Channel** | ✅ | ❌ | **Missing** |
| **String Tables** | ✅ | ❌ | Not needed yet |
| **PVS (Visibility)** | ✅ | ❌ | Not implemented |
| **Hit Registration** | ✅ Server-side | ❌ | No combat system |

---

## Issues and Concerns

### Critical Issues 🔴

1. **Networking Not Connected to Game Loop**
   - The `net/` module exists but `app.rs` and `game/` don't use it
   - Game runs purely locally with no multiplayer capability
   - This is the most significant gap

2. **No Client-Side Prediction**
   - Players will feel significant input lag (RTT + processing time)
   - Unplayable for any high-tickrate competitive scenario

3. **View Angle Encoding Overflow**
   - `(yaw * 10000.0) as i16` clips at ±3.2767 radians
   - Full rotation (2π ≈ 6.28) is not representable
   - Will cause visual glitches when looking around

4. **DEFAULT_TICK_RATE = 1**
   - Public constant is set to 1 tick/second
   - Likely a placeholder that needs correction to the target (120)

### Major Issues 🟠

5. **No Entity Removal Broadcast**
   - Clients won't know when entities despawn
   - Ghost entities will persist on client

6. **No Input Redundancy**
   - Lost command packets = lost input
   - Should send last N commands per packet

7. **No Reliable Messaging**
   - Chat, notifications, game events need reliability
   - Currently only unreliable UDP

8. **SnapshotBuffer Inefficient**
   - O(n) lookup instead of O(1)
   - Will become problematic at high tick rates (120 Hz)

### Minor Issues 🟡

9. **No Congestion Control**
   - Could flood low-bandwidth connections

10. **Salt Generation Not Cryptographic**
    - Uses `RandomState::build_hasher()` not CSPRNG
    - Acceptable for game logic

11. **Unused Hermite Interpolation**
    - Advanced interpolation code exists but isn't used

---

## Recommendations

### Immediate (Before First Playable)

1. **Integrate Networking with Game Loop**
   ```rust
   // In app.rs or game/mod.rs
   pub struct GameState {
       pub input: Input,
       pub camera: Camera,
       pub network: Option<NetworkClient>,  // Add this
       pub local_player_id: Option<u32>,    // Add this
   }
   ```

2. **Fix View Angle Encoding**
   ```rust
   // Use u16 for yaw with full rotation mapping
   pub fn encode_view_angles(&mut self, yaw: f32, pitch: f32) {
       // Normalize yaw to [0, 2π), then map to u16
       let yaw_normalized = yaw.rem_euclid(std::f32::consts::TAU);
       let yaw_encoded = (yaw_normalized / std::f32::consts::TAU * 65535.0) as u16;
       let pitch_encoded = (pitch.clamp(-PI/2, PI/2) * 10000.0) as i16;
       // ...
   }
   ```

3. **Fix DEFAULT_TICK_RATE**
   ```rust
   pub const DEFAULT_TICK_RATE: u32 = 120;  // Update to target 120 Hz
   pub const DEFAULT_TICK_DURATION_MS: u32 = 1000 / DEFAULT_TICK_RATE;
   ```

4. **Add Entity Removal to Snapshots**
   ```rust
   pub struct WorldSnapshot {
       // ...
       pub removed_entity_ids: Vec<u32>,  // Add this
   }
   ```

### Short-Term (For Playable Alpha)

5. **Implement Client-Side Prediction**
   - Store unacknowledged commands in ring buffer
   - Locally simulate player movement
   - On snapshot receive, rewind to last_ack and replay unacknowledged commands
   - Reconcile with server position (smooth correction if small delta)

6. **Add Command Redundancy**
   ```rust
   pub struct ClientCommandBatch {
       pub commands: Vec<ClientCommand>,  // Last 3-5 commands
       pub latest_sequence: u32,
   }
   ```

7. **Implement SnapshotBuffer O(1) Access**
   ```rust
   pub fn push(&mut self, snapshot: WorldSnapshot) {
       let index = (snapshot.tick as usize) % self.capacity;
       self.snapshots[index] = Some(snapshot);
   }
   
   pub fn get_by_tick(&self, tick: u32) -> Option<&WorldSnapshot> {
       let index = (tick as usize) % self.capacity;
       self.snapshots[index].as_ref().filter(|s| s.tick == tick)
   }
   ```

### Medium-Term (For Beta)

8. **Add Lag Compensation**
   - Store world state history (last 500ms+)
   - On hit detection, rewind to client's perceived time
   - Check hit against historical positions

9. **Add Reliable Message Channel**
   - Separate queue for guaranteed delivery
   - Retransmit until ACKed
   - Use for: chat, game events, player join/leave

10. **Add Simulation Guardrails**
    ```rust
    fn validate_command(&self, command: &ClientCommand) -> bool {
        let speed = command.decode_move_direction().length();
        speed <= 1.01  // Sanity check for physics stability
    }
    ```

### Long-Term (For Release)

11. **Implement PVS (Potentially Visible Sets)**
12. **Add Encryption (DTLS or custom)**
13. **Add Voice Chat Support**
14. **Optimize with Delta Compression**

---

## Conclusion

The **Dual** project demonstrates a solid foundation for a server-authoritative FPS game. The networking layer shows clear influence from professional game architectures like CS:GO, with proper:

- UDP-based transport with sequence numbers and ACK bitfields
- Entity state serialization with bandwidth-conscious encoding
- Snapshot interpolation for smooth client-side rendering
- Fixed-timestep server simulation

However, the project is currently at a **pre-alpha stage** where the networking infrastructure exists in isolation. The most critical next step is **integrating the network layer with the game loop** and implementing **client-side prediction** to achieve acceptable input responsiveness.

The codebase is well-organized, uses idiomatic Rust, and follows good practices like unit testing core functionality. With the issues addressed, this could become a functional multiplayer FPS foundation.

### Summary Scorecard

| Aspect | Score | Notes |
|--------|-------|-------|
| **Code Quality** | 8/10 | Clean, idiomatic Rust with tests |
| **Architecture Design** | 7/10 | Sound fundamentals, missing integration |
| **Network Protocol** | 7/10 | Good basics, needs reliable channel |
| **Server Logic** | 6/10 | Works but missing lag comp, validation |
| **Client Logic** | 5/10 | No prediction = unplayable latency |
| **Integration** | 2/10 | Network not connected to game |
| **Production Readiness** | 3/10 | Solid foundation, far from complete |

**Overall Assessment:** Promising early-stage project with correct architectural instincts. Needs significant work to achieve a playable multiplayer experience, but the foundation is sound.

---

*Analysis prepared for the Dual development team.*
