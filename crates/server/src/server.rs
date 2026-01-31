use std::collections::{BinaryHeap, VecDeque};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use glam::Vec3;

use dual::{
    ClientCommand, ConnectionManager, ConnectionState, NetworkEndpoint, NetworkStats, Packet,
    PacketHeader, PacketLossSimulation, PacketType, Reliability, SnapshotBuffer, World,
    WorldSnapshot,
};

use crate::config::ServerConfig;
use crate::events::{DisconnectReason, ServerEvent};
use crate::simulation::{apply_command, simulate_world};

#[derive(Debug)]
struct QueuedCommand {
    client_id: u32,
    command: ClientCommand,
}

#[derive(Debug)]
struct DelayedPacket {
    send_time: Instant,
    packet: Packet,
    addr: SocketAddr,
}

impl PartialEq for DelayedPacket {
    fn eq(&self, other: &Self) -> bool {
        self.send_time == other.send_time
    }
}

impl Eq for DelayedPacket {}

impl PartialOrd for DelayedPacket {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DelayedPacket {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.send_time.cmp(&self.send_time)
    }
}

pub struct GameServer {
    endpoint: NetworkEndpoint,
    connections: ConnectionManager,
    config: ServerConfig,
    world: World,
    snapshot_history: SnapshotBuffer,
    command_queue: VecDeque<QueuedCommand>,
    delayed_packets: BinaryHeap<DelayedPacket>,
    tick: u32,
    tick_duration: Duration,
    last_tick_time: Instant,
    accumulator: Duration,
    running: Arc<AtomicBool>,
    #[allow(dead_code)]
    start_time: Instant,
    pending_events: VecDeque<ServerEvent>,
}

impl GameServer {
    pub fn new(bind_addr: &str, config: ServerConfig) -> io::Result<Self> {
        let mut endpoint = NetworkEndpoint::bind(bind_addr)?;
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
            delayed_packets: BinaryHeap::new(),
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
        if let Some(client) = self.connections.get_mut(client_id) {
            let addr = client.addr;
            let packet = client.send_packet(PacketType::Disconnect, Reliability::Reliable);
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

        self.process_resends();
        self.process_delayed_packets();

        while self.accumulator >= self.tick_duration {
            self.accumulator -= self.tick_duration;
            self.tick();
        }
    }

    fn process_resends(&mut self) {
        let mut packets_to_send = Vec::new();
        for client in self.connections.iter_mut() {
            let resends = client.collect_resends();
            for packet in resends {
                packets_to_send.push((client.addr, packet));
            }
        }

        for (addr, packet) in packets_to_send {
            let _ = self.send_packet_simulated(packet, addr);
        }
    }

    fn process_delayed_packets(&mut self) {
        let now = Instant::now();
        while let Some(packet) = self.delayed_packets.peek() {
            if packet.send_time <= now {
                let DelayedPacket { packet, addr, .. } = self.delayed_packets.pop().unwrap();
                if let Err(e) = self.endpoint.send_to(&packet, addr) {
                    self.pending_events.push_back(ServerEvent::Error {
                        message: format!("Failed to send delayed packet to {}: {}", addr, e),
                    });
                }
            } else {
                break;
            }
        }
    }

    fn send_packet_simulated(&mut self, packet: Packet, addr: SocketAddr) -> io::Result<()> {
        let mut delay = 0;
        let mut should_drop = false;

        if let Some(client) = self.connections.get_by_addr(&addr) {
            should_drop = client.packet_loss_sim.should_drop();
            delay = client.packet_loss_sim.delay_ms();
        } else if let Some(ref sim) = self.config.global_packet_loss {
            should_drop = sim.should_drop();
            delay = sim.delay_ms();
        }

        if should_drop {
            return Ok(());
        }

        if delay == 0 {
            self.endpoint.send_to(&packet, addr).map(|_| ())
        } else {
            self.delayed_packets.push(DelayedPacket {
                send_time: Instant::now() + Duration::from_millis(delay as u64),
                packet,
                addr,
            });
            Ok(())
        }
    }

    fn tick(&mut self) {
        self.process_commands();

        let dt = 1.0 / self.config.tick_rate as f32;
        simulate_world(&mut self.world, dt);

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
        let dt = 1.0 / self.config.tick_rate as f32;

        while let Some(queued) = self.command_queue.pop_front() {
            if let Some(client) = self.connections.get_mut(queued.client_id) {
                if queued.command.command_sequence > client.last_command_ack {
                    client.last_command_ack = queued.command.command_sequence;
                }

                if let Some(entity_id) = client.entity_id {
                    if let Some(entity) = self.world.get_entity_mut(entity_id) {
                        apply_command(entity, &queued.command, dt);
                    }
                }
            }
        }
    }

    fn broadcast_snapshots(&mut self) {
        let client_data: Vec<(SocketAddr, u32, u32, u32, bool, u32)> = self
            .connections
            .iter()
            .filter(|c| c.state == ConnectionState::Connected)
            .map(|c| {
                (
                    c.addr,
                    c.last_command_ack,
                    c.last_acked_tick,
                    c.send_sequence,
                    c.packet_loss_sim.should_drop(),
                    c.packet_loss_sim.delay_ms(),
                )
            })
            .collect();

        let current_tick = self.tick;
        let max_delta_age = self.config.snapshot_buffer_size as u32 / 2;

        for (addr, last_cmd_ack, last_acked_tick, _, _, _) in client_data {
            let snapshot = self.generate_client_snapshot(
                last_cmd_ack,
                last_acked_tick,
                current_tick,
                max_delta_age,
            );

            if let Some(client) = self.connections.get_by_addr_mut(&addr) {
                let packet = client
                    .send_packet(PacketType::WorldSnapshot(snapshot), Reliability::Unreliable);
                let _ = self.send_packet_simulated(packet, addr);
            }
        }
    }

    fn generate_client_snapshot(
        &self,
        last_cmd_ack: u32,
        last_acked_tick: u32,
        current_tick: u32,
        max_delta_age: u32,
    ) -> WorldSnapshot {
        let baseline_age = current_tick.saturating_sub(last_acked_tick);

        if last_acked_tick > 0 && baseline_age < max_delta_age {
            if let Some(baseline) = self.snapshot_history.get_by_tick(last_acked_tick) {
                return self
                    .world
                    .generate_delta_from_baseline(baseline, last_cmd_ack);
            }
        }

        self.world.generate_snapshot(last_cmd_ack)
    }

    fn process_network(&mut self) -> io::Result<()> {
        let packets = self.endpoint.receive()?;

        for (packet, addr) in packets {
            if let Some(client) = self.connections.get_by_addr_mut(&addr) {
                let payloads = client.process_packet(packet);
                for payload in payloads {
                    self.handle_payload(payload, addr)?;
                }
            } else {
                // No connection, check if it's a request
                if let PacketType::ConnectionRequest { .. } = packet.payload {
                    self.handle_payload(packet.payload, addr)?;
                }
            }
        }

        Ok(())
    }

    fn handle_payload(&mut self, payload: PacketType, addr: SocketAddr) -> io::Result<()> {
        match payload {
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
            PacketType::SnapshotAck { received_tick } => {
                self.handle_snapshot_ack(addr, received_tick)?;
            }
            PacketType::Disconnect => {
                self.handle_disconnect(addr)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_connection_request(&mut self, addr: SocketAddr, client_salt: u64) -> io::Result<()> {
        self.pending_events
            .push_back(ServerEvent::ClientConnecting { addr });

        let global_packet_loss = self.config.global_packet_loss.clone();

        let client = match self.connections.get_or_create_pending(addr, client_salt) {
            Ok(c) => c,
            Err(reason) => {
                let header = PacketHeader::new(0, 0, 0, PacketHeader::CHANNEL_UNRELIABLE, 0);
                let packet = Packet::new(
                    header,
                    PacketType::ConnectionDenied {
                        reason: reason.to_string(),
                    },
                );
                self.send_packet_simulated(packet, addr)?;
                self.pending_events
                    .push_back(ServerEvent::ConnectionDenied {
                        addr,
                        reason: reason.to_string(),
                    });
                return Ok(());
            }
        };

        if let Some(sim) = global_packet_loss {
            client.packet_loss_sim = sim;
        }

        let server_salt = client.server_salt;
        let challenge = client.combined_salt();

        // Use reliable for challenge to ensure it arrives
        let packet = client.send_packet(
            PacketType::ConnectionChallenge {
                server_salt,
                challenge,
            },
            Reliability::Reliable,
        );

        self.send_packet_simulated(packet, addr)?;

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

        // Use reliable for connection accepted
        let packet = client.send_packet(
            PacketType::ConnectionAccepted {
                client_id,
                entity_id,
            },
            Reliability::Reliable,
        );

        self.send_packet_simulated(packet, addr)?;

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
        if let Some(client) = self.connections.get_by_addr_mut(&addr) {
            let packet =
                client.send_packet(PacketType::Pong { timestamp }, Reliability::Unreliable);
            self.send_packet_simulated(packet, addr)?;
        }
        Ok(())
    }

    fn handle_snapshot_ack(&mut self, addr: SocketAddr, received_tick: u32) -> io::Result<()> {
        if let Some(client) = self.connections.get_by_addr_mut(&addr) {
            if received_tick > client.last_acked_tick {
                client.last_acked_tick = received_tick;
            }
        }
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
                packet_loss_sim: c.packet_loss_sim.clone(),
            })
            .collect()
    }

    pub fn set_packet_loss_sim(&mut self, client_id: u32, sim: PacketLossSimulation) {
        if let Some(client) = self.connections.get_mut(client_id) {
            client.packet_loss_sim = sim;
        }
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
