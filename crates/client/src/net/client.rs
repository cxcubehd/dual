use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use dual::{
    ClientCommand, ConnectionState, NetworkEndpoint, NetworkStats, Packet, PacketType,
    WorldSnapshot,
};

use super::interpolation::{InterpolatedEntity, InterpolationEngine, JitterBufferConfig};

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
            server_tick_rate: 20,
            interpolation_delay: 2,
            connection_timeout_secs: 10,
            command_rate: 60,
            ping_interval_secs: 1.0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct InputState {
    pub move_direction: [f32; 3],
    pub view_yaw: f32,
    pub view_pitch: f32,
    pub sprint: bool,
    pub jump: bool,
    pub crouch: bool,
    pub fire1: bool,
    pub fire2: bool,
    pub use_key: bool,
    pub reload: bool,
}

impl InputState {
    pub fn to_command(&self, tick: u32, sequence: u32) -> ClientCommand {
        let mut cmd = ClientCommand::new(tick, sequence);
        cmd.encode_move_direction(self.move_direction);
        cmd.encode_view_angles(self.view_yaw, self.view_pitch);

        if self.sprint {
            cmd.set_flag(ClientCommand::FLAG_SPRINT, true);
        }
        if self.jump {
            cmd.set_flag(ClientCommand::FLAG_JUMP, true);
        }
        if self.crouch {
            cmd.set_flag(ClientCommand::FLAG_CROUCH, true);
        }
        if self.fire1 {
            cmd.set_flag(ClientCommand::FLAG_FIRE1, true);
        }
        if self.fire2 {
            cmd.set_flag(ClientCommand::FLAG_FIRE2, true);
        }
        if self.use_key {
            cmd.set_flag(ClientCommand::FLAG_USE, true);
        }
        if self.reload {
            cmd.set_flag(ClientCommand::FLAG_RELOAD, true);
        }

        cmd
    }
}

pub struct NetworkClient {
    endpoint: NetworkEndpoint,
    config: ClientConfig,
    state: ConnectionState,
    client_id: Option<u32>,
    client_salt: u64,
    server_salt: Option<u64>,
    interpolation: InterpolationEngine,
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
        let endpoint = NetworkEndpoint::bind("0.0.0.0:0")?;

        let tick_duration = 1.0 / config.server_tick_rate as f32;
        let interpolation_config = JitterBufferConfig {
            min_buffer_size: 2,
            max_buffer_size: 32,
            interpolation_delay: config.interpolation_delay,
            tick_duration_secs: tick_duration,
        };

        Ok(Self {
            endpoint,
            interpolation: InterpolationEngine::new(interpolation_config),
            state: ConnectionState::Disconnected,
            client_id: None,
            client_salt: Self::generate_salt(),
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

        self.send_connection_request()?;

        Ok(())
    }

    pub fn disconnect(&mut self) -> io::Result<()> {
        if self.state == ConnectionState::Connected {
            let packet = self.endpoint.create_packet(PacketType::Disconnect);
            let _ = self.endpoint.send(&packet);
        }

        self.reset();
        Ok(())
    }

    fn reset(&mut self) {
        self.state = ConnectionState::Disconnected;
        self.client_id = None;
        self.server_salt = None;
        self.client_salt = Self::generate_salt();
        self.interpolation.reset();
        self.command_sequence = 0;
        self.connection_start_time = None;
        self.last_server_ack = 0;
        self.estimated_server_tick = 0;
    }

    fn send_connection_request(&mut self) -> io::Result<()> {
        let packet = self.endpoint.create_packet(PacketType::ConnectionRequest {
            client_salt: self.client_salt,
        });
        self.endpoint.send(&packet)?;
        Ok(())
    }

    pub fn update(&mut self, delta_time: f32, input: Option<&InputState>) -> io::Result<()> {
        self.process_network()?;

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
                    if self.last_command_time.elapsed() >= self.command_interval {
                        self.send_command(input)?;
                        self.last_command_time = Instant::now();
                    }
                }

                if self.last_ping_time.elapsed() >= self.ping_interval {
                    self.send_ping()?;
                    self.last_ping_time = Instant::now();
                }

                if self.endpoint.is_timed_out() {
                    log::warn!("Server connection lost");
                    self.reset();
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn send_command(&mut self, input: &InputState) -> io::Result<()> {
        let command = input.to_command(self.estimated_server_tick, self.command_sequence);
        self.command_sequence = self.command_sequence.wrapping_add(1);

        let packet = self
            .endpoint
            .create_packet(PacketType::ClientCommand(command));
        self.endpoint.send(&packet)?;

        Ok(())
    }

    fn send_ping(&mut self) -> io::Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let packet = self.endpoint.create_packet(PacketType::Ping { timestamp });
        self.endpoint.send(&packet)?;

        Ok(())
    }

    fn process_network(&mut self) -> io::Result<()> {
        let packets = self.endpoint.receive()?;

        for (packet, _addr) in packets {
            self.handle_packet(packet)?;
        }

        Ok(())
    }

    fn handle_packet(&mut self, packet: Packet) -> io::Result<()> {
        match packet.payload {
            PacketType::ConnectionChallenge {
                server_salt,
                challenge,
            } => {
                self.handle_challenge(server_salt, challenge)?;
            }
            PacketType::ConnectionAccepted { client_id } => {
                self.handle_connection_accepted(client_id)?;
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

        let expected_challenge = self.client_salt ^ server_salt;
        if challenge != expected_challenge {
            log::warn!("Challenge mismatch");
            return Ok(());
        }

        let packet = self.endpoint.create_packet(PacketType::ChallengeResponse {
            combined_salt: expected_challenge,
        });
        self.endpoint.send(&packet)?;

        Ok(())
    }

    fn handle_connection_accepted(&mut self, client_id: u32) -> io::Result<()> {
        log::info!("Connected to server with client ID {}", client_id);

        self.client_id = Some(client_id);
        self.state = ConnectionState::Connected;
        self.endpoint.set_state(ConnectionState::Connected);

        Ok(())
    }

    fn handle_connection_denied(&mut self, reason: &str) -> io::Result<()> {
        log::warn!("Connection denied: {}", reason);
        self.reset();
        Ok(())
    }

    fn handle_snapshot(&mut self, snapshot: WorldSnapshot) -> io::Result<()> {
        self.estimated_server_tick = snapshot
            .tick
            .saturating_add(self.config.interpolation_delay);

        self.last_server_ack = snapshot.last_command_ack;

        let local_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        self.clock_offset_ms = snapshot.server_time_ms as i64 - local_time;

        self.interpolation.push_snapshot(snapshot);

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

    #[test]
    fn test_input_to_command() {
        let input = InputState {
            move_direction: [1.0, 0.0, 0.0],
            view_yaw: std::f32::consts::FRAC_PI_4,
            view_pitch: 0.0,
            sprint: true,
            jump: false,
            crouch: false,
            fire1: true,
            fire2: false,
            use_key: false,
            reload: false,
        };

        let command = input.to_command(10, 1);

        assert_eq!(command.tick, 10);
        assert_eq!(command.command_sequence, 1);
        assert!(command.has_flag(ClientCommand::FLAG_SPRINT));
        assert!(command.has_flag(ClientCommand::FLAG_FIRE1));
        assert!(!command.has_flag(ClientCommand::FLAG_JUMP));

        let decoded = command.decode_move_direction();
        assert!((decoded[0] - 1.0).abs() < 0.01);
    }
}
