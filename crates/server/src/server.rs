use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use glam::Vec3;

use dual::{
    ClientCommand, ConnectionManager, ConnectionState, EntityType, NetworkEndpoint, NetworkStats,
    Packet, PacketHeader, PacketType, SnapshotBuffer, World,
};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub tick_rate: u32,
    pub max_clients: usize,
    pub snapshot_buffer_size: usize,
    pub snapshot_send_rate: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            tick_rate: 60,
            max_clients: 32,
            snapshot_buffer_size: 64,
            snapshot_send_rate: 1,
        }
    }
}

#[derive(Debug)]
struct QueuedCommand {
    client_id: u32,
    command: ClientCommand,
}

pub struct GameServer {
    endpoint: NetworkEndpoint,
    connections: ConnectionManager,
    config: ServerConfig,
    world: World,
    snapshot_history: SnapshotBuffer,
    command_queue: VecDeque<QueuedCommand>,
    tick: u32,
    tick_duration: Duration,
    last_tick_time: Instant,
    accumulator: Duration,
    running: Arc<AtomicBool>,
    #[allow(dead_code)]
    start_time: Instant,
    pending_events: VecDeque<ServerEvent>,
}

#[derive(Debug, Clone)]
pub enum ServerEvent {
    ClientConnecting {
        addr: SocketAddr,
    },
    ClientConnected {
        client_id: u32,
        addr: SocketAddr,
        entity_id: u32,
    },
    ClientDisconnected {
        client_id: u32,
        reason: DisconnectReason,
    },
    ConnectionDenied {
        addr: SocketAddr,
        reason: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum DisconnectReason {
    Graceful,
    Timeout,
    Kicked,
}

impl DisconnectReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            DisconnectReason::Graceful => "disconnected",
            DisconnectReason::Timeout => "timed out",
            DisconnectReason::Kicked => "kicked",
        }
    }
}

impl GameServer {
    pub fn new(bind_addr: &str, config: ServerConfig) -> io::Result<Self> {
        let mut endpoint = NetworkEndpoint::bind(bind_addr)?;
        endpoint.set_server_mode(true);
        let tick_duration = Duration::from_secs_f64(1.0 / config.tick_rate as f64);

        let mut pending_events = VecDeque::new();
        pending_events.push_back(ServerEvent::ClientConnecting {
            addr: endpoint.local_addr(),
        });

        Ok(Self {
            endpoint,
            connections: ConnectionManager::new(config.max_clients),
            world: World::new(),
            snapshot_history: SnapshotBuffer::new(config.snapshot_buffer_size),
            command_queue: VecDeque::new(),
            tick: 0,
            tick_duration,
            last_tick_time: Instant::now(),
            accumulator: Duration::ZERO,
            running: Arc::new(AtomicBool::new(true)),
            start_time: Instant::now(),
            pending_events: VecDeque::new(),
            config,
        })
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.endpoint.local_addr()
    }

    pub fn running(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.running)
    }

    pub fn drain_events(&mut self) -> impl Iterator<Item = ServerEvent> + '_ {
        self.pending_events.drain(..)
    }

    pub fn run(&mut self) {
        while self.running.load(Ordering::SeqCst) {
            self.tick_once();
            std::thread::sleep(Duration::from_millis(1));
        }
        self.shutdown_connections();
    }

    pub fn shutdown_connections(&mut self) {
        let client_ids: Vec<u32> = self.connections.iter().map(|c| c.client_id).collect();
        for client_id in client_ids {
            self.kick_client(client_id);
        }
    }

    pub fn kick_client(&mut self, client_id: u32) {
        if let Some(client) = self.connections.get(client_id) {
            let addr = client.addr;
            let header = PacketHeader::new(0, 0, 0);
            let packet = Packet::new(header, PacketType::Disconnect);
            let _ = self.endpoint.send_to(&packet, addr);
        }

        if let Some(client) = self.connections.remove(client_id) {
            if let Some(entity_id) = client.entity_id {
                self.world.despawn_entity(entity_id);
            }
            self.pending_events
                .push_back(ServerEvent::ClientDisconnected {
                    client_id,
                    reason: DisconnectReason::Kicked,
                });
        }
    }

    pub fn tick_once(&mut self) {
        let now = Instant::now();
        let delta = now - self.last_tick_time;
        self.last_tick_time = now;
        self.accumulator += delta;

        if let Err(e) = self.process_network() {
            self.pending_events.push_back(ServerEvent::Error {
                message: format!("Network error: {}", e),
            });
        }

        while self.accumulator >= self.tick_duration {
            self.accumulator -= self.tick_duration;
            self.tick();
        }
    }

    fn tick(&mut self) {
        self.process_commands();
        self.simulate();

        self.world.advance_tick();
        self.tick = self.world.tick();

        let snapshot = self.world.generate_snapshot(0);
        self.snapshot_history.push(snapshot);

        if self.tick % self.config.snapshot_send_rate == 0 {
            self.broadcast_snapshots();
        }

        let timed_out = self.connections.cleanup_timed_out();
        for client_id in timed_out {
            self.pending_events
                .push_back(ServerEvent::ClientDisconnected {
                    client_id,
                    reason: DisconnectReason::Timeout,
                });
        }
    }

    fn process_commands(&mut self) {
        while let Some(queued) = self.command_queue.pop_front() {
            if let Some(client) = self.connections.get_mut(queued.client_id) {
                if queued.command.command_sequence > client.last_command_ack {
                    client.last_command_ack = queued.command.command_sequence;
                }

                if let Some(entity_id) = client.entity_id {
                    self.apply_command(entity_id, &queued.command);
                }
            }
        }
    }

    fn apply_command(&mut self, entity_id: u32, command: &ClientCommand) {
        let Some(entity) = self.world.get_entity_mut(entity_id) else {
            return;
        };

        let dt = 1.0 / self.config.tick_rate as f32;
        let move_dir = command.decode_move_direction();
        let (yaw, pitch) = command.decode_view_angles();

        let speed = if command.has_flag(ClientCommand::FLAG_SPRINT) {
            10.0
        } else {
            5.0
        };

        let move_vec = Vec3::new(move_dir[0], move_dir[1], move_dir[2]);
        if move_vec.length_squared() > 0.001 {
            let normalized = move_vec.normalize();

            let (sin_yaw, cos_yaw) = yaw.sin_cos();
            let world_move = Vec3::new(
                normalized.x * cos_yaw - normalized.z * sin_yaw,
                normalized.y,
                normalized.x * sin_yaw + normalized.z * cos_yaw,
            );

            entity.velocity = world_move * speed;
            entity.position += entity.velocity * dt;
        } else {
            entity.velocity = Vec3::ZERO;
        }

        entity.orientation = glam::Quat::from_euler(glam::EulerRot::YXZ, yaw, pitch, 0.0);
        entity.dirty = true;
    }

    fn simulate(&mut self) {
        let dt = 1.0 / self.config.tick_rate as f32;

        for entity in self.world.entities_mut() {
            match entity.entity_type {
                EntityType::Projectile => {
                    entity.velocity.y -= 9.8 * dt;
                    entity.position += entity.velocity * dt;

                    if entity.position.y < 0.0 {
                        entity.position.y = 0.0;
                        entity.velocity = Vec3::ZERO;
                    }

                    entity.dirty = true;
                }
                EntityType::Player => {}
                _ => {}
            }
        }
    }

    fn broadcast_snapshots(&mut self) {
        let client_data: Vec<(SocketAddr, u32, u32)> = self
            .connections
            .iter()
            .filter(|c| c.state == ConnectionState::Connected)
            .map(|c| (c.addr, c.last_command_ack, c.send_sequence))
            .collect();

        for (addr, last_cmd_ack, send_seq) in client_data {
            let snapshot = self.world.generate_snapshot(last_cmd_ack);

            let header = PacketHeader::new(send_seq, 0, 0);
            let packet = Packet::new(header, PacketType::WorldSnapshot(snapshot));

            if let Err(e) = self.endpoint.send_to(&packet, addr) {
                self.pending_events.push_back(ServerEvent::Error {
                    message: format!("Failed to send snapshot to {}: {}", addr, e),
                });
            }

            if let Some(client) = self.connections.get_by_addr_mut(&addr) {
                client.send_sequence = client.send_sequence.wrapping_add(1);
            }
        }
    }

    fn process_network(&mut self) -> io::Result<()> {
        let packets = self.endpoint.receive()?;

        for (packet, addr) in packets {
            self.handle_packet(packet, addr)?;
        }

        Ok(())
    }

    fn handle_packet(&mut self, packet: Packet, addr: SocketAddr) -> io::Result<()> {
        match packet.payload {
            PacketType::ConnectionRequest { client_salt } => {
                self.handle_connection_request(addr, client_salt)?;
            }
            PacketType::ChallengeResponse { combined_salt } => {
                self.handle_challenge_response(addr, combined_salt)?;
            }
            PacketType::ClientCommand(command) => {
                self.handle_client_command(addr, command)?;
            }
            PacketType::Ping { timestamp } => {
                self.handle_ping(addr, timestamp)?;
            }
            PacketType::Disconnect => {
                self.handle_disconnect(addr)?;
            }
            _ => {}
        }

        if let Some(client) = self.connections.get_by_addr_mut(&addr) {
            client.touch();
        }

        Ok(())
    }

    fn handle_connection_request(&mut self, addr: SocketAddr, client_salt: u64) -> io::Result<()> {
        self.pending_events
            .push_back(ServerEvent::ClientConnecting { addr });

        let client = match self.connections.get_or_create_pending(addr, client_salt) {
            Ok(c) => c,
            Err(reason) => {
                let header = PacketHeader::new(0, 0, 0);
                let packet = Packet::new(
                    header,
                    PacketType::ConnectionDenied {
                        reason: reason.to_string(),
                    },
                );
                self.endpoint.send_to(&packet, addr)?;
                self.pending_events
                    .push_back(ServerEvent::ConnectionDenied {
                        addr,
                        reason: reason.to_string(),
                    });
                return Ok(());
            }
        };

        let server_salt = client.server_salt;
        let challenge = client.combined_salt();

        let header = PacketHeader::new(client.send_sequence, 0, 0);
        client.send_sequence += 1;

        let packet = Packet::new(
            header,
            PacketType::ConnectionChallenge {
                server_salt,
                challenge,
            },
        );

        self.endpoint.send_to(&packet, addr)?;

        Ok(())
    }

    fn handle_challenge_response(
        &mut self,
        addr: SocketAddr,
        combined_salt: u64,
    ) -> io::Result<()> {
        let Some(client) = self.connections.get_by_addr_mut(&addr) else {
            return Ok(());
        };

        if combined_salt != client.combined_salt() {
            self.pending_events.push_back(ServerEvent::Error {
                message: format!("Invalid challenge response from {}", addr),
            });
            return Ok(());
        }

        client.state = ConnectionState::Connected;
        let client_id = client.client_id;

        let entity_id = self.world.spawn_player(Vec3::new(0.0, 1.0, 0.0));
        client.entity_id = Some(entity_id);

        self.pending_events.push_back(ServerEvent::ClientConnected {
            client_id,
            addr,
            entity_id,
        });

        let header = PacketHeader::new(client.send_sequence, 0, 0);
        client.send_sequence += 1;

        let packet = Packet::new(
            header,
            PacketType::ConnectionAccepted {
                client_id,
                entity_id,
            },
        );
        self.endpoint.send_to(&packet, addr)?;

        Ok(())
    }

    fn handle_client_command(
        &mut self,
        addr: SocketAddr,
        command: ClientCommand,
    ) -> io::Result<()> {
        let Some(client) = self.connections.get_by_addr(&addr) else {
            return Ok(());
        };

        if client.state != ConnectionState::Connected {
            return Ok(());
        }

        self.command_queue.push_back(QueuedCommand {
            client_id: client.client_id,
            command,
        });

        Ok(())
    }

    fn handle_ping(&mut self, addr: SocketAddr, timestamp: u64) -> io::Result<()> {
        let header = PacketHeader::new(0, 0, 0);
        let packet = Packet::new(header, PacketType::Pong { timestamp });
        self.endpoint.send_to(&packet, addr)?;
        Ok(())
    }

    fn handle_disconnect(&mut self, addr: SocketAddr) -> io::Result<()> {
        if let Some(client) = self.connections.remove_by_addr(&addr) {
            if let Some(entity_id) = client.entity_id {
                self.world.despawn_entity(entity_id);
            }
            self.pending_events
                .push_back(ServerEvent::ClientDisconnected {
                    client_id: client.client_id,
                    reason: DisconnectReason::Graceful,
                });
        }
        Ok(())
    }

    pub fn stats(&self) -> ServerStats {
        ServerStats {
            tick: self.tick,
            client_count: self.connections.connected_count(),
            max_clients: self.config.max_clients,
            entity_count: self.world.entity_count(),
            network_stats: self.endpoint.stats().clone(),
        }
    }

    pub fn client_infos(&self) -> Vec<crate::tui::ClientInfo> {
        self.connections
            .iter()
            .filter(|c| c.state == ConnectionState::Connected)
            .map(|c| crate::tui::ClientInfo {
                client_id: c.client_id,
                addr: c.addr.to_string(),
                entity_id: c.entity_id,
                connected_secs: c.last_receive_time.elapsed().as_secs(),
                last_ping_ms: self.endpoint.stats().rtt_ms,
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct ServerStats {
    pub tick: u32,
    pub client_count: usize,
    pub max_clients: usize,
    pub entity_count: usize,
    pub network_stats: NetworkStats,
}
