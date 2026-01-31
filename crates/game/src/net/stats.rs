#[derive(Debug, Clone, Default)]
pub struct PacketLossSimulation {
    pub enabled: bool,
    pub loss_percent: f32,
    pub min_latency_ms: u32,
    pub max_latency_ms: u32,
    pub jitter_ms: u32,
}

impl PacketLossSimulation {
    pub fn should_drop(&self) -> bool {
        if !self.enabled || self.loss_percent <= 0.0 {
            return false;
        }
        rand_percent() < (self.loss_percent / 100.0)
    }

    pub fn delay_ms(&self) -> u32 {
        if !self.enabled || self.max_latency_ms == 0 {
            return 0;
        }
        let base = self.min_latency_ms;
        let range = self.max_latency_ms.saturating_sub(self.min_latency_ms);
        let jitter = if self.jitter_ms > 0 {
            (rand_percent() * self.jitter_ms as f32) as u32
        } else {
            0
        };
        base + (rand_percent() * range as f32) as u32 + jitter
    }
}

#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub packets_lost: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub rtt_ms: f32,
    pub rtt_variance: f32,
    pub packet_loss_percent: f32,
}

pub fn rand_percent() -> f32 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_u64(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64,
    );
    (hasher.finish() % 10000) as f32 / 10000.0
}

pub fn rand_u64() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_u64(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64,
    );
    hasher.finish()
}
