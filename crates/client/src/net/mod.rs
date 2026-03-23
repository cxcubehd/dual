pub mod client;
pub mod config;
pub mod input;
pub mod interpolation;
pub mod prediction;

pub use dual::net::{
    ClientCommand, ConnectionState, NetworkEndpoint, NetworkStats, Packet, PacketHeader,
    PacketType, Reliability, DEFAULT_PORT, DEFAULT_TICK_RATE,
};
pub use dual::snapshot::{Entity, EntityKind, EntityState, EntityHandle, World, WorldSnapshot};

pub use client::NetworkClient;
pub use config::ClientConfig;
pub use input::InputState;
pub use interpolation::{InterpolatedEntity, InterpolationEngine, InterpolationStats};
pub use prediction::ClientPrediction;
