mod connection;
mod endpoint;
pub mod lag_compensation;
mod protocol;
mod stats;
mod tracking;

pub use connection::{ClientConnection, ConnectionManager, ConnectionState, Reliability};
pub use endpoint::NetworkEndpoint;
pub use lag_compensation::LagCompensationManager;
pub use protocol::{ArchivedPacket, sequence_greater_than};
pub use protocol::{
    ClientCommand, DEFAULT_PORT, DEFAULT_TICK_RATE, MAX_PACKET_SIZE, PROTOCOL_MAGIC,
    PROTOCOL_VERSION, Packet, PacketError, PacketHeader, PacketType,
};
pub use stats::{NetworkStats, PacketLossSimulation};
pub use tracking::{AckTracker, PendingPacket, ReceiveTracker};
