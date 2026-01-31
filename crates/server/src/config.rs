use dual::PacketLossSimulation;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub tick_rate: u32,
    pub max_clients: usize,
    pub snapshot_buffer_size: usize,
    pub snapshot_send_rate: u32,
    pub global_packet_loss: Option<PacketLossSimulation>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            tick_rate: 60,
            max_clients: 32,
            snapshot_buffer_size: 64,
            snapshot_send_rate: 1,
            global_packet_loss: None,
        }
    }
}
