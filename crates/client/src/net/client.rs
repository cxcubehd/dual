use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use glam::Vec3;

use dual::{
    ClientConnection, ConnectionState, NetworkEndpoint, NetworkStats, Packet, PacketType,
    Reliability, WorldSnapshot,
};

use super::config::ClientConfig;
use super::input::InputState;
use super::interpolation::{InterpolatedEntity, InterpolationConfig, InterpolationEngine};
use super::prediction::ClientPrediction;

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
    last_command_time: Instant,
    command_interval: Duration,
    last_ping_time: Instant,
    ping_interval: Duration,
    connection_start_time: Option<Instant>,
    running: Arc<AtomicBool>,
    last_server_ack: u32,
    estimated_server_tick: u32,
    clock_offset_ms: i64,
}

impl NetworkClient {
    pub fn new(config: ClientConfig) -> io::Result<Self> {
        let mut endpoint = NetworkEndpoint::bind("0.0.0.0:0")?;
        endpoint.set_timeout(Duration::from_secs(config.connection_timeout_secs));

        let interpolation_config = InterpolationConfig {
            target_delay_ms: 100.0,
            min_buffer_snapshots: 3,
            max_buffer_snapshots: 64,
            time_correction_rate: 0.1,
            extrapolation_limit_ms: 250.0,
        };

        let tick_rate = config.server_tick_rate;
        let client_salt = Self::generate_salt();

        // Dummy connection initially
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
            last_command_time: Instant::now(),
            command_interval: Duration::from_secs_f64(1.0 / config.command_rate as f64),
            last_ping_time: Instant::now(),
            ping_interval: Duration::from_secs_f32(config.ping_interval_secs),
            connection_start_time: None,
            running: Arc::new(AtomicBool::new(true)),
            last_server_ack: 0,
            estimated_server_tick: 0,
            clock_offset_ms: 0,
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

    fn send_connection_request(&mut self) -> io::Result<()> {
        let packet = self.connection.send_packet(
            PacketType::ConnectionRequest {
                client_salt: self.client_salt,
            },
            Reliability::Unreliable,
        );
        self.endpoint.send(&packet)?;
        Ok(())
    }

    pub fn update(&mut self, delta_time: f32, input: Option<&InputState>) -> io::Result<()> {
        self.process_network()?;
        self.process_resends()?;

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

                if let Some(input) = input {
                    let command =
                        input.to_command(self.estimated_server_tick, self.command_sequence);
                    self.prediction.apply_input(&command, delta_time);

                    if self.last_command_time.elapsed() >= self.command_interval {
                        self.send_command(input)?;
                        self.last_command_time = Instant::now();
                    }
                }

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

        Ok(())
    }

    fn process_resends(&mut self) -> io::Result<()> {
        let packets = self.connection.collect_resends();
        for packet in packets {
            let _ = self.endpoint.send(&packet);
        }
        Ok(())
    }

    fn send_command(&mut self, input: &InputState) -> io::Result<()> {
        let command = input.to_command(self.estimated_server_tick, self.command_sequence);
        let sequence = self.command_sequence;
        self.command_sequence = self.command_sequence.wrapping_add(1);

        self.prediction.store_command(&command, sequence);

        let packet = self
            .connection
            .send_packet(PacketType::ClientCommand(command), Reliability::Unreliable);
        self.endpoint.send(&packet)?;

        Ok(())
    }

    fn send_ping(&mut self) -> io::Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let packet = self
            .connection
            .send_packet(PacketType::Ping { timestamp }, Reliability::Unreliable);
        self.endpoint.send(&packet)?;

        Ok(())
    }

    fn process_network(&mut self) -> io::Result<()> {
        let packets = self.endpoint.receive()?;

        for (packet, _addr) in packets {
            let payloads = self.connection.process_packet(packet);
            for payload in payloads {
                self.handle_payload(payload)?;
            }
        }

        Ok(())
    }

    fn handle_payload(&mut self, payload: PacketType) -> io::Result<()> {
        match payload {
            PacketType::ConnectionChallenge {
                server_salt,
                challenge,
            } => {
                self.handle_challenge(server_salt, challenge)?;
            }
            PacketType::ConnectionAccepted {
                client_id,
                entity_id,
            } => {
                self.handle_connection_accepted(client_id, entity_id)?;
            }
            PacketType::ConnectionDenied { reason } => {
                self.handle_connection_denied(&reason)?;
            }
            PacketType::WorldSnapshot(snapshot) => {
                self.handle_snapshot(snapshot)?;
            }
            PacketType::Pong { timestamp } => {
                self.handle_pong(timestamp)?;
            }
            PacketType::Disconnect => {
                log::info!("Disconnected by server");
                self.reset();
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_challenge(&mut self, server_salt: u64, challenge: u64) -> io::Result<()> {
        log::debug!("Received challenge from server");

        self.server_salt = Some(server_salt);
        self.state = ConnectionState::ChallengeResponse;
        self.connection.state = ConnectionState::ChallengeResponse;

        let expected_challenge = self.client_salt ^ server_salt;
        if challenge != expected_challenge {
            log::warn!("Challenge mismatch");
            return Ok(());
        }

        let packet = self.connection.send_packet(
            PacketType::ChallengeResponse {
                combined_salt: expected_challenge,
            },
            Reliability::Reliable,
        );
        self.endpoint.send(&packet)?;

        Ok(())
    }

    fn handle_connection_accepted(&mut self, client_id: u32, entity_id: u32) -> io::Result<()> {
        log::info!(
            "Connected to server with client ID {}, entity ID {}",
            client_id,
            entity_id
        );

        self.client_id = Some(client_id);
        self.entity_id = Some(entity_id);
        self.state = ConnectionState::Connected;
        self.connection.state = ConnectionState::Connected;
        self.connection.client_id = client_id;
        self.connection.entity_id = Some(entity_id);
        self.endpoint.set_state(ConnectionState::Connected);

        Ok(())
    }

    fn handle_connection_denied(&mut self, reason: &str) -> io::Result<()> {
        log::warn!("Connection denied: {}", reason);
        self.reset();
        Ok(())
    }

    fn handle_snapshot(&mut self, snapshot: WorldSnapshot) -> io::Result<()> {
        let received_tick = snapshot.tick;

        self.estimated_server_tick = snapshot
            .tick
            .saturating_add(self.config.interpolation_delay);

        self.last_server_ack = snapshot.last_command_ack;

        let local_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        self.clock_offset_ms = snapshot.server_time_ms as i64 - local_time;

        if let Some(entity_id) = self.entity_id {
            if let Some(local_state) = snapshot.entities.iter().find(|e| e.entity_id == entity_id) {
                let position = Vec3::from(local_state.position);
                let orientation_arr = local_state.decode_orientation();
                let orientation = glam::Quat::from_xyzw(
                    orientation_arr[0],
                    orientation_arr[1],
                    orientation_arr[2],
                    orientation_arr[3],
                );
                self.prediction
                    .reconcile(position, orientation, snapshot.last_command_ack);
            }
        }

        self.interpolation.push_snapshot(snapshot);

        self.send_snapshot_ack(received_tick)?;

        Ok(())
    }

    fn send_snapshot_ack(&mut self, received_tick: u32) -> io::Result<()> {
        let packet = self.connection.send_packet(
            PacketType::SnapshotAck { received_tick },
            Reliability::Unreliable,
        );
        self.endpoint.send(&packet)?;
        Ok(())
    }

    fn handle_pong(&mut self, timestamp: u64) -> io::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let rtt = now.saturating_sub(timestamp);
        log::debug!("Ping RTT: {} ms", rtt);

        Ok(())
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

    pub fn interpolation_stats(&self) -> super::interpolation::InterpolationStats {
        self.interpolation.debug_stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let config = ClientConfig::default();
        let client = NetworkClient::new(config);
        assert!(client.is_ok());

        let client = client.unwrap();
        assert_eq!(client.state(), ConnectionState::Disconnected);
    }
}
