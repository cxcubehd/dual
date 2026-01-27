pub mod client;
pub mod interpolation;
pub mod protocol;
pub mod server;
pub mod snapshot;
pub mod transport;

pub use client::{ClientConfig, InputState, NetworkClient};
pub use interpolation::{InterpolatedEntity, InterpolationEngine, InterpolationStats};
pub use protocol::{ClientCommand, EntityState, Packet, PacketHeader, PacketType, WorldSnapshot};
pub use server::{GameServer, ServerConfig, ServerStats};
pub use snapshot::{Entity, EntityType, SnapshotBuffer, World};
pub use transport::{ConnectionManager, ConnectionState, NetworkEndpoint, NetworkStats};

pub const DEFAULT_PORT: u16 = 27015;
pub const DEFAULT_TICK_RATE: u32 = 1;
pub const DEFAULT_TICK_DURATION_MS: u32 = 1000 / DEFAULT_TICK_RATE;
