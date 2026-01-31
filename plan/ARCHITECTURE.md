# Architecture

Server-authoritative multiplayer FPS architecture.

## Threading Model

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   Main Thread   │     │  Sim Thread     │     │  Network Task   │
│                 │     │                 │     │  (tokio)        │
│  - Window       │     │  - Physics      │     │                 │
│  - Input        │────▶│  - Prediction   │◀───▶│  - UDP I/O      │
│  - Rendering    │◀────│  - Game Logic   │     │  - Serialization│
│  - Interpolation│     │                 │     │                 │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

### Thread Responsibilities

| Thread | Responsibility | Timing |
|--------|---------------|--------|
| Main | Window events, input, rendering | Variable (vsync) |
| Simulation | Physics, prediction, game state | Fixed (60 Hz) |
| Network | Async I/O, packet handling | Event-driven |

### Communication

Channels for all inter-thread communication. No shared mutable state.

| From | To | Data | Channel |
|------|-----|------|---------|
| Main | Sim | Player input | mpsc |
| Sim | Main | Predicted state | mpsc |
| Net | Sim | Server snapshots | mpsc |
| Sim | Net | Client commands | mpsc |

## State Management

### State Types

| Type | Scope | Persistence |
|------|-------|-------------|
| WorldSnapshot | Per-tick world state | Buffered |
| RenderState | Interpolated for display | Transient |
| GameEvent | One-shot occurrences | Consumed |

### Snapshot Buffer

Store last N snapshots for interpolation:

```rust
struct SnapshotBuffer {
    snapshots: VecDeque<TimestampedSnapshot>,
    capacity: usize,
}
```

Render time = current time - interpolation delay (~100ms).
Interpolate between two snapshots surrounding render time.

### Event Handling

Events are separate from state:
- **Snapshots**: "State at tick T" (persistent)
- **Events**: "What happened during tick T" (transient)

Events processed once, even if tick rendered multiple times.

## Client Prediction

### Flow

1. Client receives input
2. Store input with tick number
3. Apply input to local player immediately
4. Send input to server
5. When server snapshot arrives:
   - Compare predicted state with server state
   - If diverged: reset to server, replay unacknowledged inputs

### Input History

```rust
struct InputHistory {
    inputs: VecDeque<PlayerInput>,
    max_size: usize,
}
```

Retain inputs until acknowledged by server.

### Reconciliation Threshold

Position error > 1cm triggers reconciliation.
Smaller errors ignored to avoid jitter.

## Network Protocol

### Reliability Layer

UDP with custom reliability:

```rust
struct PacketHeader {
    protocol_id: u32,
    sequence: u16,
    ack: u16,
    ack_bits: u32,
}
```

- `sequence`: This packet's ID
- `ack`: Latest received remote sequence
- `ack_bits`: Previous 32 packets relative to `ack`

### Input Redundancy

Send last 3-5 inputs per packet. Survives individual packet loss.

```
Packet 1: [Input A]
Packet 2: [Input A, B]        <- Packet 1 lost
Packet 3: [Input A, B, C]     <- Server recovers A from packet 3
```

### Packet Types

| Type | Direction | Reliability |
|------|-----------|-------------|
| Input | C→S | Redundant |
| Snapshot | S→C | Unreliable |
| Events | S→C | Reliable |
| Connect/Disconnect | Both | Reliable |

## Server Architecture

### Tick Loop

```rust
fn tick(&mut self) {
    self.process_commands();
    self.simulate();
    self.world.advance_tick();
    self.broadcast_snapshots();
}
```

Fixed timestep with accumulator pattern.

### Input Buffering

Buffer client inputs by tick. Apply at appropriate server tick.
Handle out-of-order and late arrivals.

### Lag Compensation

For hit detection:
1. Client sends shot with perceived tick
2. Server rewinds world to that tick
3. Performs raycast against historical positions
4. Applies damage in current tick

Maximum compensation: ~200ms RTT.

## Fixed Timestep

Simulation uses accumulator pattern:

```rust
while accumulator >= tick_duration {
    fixed_update();
    accumulator -= tick_duration;
}
```

Spiral of death protection: clamp accumulated time.

## Rendering

### Interpolation

Remote entities interpolated between snapshots:
- Position: linear interpolation
- Rotation: spherical linear interpolation (SLERP)

Local player rendered at predicted position (no interpolation delay).

### Extrapolation

When snapshots late, extrapolate based on velocity.
Limit extrapolation time to avoid wild predictions.
