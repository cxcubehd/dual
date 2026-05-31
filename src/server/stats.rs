use crate::{NetworkStats, PacketLossSimulation};

#[derive(Debug, Clone)]
pub struct ServerStats {
    pub tick: u32,
    pub client_count: usize,
    pub max_clients: usize,
    pub entity_count: usize,
    pub network_stats: NetworkStats,
}

#[derive(Debug, Clone)]
pub struct ServerClientInfo {
    pub client_id: u32,
    pub addr: String,
    pub entity_id: Option<u32>,
    pub connected_secs: u64,
    pub last_ping_ms: f32,
    pub packet_loss_sim: PacketLossSimulation,
    pub incoming_packet_loss_sim: PacketLossSimulation,
}
