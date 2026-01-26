//! Network Module
//!
//! Server-authoritative state synchronization system implementing
//! Source Engine / Quake-style networking patterns.
//!
//! # Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │                        Server-Authoritative Architecture                     │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │                                                                              │
//! │  ┌─────────────────────────────────────────────────────────────────────┐    │
//! │  │                           SERVER                                     │    │
//! │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐             │    │
//! │  │  │ Receive  │─▶│ Command  │─▶│ Simulate │─▶│ Snapshot │             │    │
//! │  │  │ Commands │  │ Queue    │  │ (Tick)   │  │ Generate │             │    │
//! │  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘             │    │
//! │  │       ▲                                          │                   │    │
//! │  │       │                                          ▼                   │    │
//! │  │       │     ┌────────────────────────────────────────┐              │    │
//! │  │       │     │          World State (Truth)           │              │    │
//! │  │       │     └────────────────────────────────────────┘              │    │
//! │  └───────┼──────────────────────────────────────────────────────────────┘    │
//! │          │                                          │                        │
//! │          │ UDP (Commands)              UDP (Snapshots)                       │
//! │          │                                          │                        │
//! │  ┌───────┴──────────────────────────────────────────┴───────────────────┐    │
//! │  │                           CLIENT                                      │    │
//! │  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐             │    │
//! │  │  │ Input    │─▶│ Command  │  │ Jitter   │─▶│ Interp   │─▶ Render   │    │
//! │  │  │ Sample   │  │ Send     │  │ Buffer   │  │ Engine   │             │    │
//! │  │  └──────────┘  └──────────┘  └──────────┘  └──────────┘             │    │
//! │  │                                   ▲                                   │    │
//! │  │                                   │                                   │    │
//! │  │                            Received Snapshots                        │    │
//! │  └───────────────────────────────────────────────────────────────────────┘    │
//! │                                                                              │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Key Components
//!
//! ## Protocol Layer ([`protocol`])
//! - Packet format with sequencing headers
//! - Acknowledgment bitfields for reliable delivery detection
//! - Entity state serialization with bandwidth optimization
//!
//! ## Snapshot System ([`snapshot`])
//! - Discrete world-state captures at fixed tick intervals
//! - Entity lifecycle management (spawn, update, despawn)
//! - Snapshot history buffer for lag compensation
//!
//! ## Interpolation Engine ([`interpolation`])
//! - Jitter buffer to smooth network variance
//! - LERP for positions, SLERP for orientations
//! - Hermite spline and SQUAD for advanced smoothing
//!
//! ## Transport Layer ([`transport`])
//! - UDP socket abstraction
//! - Connection handshake with challenge-response
//! - RTT estimation and packet loss tracking
//!
//! ## Server ([`server`])
//! - Fixed-frequency tick simulation
//! - Client command processing
//! - Authoritative world state management
//!
//! ## Client ([`client`])
//! - Connection state machine
//! - Input sampling and command generation
//! - Snapshot reception and interpolation
//!
//! # Example Usage
//!
//! ## Starting a Server
//! ```no_run
//! use dual::net::{GameServer, ServerConfig};
//!
//! let config = ServerConfig {
//!     tick_rate: 20,
//!     max_clients: 16,
//!     ..Default::default()
//! };
//!
//! let mut server = GameServer::new("0.0.0.0:27015", config).unwrap();
//! server.run();
//! ```
//!
//! ## Connecting a Client
//! ```no_run
//! use dual::net::{NetworkClient, ClientConfig, InputState};
//! use std::net::SocketAddr;
//!
//! let config = ClientConfig::default();
//! let mut client = NetworkClient::new(config).unwrap();
//!
//! let server_addr: SocketAddr = "127.0.0.1:27015".parse().unwrap();
//! client.connect(server_addr).unwrap();
//!
//! // Game loop
//! loop {
//!     let input = InputState::default();
//!     client.update(1.0 / 60.0, Some(&input)).unwrap();
//!     
//!     if client.is_connected() && client.is_interpolation_ready() {
//!         for entity in client.entities() {
//!             // Render entity at entity.position
//!         }
//!     }
//!     
//!     // Break condition...
//!     # break;
//! }
//! ```
//!
//! # Technical Details
//!
//! ## Tick Rate and Timing
//! - Server runs at a fixed tick rate (default 20 Hz)
//! - Snapshots generated each tick
//! - Clients interpolate between $T_{-1}$ and $T_0$ with configurable delay
//!
//! ## Packet Sequencing
//! - 32-bit sequence numbers with wrap-around handling
//! - Sliding window acknowledgment with 32-bit bitfield
//! - Duplicate detection via sequence tracking
//!
//! ## Bandwidth Optimization
//! - Position: 12 bytes (full f32 precision)
//! - Velocity: 6 bytes (i16, scaled by 100)
//! - Orientation: 8 bytes (quaternion as i16 * 4)
//! - View angles: 4 bytes (i16 * 2, radians * 10000)
//!
//! ## Future Extensions
//! This architecture provides the foundation for:
//! - Client-side prediction (predict ahead of server state)
//! - Lag compensation (rewind server state for hit detection)
//! - Delta compression (send only changed entity fields)

pub mod client;
pub mod interpolation;
pub mod protocol;
pub mod server;
pub mod snapshot;
pub mod transport;

// Re-export commonly used types
pub use client::{ClientConfig, InputState, NetworkClient};
pub use interpolation::{InterpolatedEntity, InterpolationEngine, InterpolationStats};
pub use protocol::{ClientCommand, EntityState, Packet, PacketHeader, PacketType, WorldSnapshot};
pub use server::{GameServer, ServerConfig, ServerStats};
pub use snapshot::{Entity, EntityType, SnapshotBuffer, World};
pub use transport::{ConnectionManager, ConnectionState, NetworkEndpoint, NetworkStats};

/// Default server port
pub const DEFAULT_PORT: u16 = 27015;

/// Default tick rate (ticks per second)
pub const DEFAULT_TICK_RATE: u32 = 20;

/// Tick duration for default tick rate
pub const DEFAULT_TICK_DURATION_MS: u32 = 1000 / DEFAULT_TICK_RATE;
