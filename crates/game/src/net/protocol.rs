use rkyv::{rancor, Archive, Deserialize, Serialize};

pub const MAX_PACKET_SIZE: usize = 1200;
pub const PROTOCOL_VERSION: u32 = 1;
pub const PROTOCOL_MAGIC: u32 = 0x4455414C;
pub const DEFAULT_PORT: u16 = 27015;
pub const DEFAULT_TICK_RATE: u32 = 60;

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

    pub fn is_valid(&self) -> bool {
        self.magic == PROTOCOL_MAGIC && self.version == PROTOCOL_VERSION
    }
}

#[inline]
pub fn sequence_greater_than(s1: u32, s2: u32) -> bool {
    ((s1 > s2) && (s1 - s2 <= SEQUENCE_WRAP_THRESHOLD))
        || ((s1 < s2) && (s2 - s1 > SEQUENCE_WRAP_THRESHOLD))
}

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
    },
    ConnectionDenied {
        reason: String,
    },
    ClientCommand(ClientCommand),
    WorldSnapshot(WorldSnapshot),
    Ping {
        timestamp: u64,
    },
    Pong {
        timestamp: u64,
    },
    Disconnect,
    LobbyList(Vec<LobbyInfo>),
    LobbyJoin {
        lobby_id: u64,
    },
    LobbyLeave,
    QueueJoin,
    QueueLeave,
    QueueStatus {
        position: u32,
        estimated_wait_secs: u32,
    },
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct LobbyInfo {
    pub id: u64,
    pub name: String,
    pub player_count: u8,
    pub max_players: u8,
    pub has_password: bool,
    pub map_name: String,
    pub game_mode: String,
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct ClientCommand {
    pub tick: u32,
    pub command_sequence: u32,
    pub move_direction: [i8; 3],
    pub view_angles: [i16; 2],
    pub input_flags: u16,
}

impl ClientCommand {
    pub const FLAG_SPRINT: u16 = 1 << 0;
    pub const FLAG_JUMP: u16 = 1 << 1;
    pub const FLAG_CROUCH: u16 = 1 << 2;
    pub const FLAG_FIRE1: u16 = 1 << 3;
    pub const FLAG_FIRE2: u16 = 1 << 4;
    pub const FLAG_USE: u16 = 1 << 5;
    pub const FLAG_RELOAD: u16 = 1 << 6;

    pub fn new(tick: u32, command_sequence: u32) -> Self {
        Self {
            tick,
            command_sequence,
            move_direction: [0, 0, 0],
            view_angles: [0, 0],
            input_flags: 0,
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

#[derive(Debug, Clone, Copy, Default, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct EntityState {
    pub entity_id: u32,
    pub entity_type: u8,
    pub position: [f32; 3],
    pub velocity: [i16; 3],
    pub orientation: [i16; 4],
    pub animation_state: u8,
    pub animation_frame: u8,
    pub flags: u16,
}

impl EntityState {
    pub const MAX_VELOCITY: f32 = 327.67;

    pub fn new(entity_id: u32, entity_type: u8) -> Self {
        Self {
            entity_id,
            entity_type,
            position: [0.0; 3],
            velocity: [0; 3],
            orientation: [0, 0, 0, 32767],
            animation_state: 0,
            animation_frame: 0,
            flags: 0,
        }
    }

    pub fn encode_velocity(&mut self, vel: [f32; 3]) {
        self.velocity = [
            (vel[0].clamp(-Self::MAX_VELOCITY, Self::MAX_VELOCITY) * 100.0) as i16,
            (vel[1].clamp(-Self::MAX_VELOCITY, Self::MAX_VELOCITY) * 100.0) as i16,
            (vel[2].clamp(-Self::MAX_VELOCITY, Self::MAX_VELOCITY) * 100.0) as i16,
        ];
    }

    pub fn decode_velocity(&self) -> [f32; 3] {
        [
            self.velocity[0] as f32 / 100.0,
            self.velocity[1] as f32 / 100.0,
            self.velocity[2] as f32 / 100.0,
        ]
    }

    pub fn encode_orientation(&mut self, quat: [f32; 4]) {
        self.orientation = [
            (quat[0].clamp(-1.0, 1.0) * 32767.0) as i16,
            (quat[1].clamp(-1.0, 1.0) * 32767.0) as i16,
            (quat[2].clamp(-1.0, 1.0) * 32767.0) as i16,
            (quat[3].clamp(-1.0, 1.0) * 32767.0) as i16,
        ];
    }

    pub fn decode_orientation(&self) -> [f32; 4] {
        [
            self.orientation[0] as f32 / 32767.0,
            self.orientation[1] as f32 / 32767.0,
            self.orientation[2] as f32 / 32767.0,
            self.orientation[3] as f32 / 32767.0,
        ]
    }
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct WorldSnapshot {
    pub tick: u32,
    pub server_time_ms: u64,
    pub last_command_ack: u32,
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
