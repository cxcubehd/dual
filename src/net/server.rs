use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use glam::Vec3;

use super::protocol::{ClientCommand, Packet, PacketHeader, PacketType};
use super::snapshot::{EntityType, SnapshotBuffer, World};
use super::transport::{ConnectionManager, ConnectionState, NetworkEndpoint};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub tick_rate: u32,
    pub max_clients: usize,
    pub snapshot_buffer_size: usize,
    pub connection_timeout_secs: u64,
    pub snapshot_send_rate: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            tick_rate: 5,
            max_clients: 32,
            snapshot_buffer_size: 64,
            connection_timeout_secs: 10,
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
    start_time: Instant,
}

impl GameServer {
    pub fn new(bind_addr: &str, config: ServerConfig) -> io::Result<Self> {
        let endpoint = NetworkEndpoint::bind(bind_addr)?;
        let tick_duration = Duration::from_secs_f64(1.0 / config.tick_rate as f64);

        log::info!(
            "Server started on {} with tick rate {}",
            endpoint.local_addr(),
            config.tick_rate
        );

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
            config,
        })
    }

    pub fn running(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.running)
    }

    pub fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn run(&mut self) {
        while self.running.load(Ordering::SeqCst) {
            let now = Instant::now();
            let delta = now - self.last_tick_time;
            self.last_tick_time = now;
            self.accumulator += delta;

            if let Err(e) = self.process_network() {
                log::error!("Network error: {}", e);
            }

            while self.accumulator >= self.tick_duration {
                self.accumulator -= self.tick_duration;
                self.tick();
            }

            let elapsed = now.elapsed();
            if elapsed < self.tick_duration / 2 {
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        log::info!("Server shutting down");
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
            log::info!("Client {} timed out", client_id);
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
                log::warn!("Failed to send snapshot to {}: {}", addr, e);
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
            _ => {
                log::debug!("Unexpected packet type from {}", addr);
            }
        }

        if let Some(client) = self.connections.get_by_addr_mut(&addr) {
            client.touch();
        }

        Ok(())
    }

    fn handle_connection_request(&mut self, addr: SocketAddr, client_salt: u64) -> io::Result<()> {
        log::info!("Connection request from {}", addr);

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
            log::warn!("Invalid challenge response from {}", addr);
            return Ok(());
        }

        client.state = ConnectionState::Connected;
        let client_id = client.client_id;

        let entity_id = self.world.spawn_player(Vec3::new(0.0, 1.0, 0.0));
        client.entity_id = Some(entity_id);

        log::info!(
            "Client {} connected from {}, entity {}",
            client_id,
            addr,
            entity_id
        );

        let header = PacketHeader::new(client.send_sequence, 0, 0);
        client.send_sequence += 1;

        let packet = Packet::new(header, PacketType::ConnectionAccepted { client_id });
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
            log::info!("Client {} disconnected", client.client_id);

            if let Some(entity_id) = client.entity_id {
                self.world.despawn_entity(entity_id);
            }
        }
        Ok(())
    }

    pub fn stats(&self) -> ServerStats {
        ServerStats {
            tick: self.tick,
            client_count: self.connections.client_count(),
            entity_count: self.world.entities().count(),
            uptime_secs: self.start_time.elapsed().as_secs(),
            network_stats: self.endpoint.stats().clone(),
        }
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }
}

#[derive(Debug, Clone)]
pub struct ServerStats {
    pub tick: u32,
    pub client_count: usize,
    pub entity_count: usize,
    pub uptime_secs: u64,
    pub network_stats: super::transport::NetworkStats,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_creation() {
        let config = ServerConfig::default();
        let server = GameServer::new("127.0.0.1:0", config);
        assert!(server.is_ok());
    }

    #[test]
    fn test_command_processing() {
        let config = ServerConfig::default();
        let mut server = GameServer::new("127.0.0.1:0", config).unwrap();

        let entity_id = server.world.spawn_player(Vec3::ZERO);

        let mut command = ClientCommand::new(0, 1);
        command.encode_move_direction([1.0, 0.0, 0.0]);
        command.encode_view_angles(0.0, 0.0);

        server.apply_command(entity_id, &command);

        let entity = server.world.get_entity(entity_id).unwrap();
        assert!(entity.position.x > 0.0);
    }
}
