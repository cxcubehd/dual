mod lifecycle;
mod snapshots;
mod transport;

#[cfg(test)]
mod tests;

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use glam::Vec3;

use crate::player::InputState;
use crate::simulation::ClientPrediction;
use crate::snapshot::{
    InterpolatedEntity, InterpolationConfig, InterpolationEngine, InterpolationStats,
};
use crate::{
    ClientConnection, ConnectionState, NetworkEndpoint, NetworkStats, PacketType, Reliability,
};

use super::client_config::ClientConfig;

pub struct NetworkClient {
    endpoint: NetworkEndpoint,
    connection: ClientConnection,
    config: ClientConfig,
    state: ConnectionState,
    client_id: Option<u32>,
    entity_id: Option<u32>,
    client_salt: u64,
    server_salt: Option<u64>,
    interpolation: InterpolationEngine,
    prediction: ClientPrediction,
    command_sequence: u32,
    command_interval: Duration,
    last_ping_time: Instant,
    ping_interval: Duration,
    connection_start_time: Option<Instant>,
    running: Arc<AtomicBool>,
    last_server_ack: u32,
    estimated_server_tick: u32,
    clock_offset_ms: i64,
    input_accumulator: f32,
}

impl NetworkClient {
    pub fn new(config: ClientConfig) -> io::Result<Self> {
        let mut endpoint = NetworkEndpoint::bind("0.0.0.0:0")?;
        endpoint.set_timeout(Duration::from_secs(config.connection_timeout_secs));

        let interpolation_config = InterpolationConfig::default();
        let tick_rate = config.server_tick_rate;
        let client_salt = Self::generate_salt();
        let connection = ClientConnection::new("127.0.0.1:80".parse().unwrap(), 0, client_salt);

        Ok(Self {
            endpoint,
            connection,
            interpolation: InterpolationEngine::new(interpolation_config),
            prediction: ClientPrediction::new(tick_rate),
            state: ConnectionState::Disconnected,
            client_id: None,
            entity_id: None,
            client_salt,
            server_salt: None,
            command_sequence: 0,
            command_interval: Duration::from_secs_f64(1.0 / config.command_rate as f64),
            last_ping_time: Instant::now(),
            ping_interval: Duration::from_secs_f32(config.ping_interval_secs),
            connection_start_time: None,
            running: Arc::new(AtomicBool::new(true)),
            last_server_ack: 0,
            estimated_server_tick: 0,
            clock_offset_ms: 0,
            input_accumulator: 0.0,
            config,
        })
    }

    fn generate_salt() -> u64 {
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

    pub fn connect(&mut self, server_addr: SocketAddr) -> io::Result<()> {
        log::info!("Connecting to {}", server_addr);

        self.endpoint.set_remote(server_addr);
        self.state = ConnectionState::Connecting;
        self.connection_start_time = Some(Instant::now());

        self.connection = ClientConnection::new(server_addr, 0, self.client_salt);

        self.send_connection_request()?;

        Ok(())
    }

    pub fn disconnect(&mut self) -> io::Result<()> {
        if self.state == ConnectionState::Connected {
            let packet = self
                .connection
                .send_packet(PacketType::Disconnect, Reliability::Reliable);
            let _ = self.endpoint.send(&packet);
        }

        self.reset();
        Ok(())
    }

    fn reset(&mut self) {
        self.state = ConnectionState::Disconnected;
        self.client_id = None;
        self.entity_id = None;
        self.server_salt = None;
        self.client_salt = Self::generate_salt();
        self.interpolation.reset();
        self.prediction.reset();
        self.command_sequence = 0;
        self.connection_start_time = None;
        self.last_server_ack = 0;
        self.estimated_server_tick = 0;
    }

    pub fn update(&mut self, delta_time: f32, input: Option<&InputState>) -> io::Result<bool> {
        self.process_network()?;
        self.process_resends()?;

        let mut ticks_processed = false;

        match self.state {
            ConnectionState::Connecting | ConnectionState::ChallengeResponse => {
                if let Some(start) = self.connection_start_time {
                    if start.elapsed() > Duration::from_secs(self.config.connection_timeout_secs) {
                        log::warn!("Connection timeout");
                        self.reset();
                    }
                }
            }
            ConnectionState::Connected => {
                self.interpolation.update(delta_time);
                self.prediction.update(delta_time);

                self.input_accumulator += delta_time;
                let step = self.command_interval.as_secs_f32();

                while self.input_accumulator >= step {
                    self.input_accumulator -= step;
                    ticks_processed = true;

                    if let Some(input) = input {
                        let command =
                            input.to_command(self.estimated_server_tick, self.command_sequence);

                        self.prediction.prepare_tick();
                        self.prediction.apply_input(&command, step);
                        self.send_command(input)?;
                    }
                }

                let alpha = self.input_accumulator / step;
                self.prediction.update_visuals(alpha);

                if self.last_ping_time.elapsed() >= self.ping_interval {
                    self.send_ping()?;
                    self.last_ping_time = Instant::now();
                }

                if self
                    .connection
                    .is_timed_out(Duration::from_secs(self.config.connection_timeout_secs))
                {
                    log::warn!("Server connection lost");
                    self.reset();
                }
            }
            _ => {}
        }

        Ok(ticks_processed)
    }

    pub fn state(&self) -> ConnectionState {
        self.state
    }

    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    pub fn client_id(&self) -> Option<u32> {
        self.client_id
    }

    pub fn entity_id(&self) -> Option<u32> {
        self.entity_id
    }

    pub fn local_player(&self) -> Option<&InterpolatedEntity> {
        self.entity_id
            .and_then(|id| self.interpolation.get_entity(id))
    }

    pub fn predicted_position(&self) -> Vec3 {
        self.prediction.predicted_position()
    }

    pub fn predicted_orientation(&self) -> glam::Quat {
        self.prediction.predicted_orientation()
    }

    pub fn get_entity(&self, entity_id: u32) -> Option<&InterpolatedEntity> {
        self.interpolation.get_entity(entity_id)
    }

    pub fn entities(&self) -> impl Iterator<Item = &InterpolatedEntity> {
        self.interpolation.entities()
    }

    pub fn is_interpolation_ready(&self) -> bool {
        self.interpolation.is_ready()
    }

    pub fn stats(&self) -> &NetworkStats {
        self.endpoint.stats()
    }

    pub fn running(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.running)
    }

    pub fn shutdown(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = self.disconnect();
    }

    pub fn estimated_server_tick(&self) -> u32 {
        self.estimated_server_tick
    }

    pub fn clock_offset_ms(&self) -> i64 {
        self.clock_offset_ms
    }

    pub fn interpolation_stats(&self) -> InterpolationStats {
        self.interpolation.debug_stats()
    }
}
