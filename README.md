# Dual

![Screenshot](image.png)

Custom multiplayer FPS engine written in Rust. Server-authoritative architecture inspired by Quake and Counter-Strike.

**Status:** Pre-alpha. Core rendering and networking implemented, integration in progress.

## Architecture

Server-authoritative multiplayer with client-side prediction:

| Component | Description |
|-----------|-------------|
| Server | Authoritative simulation, snapshot broadcasting |
| Client | Prediction, reconciliation, entity interpolation |
| Protocol | UDP with reliability layer, zero-copy serialization |

See [`plan/ARCHITECTURE.md`](plan/ARCHITECTURE.md) for details.

## Tech Stack

| Component | Library |
|-----------|---------|
| Graphics | wgpu 28.0 (WebGPU) |
| Windowing | winit 0.30 |
| Math | glam 0.31 |
| Serialization | rkyv 0.8 (zero-copy) |
| Async Runtime | tokio |
| Physics | Rapier 3D 0.32 (planned) |

## Project Structure

```
crates/
├── client/          # Game client
│   ├── src/
│   │   ├── game/    # Input, state management
│   │   ├── net/     # Networking, protocol
│   │   ├── render/  # Graphics, camera
│   │   └── debug/   # Stats, overlays
│   └── assets/      # Models, textures, shaders
└── demo/            # Serialization experiments
```

## Controls

| Action | Key |
|--------|-----|
| Move | W A S D |
| Fly Up | Space |
| Fly Down | Ctrl |
| Look | Mouse |
| Sprint | Shift |
| Release Mouse | Esc |
| Fullscreen | F11 |
| Quit | Shift + F12 |

## Building

Requires Rust 1.75+.

```bash
# Development
cargo run -p client

# Release
cargo run -p client --release
```

## Development

### Code Style

Follow [`AGENTS.md`](AGENTS.md) for coding guidelines. Key points:
- Self-documenting code, minimal comments
- Docstrings for public APIs
- Strong typing for domain concepts
- Minimize allocations in hot paths

### Before Committing

```bash
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo build --release
cargo test
```

## Roadmap

See [`plan/ROADMAP.md`](plan/ROADMAP.md).

| Phase | Focus |
|-------|-------|
| 1 | Network integration |
| 2 | Client prediction |
| 3 | Physics (Rapier 3D) |
| 4 | Gameplay prototype |
| 5 | Polish |

## License

MIT
