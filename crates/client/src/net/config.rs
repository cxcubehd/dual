#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub server_tick_rate: u32,
    pub interpolation_delay: u32,
    pub connection_timeout_secs: u64,
    pub command_rate: u32,
    pub ping_interval_secs: f32,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_tick_rate: 60,
            interpolation_delay: 2,
            connection_timeout_secs: 120,
            command_rate: 60,
            ping_interval_secs: 0.25,
        }
    }
}
