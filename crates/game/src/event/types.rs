use rkyv::{Archive, Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReliabilityMode {
    Unreliable,
    UnreliableExpiring { ttl_ms: u64 },
    Reliable,
}

impl ReliabilityMode {
    pub fn is_reliable(&self) -> bool {
        matches!(self, Self::Reliable)
    }

    pub fn ttl_ms(&self) -> Option<u64> {
        match self {
            Self::UnreliableExpiring { ttl_ms } => Some(*ttl_ms),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub enum GameEvent {
    PlayerKill {
        killer_id: u32,
        victim_id: u32,
        weapon_id: u8,
    },
    PlayerDeath {
        player_id: u32,
    },
    PlayerRespawn {
        player_id: u32,
        position: [f32; 3],
    },
    DamageDealt {
        attacker_id: u32,
        target_id: u32,
        damage: u16,
        hitbox: u8,
    },
    ProjectileFired {
        owner_id: u32,
        projectile_id: u32,
        weapon_id: u8,
    },
    ProjectileHit {
        projectile_id: u32,
        hit_entity_id: Option<u32>,
        position: [f32; 3],
    },
    ItemPickup {
        player_id: u32,
        item_id: u32,
        item_type: u8,
    },
    ItemDrop {
        player_id: u32,
        item_id: u32,
        position: [f32; 3],
    },
    ChatMessage {
        sender_id: u32,
        channel: u8,
        message: String,
    },
    VoiceData {
        sender_id: u32,
        data: Vec<u8>,
    },
    GameStateChange {
        new_state: u8,
    },
    RoundStart {
        round_number: u16,
    },
    RoundEnd {
        winning_team: u8,
    },
    ScoreUpdate {
        team_scores: [u16; 2],
    },
}

impl GameEvent {
    pub fn reliability(&self) -> ReliabilityMode {
        match self {
            Self::ChatMessage { .. } => ReliabilityMode::Reliable,
            Self::GameStateChange { .. } => ReliabilityMode::Reliable,
            Self::RoundStart { .. } => ReliabilityMode::Reliable,
            Self::RoundEnd { .. } => ReliabilityMode::Reliable,
            Self::ScoreUpdate { .. } => ReliabilityMode::Reliable,
            Self::PlayerRespawn { .. } => ReliabilityMode::Reliable,

            Self::PlayerKill { .. } => ReliabilityMode::UnreliableExpiring { ttl_ms: 10_000 },
            Self::PlayerDeath { .. } => ReliabilityMode::UnreliableExpiring { ttl_ms: 5_000 },
            Self::ItemPickup { .. } => ReliabilityMode::UnreliableExpiring { ttl_ms: 5_000 },
            Self::ItemDrop { .. } => ReliabilityMode::UnreliableExpiring { ttl_ms: 5_000 },

            Self::DamageDealt { .. } => ReliabilityMode::Unreliable,
            Self::ProjectileFired { .. } => ReliabilityMode::Unreliable,
            Self::ProjectileHit { .. } => ReliabilityMode::Unreliable,
            Self::VoiceData { .. } => ReliabilityMode::Unreliable,
        }
    }

    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::VoiceData { .. } | Self::DamageDealt { .. } | Self::ProjectileFired { .. }
        )
    }
}
