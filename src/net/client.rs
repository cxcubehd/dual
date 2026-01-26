//! Network Client
//!
//! Implements the client-side networking for connecting to a game server,
//! receiving world snapshots, and sending input commands.
//!
//! # Client State Machine
//! ```text
//! ┌──────────────┐     ConnectionRequest      ┌──────────────┐
//! │ Disconnected │ ──────────────────────────▶│  Connecting  │
//! └──────────────┘                            └──────────────┘
//!        ▲                                           │
//!        │                                           │ Challenge
//!        │ Timeout                                   ▼
//!        │                                    ┌──────────────┐
//!        │◀─────────────────────────────────  │  Challenge   │
//!        │                                    │  Response    │
//!        │                                    └──────────────┘
//!        │                                           │
//!        │                                           │ Accepted
//!        │                                           ▼
//!        │      Disconnect                    ┌──────────────┐
//!        └◀───────────────────────────────────│  Connected   │
//!                                             └──────────────┘
//! ```

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::interpolation::{InterpolatedEntity, InterpolationEngine, JitterBufferConfig};
use super::protocol::{ClientCommand, Packet, PacketHeader, PacketType, WorldSnapshot};
use super::transport::{ConnectionState, NetworkEndpoint, NetworkStats};

/// Client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Server tick rate (must match server)
    pub server_tick_rate: u32,
    /// Interpolation delay in ticks
    pub interpolation_delay: u32,
    /// Connection timeout in seconds
    pub connection_timeout_secs: u64,
    /// Command send rate (commands per second)
    pub command_rate: u32,
    /// Ping interval in seconds
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

/// Input state to be sent to server
#[derive(Debug, Clone, Default)]
pub struct InputState {
    /// Movement direction (normalized)
    pub move_direction: [f32; 3],
    /// View angles (yaw, pitch) in radians
    pub view_yaw: f32,
    pub view_pitch: f32,
    /// Input flags
    pub sprint: bool,
    pub jump: bool,
    pub crouch: bool,
    pub fire1: bool,
    pub fire2: bool,
    pub use_key: bool,
    pub reload: bool,
}

impl InputState {
    /// Convert to a client command
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

/// Network client instance
pub struct NetworkClient {
    /// Network endpoint
    endpoint: NetworkEndpoint,
    /// Client configuration
    config: ClientConfig,
    /// Connection state
    state: ConnectionState,
    /// Assigned client ID (after connection)
    client_id: Option<u32>,
    /// Client's random salt for connection
    client_salt: u64,
    /// Server's challenge salt
    server_salt: Option<u64>,
    /// Interpolation engine
    interpolation: InterpolationEngine,
    /// Command sequence number
    command_sequence: u32,
    /// Last sent command time
    last_command_time: Instant,
    /// Command send interval
    command_interval: Duration,
    /// Last ping time
    last_ping_time: Instant,
    /// Ping interval
    ping_interval: Duration,
    /// Connection start time (for timeout)
    connection_start_time: Option<Instant>,
    /// Running flag
    running: Arc<AtomicBool>,
    /// Last acknowledged command from server
    last_server_ack: u32,
    /// Estimated server tick
    estimated_server_tick: u32,
    /// Clock sync offset (server_time - client_time)
    clock_offset_ms: i64,
}

impl NetworkClient {
    /// Create a new network client
    pub fn new(config: ClientConfig) -> io::Result<Self> {
        // Bind to any available port
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

    /// Generate a random salt for connection handshake
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

    /// Connect to a server
    pub fn connect(&mut self, server_addr: SocketAddr) -> io::Result<()> {
        log::info!("Connecting to {}", server_addr);

        self.endpoint.set_remote(server_addr);
        self.state = ConnectionState::Connecting;
        self.connection_start_time = Some(Instant::now());

        // Send connection request
        self.send_connection_request()?;

        Ok(())
    }

    /// Disconnect from the server
    pub fn disconnect(&mut self) -> io::Result<()> {
        if self.state == ConnectionState::Connected {
            let packet = self.endpoint.create_packet(PacketType::Disconnect);
            let _ = self.endpoint.send(&packet);
        }

        self.reset();
        Ok(())
    }

    /// Reset client state
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

    /// Send connection request
    fn send_connection_request(&mut self) -> io::Result<()> {
        let packet = self.endpoint.create_packet(PacketType::ConnectionRequest {
            client_salt: self.client_salt,
        });
        self.endpoint.send(&packet)?;
        Ok(())
    }

    /// Update the client (call every frame)
    pub fn update(&mut self, delta_time: f32, input: Option<&InputState>) -> io::Result<()> {
        // Process network
        self.process_network()?;

        // Handle connection state
        match self.state {
            ConnectionState::Connecting | ConnectionState::ChallengeResponse => {
                // Check for timeout
                if let Some(start) = self.connection_start_time {
                    if start.elapsed() > Duration::from_secs(self.config.connection_timeout_secs) {
                        log::warn!("Connection timeout");
                        self.reset();
                    }
                }
            }
            ConnectionState::Connected => {
                // Update interpolation
                self.interpolation.update(delta_time);

                // Send commands at fixed rate
                if let Some(input) = input {
                    if self.last_command_time.elapsed() >= self.command_interval {
                        self.send_command(input)?;
                        self.last_command_time = Instant::now();
                    }
                }

                // Send periodic pings
                if self.last_ping_time.elapsed() >= self.ping_interval {
                    self.send_ping()?;
                    self.last_ping_time = Instant::now();
                }

                // Check for server timeout
                if self.endpoint.is_timed_out() {
                    log::warn!("Server connection lost");
                    self.reset();
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Send a command to the server
    fn send_command(&mut self, input: &InputState) -> io::Result<()> {
        let command = input.to_command(self.estimated_server_tick, self.command_sequence);
        self.command_sequence = self.command_sequence.wrapping_add(1);

        let packet = self
            .endpoint
            .create_packet(PacketType::ClientCommand(command));
        self.endpoint.send(&packet)?;

        Ok(())
    }

    /// Send a ping
    fn send_ping(&mut self) -> io::Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let packet = self.endpoint.create_packet(PacketType::Ping { timestamp });
        self.endpoint.send(&packet)?;

        Ok(())
    }

    /// Process incoming network packets
    fn process_network(&mut self) -> io::Result<()> {
        let packets = self.endpoint.receive()?;

        for (packet, _addr) in packets {
            self.handle_packet(packet)?;
        }

        Ok(())
    }

    /// Handle a received packet
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

    /// Handle connection challenge
    fn handle_challenge(&mut self, server_salt: u64, challenge: u64) -> io::Result<()> {
        log::debug!("Received challenge from server");

        self.server_salt = Some(server_salt);
        self.state = ConnectionState::ChallengeResponse;

        // Verify and respond
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

    /// Handle connection accepted
    fn handle_connection_accepted(&mut self, client_id: u32) -> io::Result<()> {
        log::info!("Connected to server with client ID {}", client_id);

        self.client_id = Some(client_id);
        self.state = ConnectionState::Connected;
        self.endpoint.set_state(ConnectionState::Connected);

        Ok(())
    }

    /// Handle connection denied
    fn handle_connection_denied(&mut self, reason: &str) -> io::Result<()> {
        log::warn!("Connection denied: {}", reason);
        self.reset();
        Ok(())
    }

    /// Handle world snapshot
    fn handle_snapshot(&mut self, snapshot: WorldSnapshot) -> io::Result<()> {
        // Update server tick estimate
        self.estimated_server_tick = snapshot
            .tick
            .saturating_add(self.config.interpolation_delay);

        // Update last acknowledged command
        self.last_server_ack = snapshot.last_command_ack;

        // Calculate clock offset
        let local_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        self.clock_offset_ms = snapshot.server_time_ms as i64 - local_time;

        // Push to interpolation engine
        self.interpolation.push_snapshot(snapshot);

        Ok(())
    }

    /// Handle pong response
    fn handle_pong(&mut self, timestamp: u64) -> io::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let rtt = now.saturating_sub(timestamp);
        log::debug!("Ping RTT: {} ms", rtt);

        Ok(())
    }

    /// Get connection state
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }

    /// Get client ID (if connected)
    pub fn client_id(&self) -> Option<u32> {
        self.client_id
    }

    /// Get interpolated entity by ID
    pub fn get_entity(&self, entity_id: u32) -> Option<&InterpolatedEntity> {
        self.interpolation.get_entity(entity_id)
    }

    /// Get all interpolated entities
    pub fn entities(&self) -> impl Iterator<Item = &InterpolatedEntity> {
        self.interpolation.entities()
    }

    /// Check if interpolation engine is ready
    pub fn is_interpolation_ready(&self) -> bool {
        self.interpolation.is_ready()
    }

    /// Get network statistics
    pub fn stats(&self) -> &NetworkStats {
        self.endpoint.stats()
    }

    /// Get running flag
    pub fn running(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.running)
    }

    /// Shutdown the client
    pub fn shutdown(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        let _ = self.disconnect();
    }

    /// Get estimated server tick
    pub fn estimated_server_tick(&self) -> u32 {
        self.estimated_server_tick
    }

    /// Get clock offset (server - client time in ms)
    pub fn clock_offset_ms(&self) -> i64 {
        self.clock_offset_ms
    }

    /// Get interpolation statistics
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
