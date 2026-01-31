pub mod client;
pub mod config;
pub mod input;
pub mod interpolation;
pub mod prediction;

pub use dual::{
    ClientCommand, ConnectionState, DEFAULT_PORT, DEFAULT_TICK_RATE, Entity, EntityState,
    EntityType, NetworkEndpoint, NetworkStats, Packet, PacketHeader, PacketType, SnapshotBuffer,
    World, WorldSnapshot,
};

pub use client::NetworkClient;
pub use config::ClientConfig;
pub use input::InputState;
pub use interpolation::{InterpolatedEntity, InterpolationEngine, InterpolationStats};
pub use prediction::ClientPrediction;
