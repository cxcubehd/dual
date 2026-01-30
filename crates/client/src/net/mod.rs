pub mod client;
pub mod interpolation;

pub use dual_game::{
    ClientCommand, ConnectionState, Entity, EntityState, EntityType, NetworkEndpoint, NetworkStats,
    Packet, PacketHeader, PacketType, SnapshotBuffer, World, WorldSnapshot, DEFAULT_PORT,
    DEFAULT_TICK_RATE,
};

pub use client::{ClientConfig, InputState, NetworkClient};
pub use interpolation::{InterpolatedEntity, InterpolationEngine, InterpolationStats};
