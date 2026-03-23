use rkyv::{Archive, Deserialize, Serialize, rancor};

use crate::snapshot::WorldSnapshot;

pub const MAX_PACKET_SIZE: usize = 1200;
pub const PROTOCOL_VERSION: u32 = 2;
pub const PROTOCOL_MAGIC: u32 = 0x4455414C;
pub const DEFAULT_PORT: u16 = 27015;
pub const DEFAULT_TICK_RATE: u32 = 128;

const SEQUENCE_WRAP_THRESHOLD: u32 = u32::MAX / 2;

fn normalize_angle(angle: f32) -> f32 {
    let two_pi = std::f32::consts::TAU;
    let mut normalized = angle % two_pi;
    if normalized > std::f32::consts::PI {
        normalized -= two_pi;
    } else if normalized < -std::f32::consts::PI {
        normalized += two_pi;
    }
    normalized
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Archive, Serialize, Deserialize)]
#[rkyv(compare(PartialEq), derive(Debug))]
pub struct PacketHeader {
    pub magic: u32,
    pub version: u32,
    pub sequence: u32,
    pub ack: u32,
    pub ack_bitfield: u32,
    pub channel: u8,
    pub channel_seq: u16,
}

impl PacketHeader {
    pub const CHANNEL_UNRELIABLE: u8 = 0;
    pub const CHANNEL_RELIABLE: u8 = 1;
    pub const CHANNEL_ORDERED: u8 = 2;

    pub fn new(sequence: u32, ack: u32, ack_bitfield: u32, channel: u8, channel_seq: u16) -> Self {
        Self {
            magic: PROTOCOL_MAGIC,
            version: PROTOCOL_VERSION,
            sequence,
            ack,
            ack_bitfield,
            channel,
            channel_seq,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.magic == PROTOCOL_MAGIC && self.version == PROTOCOL_VERSION
    }
}

#[inline]
pub fn sequence_greater_than(s1: u32, s2: u32) -> bool {
    ((s1 > s2) && (s1 - s2 <= SEQUENCE_WRAP_THRESHOLD))
        || ((s1 < s2) && (s2 - s1 > SEQUENCE_WRAP_THRESHOLD))
}

// ---------------------------------------------------------------------------
// Packet types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub enum PacketType {
    ConnectionRequest {
        client_salt: u64,
    },
    ConnectionChallenge {
        server_salt: u64,
        challenge: u64,
    },
    ChallengeResponse {
        combined_salt: u64,
    },
    ConnectionAccepted {
        client_id: u32,
        entity_id: u32,
        tick_rate: u32,
    },
    ConnectionDenied {
        reason: String,
    },
    /// Client input – contains the last N commands for redundancy.
    ClientInput(Vec<ClientCommand>),
    /// Server world state – delta-compressed against client's last acked baseline.
    WorldSnapshot(WorldSnapshot),
    Ping {
        timestamp: u64,
    },
    Pong {
        timestamp: u64,
    },
    SnapshotAck {
        received_tick: u32,
    },
    Disconnect,
}

// ---------------------------------------------------------------------------
// Client input command
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct ClientCommand {
    pub tick: u32,
    pub command_sequence: u32,
    pub move_direction: [i8; 3],
    pub view_angles: [i16; 2],
    pub input_flags: u16,
    /// Command duration in microseconds (for sub-tick precision).
    pub duration_us: u16,
}

impl ClientCommand {
    pub const FLAG_SPRINT: u16 = 1 << 0;
    pub const FLAG_JUMP: u16 = 1 << 1;
    pub const FLAG_CROUCH: u16 = 1 << 2;
    pub const FLAG_FIRE1: u16 = 1 << 3;
    pub const FLAG_FIRE2: u16 = 1 << 4;
    pub const FLAG_USE: u16 = 1 << 5;
    pub const FLAG_RELOAD: u16 = 1 << 6;
    pub const FLAG_JUMP_HELD: u16 = 1 << 7;

    pub fn new(tick: u32, command_sequence: u32) -> Self {
        Self {
            tick,
            command_sequence,
            move_direction: [0, 0, 0],
            view_angles: [0, 0],
            input_flags: 0,
            duration_us: 0,
        }
    }

    pub fn decode_move_direction(&self) -> [f32; 3] {
        [
            self.move_direction[0] as f32 / 127.0,
            self.move_direction[1] as f32 / 127.0,
            self.move_direction[2] as f32 / 127.0,
        ]
    }

    pub fn encode_move_direction(&mut self, dir: [f32; 3]) {
        self.move_direction = [
            (dir[0].clamp(-1.0, 1.0) * 127.0) as i8,
            (dir[1].clamp(-1.0, 1.0) * 127.0) as i8,
            (dir[2].clamp(-1.0, 1.0) * 127.0) as i8,
        ];
    }

    pub fn decode_view_angles(&self) -> (f32, f32) {
        (
            self.view_angles[0] as f32 / 10000.0,
            self.view_angles[1] as f32 / 10000.0,
        )
    }

    pub fn encode_view_angles(&mut self, yaw: f32, pitch: f32) {
        let normalized_yaw = normalize_angle(yaw);
        self.view_angles = [(normalized_yaw * 10000.0) as i16, (pitch * 10000.0) as i16];
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

// ---------------------------------------------------------------------------
// Packet wrapper
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct Packet {
    pub header: PacketHeader,
    pub payload: PacketType,
}

#[derive(Debug, thiserror::Error)]
pub enum PacketError {
    #[error("serialization failed: {0}")]
    Serialize(rancor::Error),
    #[error("deserialization failed: {0}")]
    Deserialize(rancor::Error),
}

impl Packet {
    pub fn new(header: PacketHeader, payload: PacketType) -> Self {
        Self { header, payload }
    }

    pub fn serialize(&self) -> Result<Vec<u8>, PacketError> {
        rkyv::to_bytes::<rancor::Error>(self)
            .map(|aligned| aligned.into_vec())
            .map_err(PacketError::Serialize)
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, PacketError> {
        rkyv::from_bytes::<Self, rancor::Error>(data).map_err(PacketError::Deserialize)
    }

    pub fn access_archived(data: &[u8]) -> Result<&ArchivedPacket, PacketError> {
        rkyv::access::<ArchivedPacket, rancor::Error>(data).map_err(PacketError::Deserialize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequence_comparison() {
        assert!(sequence_greater_than(2, 1));
        assert!(!sequence_greater_than(1, 2));
        assert!(sequence_greater_than(0, u32::MAX));
        assert!(!sequence_greater_than(u32::MAX, 0));
    }

    #[test]
    fn test_packet_serialization() {
        let header = PacketHeader::new(1, 0, 0, PacketHeader::CHANNEL_UNRELIABLE, 0);
        let payload = PacketType::Ping { timestamp: 12345 };
        let packet = Packet::new(header, payload);

        let serialized = packet.serialize().unwrap();
        let deserialized = Packet::deserialize(&serialized).unwrap();

        assert_eq!(packet.header, deserialized.header);
    }
}
