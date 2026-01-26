//! Network Protocol Layer
//!
//! Defines the packet structure, sequencing headers, and serialization schema
//! for the server-authoritative state synchronization system.
//!
//! # Packet Structure
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Packet Header (12 bytes)                  │
//! ├──────────────┬──────────────┬──────────────┬────────────────┤
//! │  Sequence    │  Ack         │  Ack Bitfield│   Packet Type  │
//! │  (u32)       │  (u32)       │  (u32)       │   (u8)         │
//! └──────────────┴──────────────┴──────────────┴────────────────┘
//! │                    Payload (variable)                        │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use rkyv::{rancor, Archive, Deserialize, Serialize};

#[allow(unused_imports)]
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

/// Maximum transmission unit for UDP packets (conservative for internet)
pub const MAX_PACKET_SIZE: usize = 1200;

/// Protocol version for compatibility checking
pub const PROTOCOL_VERSION: u32 = 1;

/// Magic bytes to identify our protocol
pub const PROTOCOL_MAGIC: u32 = 0x4455414C; // "DUAL" in ASCII

/// Sequence number wrapping threshold for comparison
const SEQUENCE_WRAP_THRESHOLD: u32 = u32::MAX / 2;

/// Packet header with sequencing and acknowledgment information.
///
/// Uses a sliding window acknowledgment system similar to Quake 3's networking:
/// - `sequence`: Monotonically increasing packet number
/// - `ack`: Last received packet sequence from remote
/// - `ack_bitfield`: Bitmask of the 32 packets before `ack` (1 = received)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub struct PacketHeader {
    /// Protocol magic number for validation
    pub magic: u32,
    /// Protocol version
    pub version: u32,
    /// This packet's sequence number
    pub sequence: u32,
    /// Last acknowledged sequence from remote peer
    pub ack: u32,
    /// Bitfield of previous 32 acknowledged packets
    pub ack_bitfield: u32,
}

impl PacketHeader {
    pub fn new(sequence: u32, ack: u32, ack_bitfield: u32) -> Self {
        Self {
            magic: PROTOCOL_MAGIC,
            version: PROTOCOL_VERSION,
            sequence,
            ack,
            ack_bitfield,
        }
    }

    /// Validates the packet header
    pub fn is_valid(&self) -> bool {
        self.magic == PROTOCOL_MAGIC && self.version == PROTOCOL_VERSION
    }
}

/// Compare two sequence numbers accounting for wrap-around.
/// Returns true if s1 > s2 (more recent).
#[inline]
pub fn sequence_greater_than(s1: u32, s2: u32) -> bool {
    ((s1 > s2) && (s1 - s2 <= SEQUENCE_WRAP_THRESHOLD))
        || ((s1 < s2) && (s2 - s1 > SEQUENCE_WRAP_THRESHOLD))
}

/// Types of packets in the protocol
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub enum PacketType {
    /// Connection request from client
    ConnectionRequest { client_salt: u64 },
    /// Connection challenge from server
    ConnectionChallenge { server_salt: u64, challenge: u64 },
    /// Challenge response from client
    ChallengeResponse { combined_salt: u64 },
    /// Connection accepted
    ConnectionAccepted { client_id: u32 },
    /// Connection denied with reason
    ConnectionDenied { reason: String },
    /// Client input commands
    ClientCommand(ClientCommand),
    /// Server world state snapshot
    WorldSnapshot(WorldSnapshot),
    /// Keep-alive ping
    Ping { timestamp: u64 },
    /// Keep-alive pong
    Pong { timestamp: u64 },
    /// Graceful disconnect
    Disconnect,
}

/// Client input command sent to server for processing.
///
/// Commands are timestamped with the server tick they were intended for,
/// enabling server-side lag compensation.
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct ClientCommand {
    /// Server tick this command targets
    pub tick: u32,
    /// Sequence number for command acknowledgment
    pub command_sequence: u32,
    /// Movement direction (normalized, compressed)
    pub move_direction: [i8; 3],
    /// View angles (yaw, pitch) in fixed-point (radians * 10000)
    pub view_angles: [i16; 2],
    /// Input flags (jump, crouch, fire, etc.)
    pub input_flags: u16,
}

impl ClientCommand {
    /// Input flag: Sprint
    pub const FLAG_SPRINT: u16 = 1 << 0;
    /// Input flag: Jump
    pub const FLAG_JUMP: u16 = 1 << 1;
    /// Input flag: Crouch
    pub const FLAG_CROUCH: u16 = 1 << 2;
    /// Input flag: Primary fire
    pub const FLAG_FIRE1: u16 = 1 << 3;
    /// Input flag: Secondary fire
    pub const FLAG_FIRE2: u16 = 1 << 4;
    /// Input flag: Use/Interact
    pub const FLAG_USE: u16 = 1 << 5;
    /// Input flag: Reload
    pub const FLAG_RELOAD: u16 = 1 << 6;

    /// Creates a new client command with the given parameters
    pub fn new(tick: u32, command_sequence: u32) -> Self {
        Self {
            tick,
            command_sequence,
            move_direction: [0, 0, 0],
            view_angles: [0, 0],
            input_flags: 0,
        }
    }

    /// Decode move direction to f32 vector
    pub fn decode_move_direction(&self) -> [f32; 3] {
        [
            self.move_direction[0] as f32 / 127.0,
            self.move_direction[1] as f32 / 127.0,
            self.move_direction[2] as f32 / 127.0,
        ]
    }

    /// Encode move direction from f32 vector
    pub fn encode_move_direction(&mut self, dir: [f32; 3]) {
        self.move_direction = [
            (dir[0].clamp(-1.0, 1.0) * 127.0) as i8,
            (dir[1].clamp(-1.0, 1.0) * 127.0) as i8,
            (dir[2].clamp(-1.0, 1.0) * 127.0) as i8,
        ];
    }

    /// Decode view angles to radians
    pub fn decode_view_angles(&self) -> (f32, f32) {
        (
            self.view_angles[0] as f32 / 10000.0,
            self.view_angles[1] as f32 / 10000.0,
        )
    }

    /// Encode view angles from radians
    pub fn encode_view_angles(&mut self, yaw: f32, pitch: f32) {
        self.view_angles = [(yaw * 10000.0) as i16, (pitch * 10000.0) as i16];
    }

    #[inline]
    pub fn has_flag(&self, flag: u16) -> bool {
        self.input_flags & flag != 0
    }

    #[inline]
    pub fn set_flag(&mut self, flag: u16, value: bool) {
        if value {
            self.input_flags |= flag;
        } else {
            self.input_flags &= !flag;
        }
    }
}

/// Entity state for network synchronization.
///
/// Optimized for bandwidth with fixed-point encoding:
/// - Position: Full precision f32 (12 bytes)
/// - Velocity: Compressed to i16 (6 bytes)
/// - Orientation: Quaternion compressed to i16 (8 bytes)
#[derive(Debug, Clone, Copy, Default, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct EntityState {
    /// Entity ID
    pub entity_id: u32,
    /// Entity type for polymorphic handling
    pub entity_type: u8,
    /// Position in world space (full precision)
    pub position: [f32; 3],
    /// Velocity (scaled: actual = encoded / 100.0)
    pub velocity: [i16; 3],
    /// Orientation quaternion (scaled: actual = encoded / 32767.0)
    pub orientation: [i16; 4],
    /// Animation state index
    pub animation_state: u8,
    /// Animation frame (0-255 normalized)
    pub animation_frame: u8,
    /// Entity-specific flags
    pub flags: u16,
}

impl EntityState {
    /// Maximum velocity that can be encoded (327.67 units/sec)
    pub const MAX_VELOCITY: f32 = 327.67;

    pub fn new(entity_id: u32, entity_type: u8) -> Self {
        Self {
            entity_id,
            entity_type,
            position: [0.0; 3],
            velocity: [0; 3],
            orientation: [0, 0, 0, 32767], // Identity quaternion (w=1)
            animation_state: 0,
            animation_frame: 0,
            flags: 0,
        }
    }

    /// Encode velocity from f32 vector
    pub fn encode_velocity(&mut self, vel: [f32; 3]) {
        self.velocity = [
            (vel[0].clamp(-Self::MAX_VELOCITY, Self::MAX_VELOCITY) * 100.0) as i16,
            (vel[1].clamp(-Self::MAX_VELOCITY, Self::MAX_VELOCITY) * 100.0) as i16,
            (vel[2].clamp(-Self::MAX_VELOCITY, Self::MAX_VELOCITY) * 100.0) as i16,
        ];
    }

    /// Decode velocity to f32 vector
    pub fn decode_velocity(&self) -> [f32; 3] {
        [
            self.velocity[0] as f32 / 100.0,
            self.velocity[1] as f32 / 100.0,
            self.velocity[2] as f32 / 100.0,
        ]
    }

    /// Encode orientation from unit quaternion [x, y, z, w]
    pub fn encode_orientation(&mut self, quat: [f32; 4]) {
        self.orientation = [
            (quat[0].clamp(-1.0, 1.0) * 32767.0) as i16,
            (quat[1].clamp(-1.0, 1.0) * 32767.0) as i16,
            (quat[2].clamp(-1.0, 1.0) * 32767.0) as i16,
            (quat[3].clamp(-1.0, 1.0) * 32767.0) as i16,
        ];
    }

    /// Decode orientation to unit quaternion [x, y, z, w]
    pub fn decode_orientation(&self) -> [f32; 4] {
        [
            self.orientation[0] as f32 / 32767.0,
            self.orientation[1] as f32 / 32767.0,
            self.orientation[2] as f32 / 32767.0,
            self.orientation[3] as f32 / 32767.0,
        ]
    }
}

/// World state snapshot at a specific server tick.
///
/// Contains the complete authoritative state of all entities
/// that the client needs to render.
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct WorldSnapshot {
    /// Server tick number (monotonically increasing)
    pub tick: u32,
    /// Server timestamp in milliseconds since server start
    pub server_time_ms: u64,
    /// Last acknowledged client command sequence
    pub last_command_ack: u32,
    /// Entity states in this snapshot
    pub entities: Vec<EntityState>,
}

impl WorldSnapshot {
    pub fn new(tick: u32, server_time_ms: u64) -> Self {
        Self {
            tick,
            server_time_ms,
            last_command_ack: 0,
            entities: Vec::new(),
        }
    }
}

/// Complete network packet
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct Packet {
    pub header: PacketHeader,
    pub payload: PacketType,
}

/// Error type for packet serialization/deserialization
#[derive(Debug)]
pub enum PacketError {
    /// Serialization failed
    SerializeError(rancor::Error),
    /// Deserialization/validation failed  
    DeserializeError(rancor::Error),
}

impl std::fmt::Display for PacketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PacketError::SerializeError(e) => write!(f, "Serialization error: {}", e),
            PacketError::DeserializeError(e) => write!(f, "Deserialization error: {}", e),
        }
    }
}

impl std::error::Error for PacketError {}

impl Packet {
    pub fn new(header: PacketHeader, payload: PacketType) -> Self {
        Self { header, payload }
    }

    /// Serialize packet to bytes using rkyv zero-copy serialization
    pub fn serialize(&self) -> Result<Vec<u8>, PacketError> {
        rkyv::to_bytes::<rancor::Error>(self)
            .map(|aligned| aligned.into_vec())
            .map_err(PacketError::SerializeError)
    }

    /// Deserialize packet from bytes with validation
    pub fn deserialize(data: &[u8]) -> Result<Self, PacketError> {
        rkyv::from_bytes::<Self, rancor::Error>(data).map_err(PacketError::DeserializeError)
    }

    /// Access archived packet without deserialization (zero-copy)
    pub fn access_archived(data: &[u8]) -> Result<&ArchivedPacket, PacketError> {
        rkyv::access::<ArchivedPacket, rancor::Error>(data).map_err(PacketError::DeserializeError)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_comparison() {
        assert!(sequence_greater_than(2, 1));
        assert!(!sequence_greater_than(1, 2));
        // Test wrap-around
        assert!(sequence_greater_than(0, u32::MAX));
        assert!(!sequence_greater_than(u32::MAX, 0));
    }

    #[test]
    fn test_entity_state_encoding() {
        let mut state = EntityState::new(1, 0);
        state.position = [100.5, 50.25, -30.0];
        state.encode_velocity([10.5, -5.25, 0.0]);
        state.encode_orientation([0.0, 0.0, 0.0, 1.0]);

        let vel = state.decode_velocity();
        assert!((vel[0] - 10.5).abs() < 0.01);
        assert!((vel[1] - -5.25).abs() < 0.01);

        let quat = state.decode_orientation();
        assert!((quat[3] - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_packet_serialization() {
        let header = PacketHeader::new(1, 0, 0);
        let payload = PacketType::Ping { timestamp: 12345 };
        let packet = Packet::new(header, payload);

        let serialized = packet.serialize().unwrap();
        let deserialized = Packet::deserialize(&serialized).unwrap();

        assert_eq!(packet.header, deserialized.header);
    }
}
