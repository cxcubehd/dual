use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::protocol::{sequence_greater_than, Packet, PacketHeader, PacketType, MAX_PACKET_SIZE};

#[derive(Debug, Clone, Default)]
pub struct PacketLossSimulation {
    pub enabled: bool,
    pub loss_percent: f32,
    pub min_latency_ms: u32,
    pub max_latency_ms: u32,
    pub jitter_ms: u32,
}

impl PacketLossSimulation {
    pub fn should_drop(&self) -> bool {
        if !self.enabled || self.loss_percent <= 0.0 {
            return false;
        }
        rand_percent() < self.loss_percent
    }

    pub fn delay_ms(&self) -> u32 {
        if !self.enabled || self.max_latency_ms == 0 {
            return 0;
        }
        let base = self.min_latency_ms;
        let range = self.max_latency_ms.saturating_sub(self.min_latency_ms);
        let jitter = if self.jitter_ms > 0 {
            (rand_percent() * self.jitter_ms as f32) as u32
        } else {
            0
        };
        base + (rand_percent() * range as f32) as u32 + jitter
    }
}

fn rand_percent() -> f32 {
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
    (hasher.finish() % 10000) as f32 / 10000.0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    ChallengeResponse,
    Connected,
    Disconnecting,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    pub packets_sent: u64,
    pub packets_received: u64,
    pub packets_lost: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub rtt_ms: f32,
    pub rtt_variance: f32,
    pub packet_loss_percent: f32,
}

#[derive(Debug, Clone)]
pub struct PendingPacket {
    pub sequence: u32,
    pub send_time: Instant,
    pub acked: bool,
}

#[derive(Debug)]
pub struct AckTracker {
    pending: VecDeque<PendingPacket>,
    max_pending: usize,
    srtt: f32,
    rtt_var: f32,
}

impl AckTracker {
    pub fn new(max_pending: usize) -> Self {
        Self {
            pending: VecDeque::with_capacity(max_pending),
            max_pending,
            srtt: 100.0,
            rtt_var: 50.0,
        }
    }

    pub fn track_packet(&mut self, sequence: u32) {
        while self.pending.len() >= self.max_pending {
            self.pending.pop_front();
        }

        self.pending.push_back(PendingPacket {
            sequence,
            send_time: Instant::now(),
            acked: false,
        });
    }

    pub fn process_ack(&mut self, ack: u32, ack_bitfield: u32) -> Vec<u32> {
        let mut acked_sequences = Vec::new();
        let mut rtt_samples = Vec::new();
        let now = Instant::now();

        for pending in &mut self.pending {
            if pending.acked {
                continue;
            }

            let is_acked = if pending.sequence == ack {
                true
            } else if sequence_greater_than(ack, pending.sequence) {
                let diff = ack.wrapping_sub(pending.sequence);
                if diff <= 32 {
                    (ack_bitfield & (1 << (diff - 1))) != 0
                } else {
                    false
                }
            } else {
                false
            };

            if is_acked {
                pending.acked = true;
                acked_sequences.push(pending.sequence);

                let rtt = now.duration_since(pending.send_time).as_secs_f32() * 1000.0;
                rtt_samples.push(rtt);
            }
        }

        for rtt in rtt_samples {
            self.update_rtt(rtt);
        }

        while self.pending.front().is_some_and(|p| p.acked) {
            self.pending.pop_front();
        }

        acked_sequences
    }

    fn update_rtt(&mut self, rtt: f32) {
        const ALPHA: f32 = 0.125;
        const BETA: f32 = 0.25;

        let diff = (rtt - self.srtt).abs();
        self.rtt_var = (1.0 - BETA) * self.rtt_var + BETA * diff;
        self.srtt = (1.0 - ALPHA) * self.srtt + ALPHA * rtt;
    }

    pub fn srtt(&self) -> f32 {
        self.srtt
    }

    pub fn rtt_var(&self) -> f32 {
        self.rtt_var
    }

    pub fn unacked_count(&self) -> usize {
        self.pending.iter().filter(|p| !p.acked).count()
    }
}

#[derive(Debug)]
pub struct ReceiveTracker {
    last_received: u32,
    received_bitfield: u32,
    recent_sequences: VecDeque<u32>,
    max_recent: usize,
}

impl Default for ReceiveTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ReceiveTracker {
    pub fn new() -> Self {
        Self {
            last_received: 0,
            received_bitfield: 0,
            recent_sequences: VecDeque::with_capacity(128),
            max_recent: 128,
        }
    }

    pub fn record_received(&mut self, sequence: u32) -> bool {
        if self.recent_sequences.contains(&sequence) {
            return false;
        }

        if self.recent_sequences.len() >= self.max_recent {
            self.recent_sequences.pop_front();
        }
        self.recent_sequences.push_back(sequence);

        if sequence_greater_than(sequence, self.last_received) {
            let diff = sequence.wrapping_sub(self.last_received);
            if diff <= 32 {
                self.received_bitfield = (self.received_bitfield << diff) | 1;
            } else {
                self.received_bitfield = 0;
            }
            self.last_received = sequence;
        } else {
            let diff = self.last_received.wrapping_sub(sequence);
            if diff > 0 && diff <= 32 {
                self.received_bitfield |= 1 << (diff - 1);
            }
        }

        true
    }

    pub fn ack_data(&self) -> (u32, u32) {
        (self.last_received, self.received_bitfield)
    }
}

#[derive(Debug)]
pub struct ClientConnection {
    pub addr: std::net::SocketAddr,
    pub client_id: u32,
    pub state: ConnectionState,
    pub client_salt: u64,
    pub server_salt: u64,
    pub last_command_ack: u32,
    pub last_receive_time: Instant,
    pub entity_id: Option<u32>,
    pub receive_tracker: ReceiveTracker,
    pub send_sequence: u32,
    pub lobby_id: Option<u64>,
    pub packet_loss_sim: PacketLossSimulation,
}

impl ClientConnection {
    pub fn new(addr: SocketAddr, client_id: u32, client_salt: u64) -> Self {
        Self {
            addr,
            client_id,
            state: ConnectionState::Connecting,
            client_salt,
            server_salt: rand_u64(),
            last_command_ack: 0,
            last_receive_time: Instant::now(),
            entity_id: None,
            receive_tracker: ReceiveTracker::new(),
            send_sequence: 0,
            lobby_id: None,
            packet_loss_sim: PacketLossSimulation::default(),
        }
    }

    pub fn combined_salt(&self) -> u64 {
        self.client_salt ^ self.server_salt
    }

    pub fn is_timed_out(&self, timeout: Duration) -> bool {
        self.last_receive_time.elapsed() > timeout
    }

    pub fn touch(&mut self) {
        self.last_receive_time = Instant::now();
    }
}

#[derive(Debug)]
pub struct ConnectionManager {
    clients_by_addr: HashMap<std::net::SocketAddr, u32>,
    clients: HashMap<u32, ClientConnection>,
    next_client_id: u32,
    max_clients: usize,
    timeout: Duration,
}

impl ConnectionManager {
    pub fn new(max_clients: usize) -> Self {
        Self {
            clients_by_addr: HashMap::new(),
            clients: HashMap::new(),
            next_client_id: 1,
            max_clients,
            timeout: Duration::from_secs(10),
        }
    }

    pub fn get_or_create_pending(
        &mut self,
        addr: SocketAddr,
        client_salt: u64,
    ) -> Result<&mut ClientConnection, &'static str> {
        if let Some(&client_id) = self.clients_by_addr.get(&addr) {
            return Ok(self.clients.get_mut(&client_id).unwrap());
        }

        if self.clients.len() >= self.max_clients {
            return Err("Server full");
        }

        let client_id = self.next_client_id;
        self.next_client_id += 1;

        let connection = ClientConnection::new(addr, client_id, client_salt);
        self.clients.insert(client_id, connection);
        self.clients_by_addr.insert(addr, client_id);

        Ok(self.clients.get_mut(&client_id).unwrap())
    }

    pub fn get_by_addr(&self, addr: &std::net::SocketAddr) -> Option<&ClientConnection> {
        self.clients_by_addr
            .get(addr)
            .and_then(|id| self.clients.get(id))
    }

    pub fn get_by_addr_mut(
        &mut self,
        addr: &std::net::SocketAddr,
    ) -> Option<&mut ClientConnection> {
        if let Some(&id) = self.clients_by_addr.get(addr) {
            self.clients.get_mut(&id)
        } else {
            None
        }
    }

    pub fn get(&self, client_id: u32) -> Option<&ClientConnection> {
        self.clients.get(&client_id)
    }

    pub fn get_mut(&mut self, client_id: u32) -> Option<&mut ClientConnection> {
        self.clients.get_mut(&client_id)
    }

    pub fn remove(&mut self, client_id: u32) -> Option<ClientConnection> {
        if let Some(conn) = self.clients.remove(&client_id) {
            self.clients_by_addr.remove(&conn.addr);
            Some(conn)
        } else {
            None
        }
    }

    pub fn remove_by_addr(&mut self, addr: &std::net::SocketAddr) -> Option<ClientConnection> {
        if let Some(client_id) = self.clients_by_addr.remove(addr) {
            self.clients.remove(&client_id)
        } else {
            None
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &ClientConnection> {
        self.clients.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut ClientConnection> {
        self.clients.values_mut()
    }

    pub fn cleanup_timed_out(&mut self) -> Vec<u32> {
        let timed_out: Vec<u32> = self
            .clients
            .iter()
            .filter(|(_, c)| c.is_timed_out(self.timeout))
            .map(|(&id, _)| id)
            .collect();

        for id in &timed_out {
            self.remove(*id);
        }

        timed_out
    }

    pub fn connected_count(&self) -> usize {
        self.clients
            .values()
            .filter(|c| c.state == ConnectionState::Connected)
            .count()
    }

    pub fn total_count(&self) -> usize {
        self.clients.len()
    }
}

fn rand_u64() -> u64 {
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

pub struct NetworkEndpoint {
    socket: UdpSocket,
    local_addr: SocketAddr,
    remote_addr: Option<SocketAddr>,
    state: ConnectionState,
    send_sequence: u32,
    ack_tracker: AckTracker,
    receive_tracker: ReceiveTracker,
    stats: NetworkStats,
    recv_buffer: [u8; MAX_PACKET_SIZE],
    timeout: Duration,
    last_receive_time: Instant,
    running: Arc<AtomicBool>,
    /// When true, skip global duplicate filtering (for servers where per-client tracking is done)
    server_mode: bool,
}

impl NetworkEndpoint {
    pub fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_nonblocking(true)?;

        let local_addr = socket.local_addr()?;

        Ok(Self {
            socket,
            local_addr,
            remote_addr: None,
            state: ConnectionState::Disconnected,
            send_sequence: 0,
            ack_tracker: AckTracker::new(256),
            receive_tracker: ReceiveTracker::new(),
            stats: NetworkStats::default(),
            recv_buffer: [0u8; MAX_PACKET_SIZE],
            timeout: Duration::from_secs(10),
            last_receive_time: Instant::now(),
            running: Arc::new(AtomicBool::new(true)),
            server_mode: false,
        })
    }

    /// Enable server mode - disables global duplicate filtering
    /// (server should do per-client tracking via ConnectionManager)
    pub fn set_server_mode(&mut self, server_mode: bool) {
        self.server_mode = server_mode;
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    pub fn remote_addr(&self) -> Option<SocketAddr> {
        self.remote_addr
    }

    pub fn state(&self) -> ConnectionState {
        self.state
    }

    pub fn set_state(&mut self, state: ConnectionState) {
        self.state = state;
    }

    pub fn set_remote(&mut self, addr: SocketAddr) {
        self.remote_addr = Some(addr);
    }

    pub fn stats(&self) -> &NetworkStats {
        &self.stats
    }

    pub fn send_to(&mut self, packet: &Packet, addr: SocketAddr) -> io::Result<usize> {
        let data = packet.serialize().map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Serialization error: {}", e),
            )
        })?;

        if data.len() > MAX_PACKET_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Packet exceeds MTU",
            ));
        }

        let bytes = self.socket.send_to(&data, addr)?;

        self.ack_tracker.track_packet(packet.header.sequence);

        self.stats.packets_sent += 1;
        self.stats.bytes_sent += bytes as u64;

        Ok(bytes)
    }

    pub fn send(&mut self, packet: &Packet) -> io::Result<usize> {
        let addr = self
            .remote_addr
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "No remote address set"))?;
        self.send_to(packet, addr)
    }

    pub fn create_packet(&mut self, payload: PacketType) -> Packet {
        let sequence = self.send_sequence;
        self.send_sequence = self.send_sequence.wrapping_add(1);

        let (ack, ack_bitfield) = self.receive_tracker.ack_data();
        let header = PacketHeader::new(sequence, ack, ack_bitfield);

        Packet::new(header, payload)
    }

    pub fn receive(&mut self) -> io::Result<Vec<(Packet, SocketAddr)>> {
        let mut packets = Vec::new();

        loop {
            match self.socket.recv_from(&mut self.recv_buffer) {
                Ok((size, addr)) => {
                    if size < 8 {
                        continue;
                    }

                    match Packet::deserialize(&self.recv_buffer[..size]) {
                        Ok(packet) => {
                            if !packet.header.is_valid() {
                                continue;
                            }

                            // In server mode, skip global duplicate filtering
                            // (per-client tracking is done via ConnectionManager)
                            if !self.server_mode
                                && !self.receive_tracker.record_received(packet.header.sequence)
                            {
                                continue;
                            }

                            let _acked = self
                                .ack_tracker
                                .process_ack(packet.header.ack, packet.header.ack_bitfield);

                            self.stats.packets_received += 1;
                            self.stats.bytes_received += size as u64;
                            self.stats.rtt_ms = self.ack_tracker.srtt();
                            self.stats.rtt_variance = self.ack_tracker.rtt_var();

                            if self.stats.packets_sent > 0 {
                                let unacked = self.ack_tracker.unacked_count() as f32;
                                let sent = self.stats.packets_sent as f32;
                                self.stats.packet_loss_percent = (unacked / sent.max(1.0)) * 100.0;
                            }

                            self.last_receive_time = Instant::now();
                            packets.push((packet, addr));
                        }
                        Err(_) => continue,
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }

        Ok(packets)
    }

    pub fn is_timed_out(&self) -> bool {
        self.last_receive_time.elapsed() > self.timeout
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub fn reset(&mut self) {
        self.state = ConnectionState::Disconnected;
        self.send_sequence = 0;
        self.ack_tracker = AckTracker::new(256);
        self.receive_tracker = ReceiveTracker::new();
        self.stats = NetworkStats::default();
        self.last_receive_time = Instant::now();
    }

    pub fn running(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.running)
    }

    pub fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_receive_tracker_bitfield() {
        let mut tracker = ReceiveTracker::new();

        tracker.record_received(1);
        tracker.record_received(2);
        tracker.record_received(3);

        let (ack, bitfield) = tracker.ack_data();
        assert_eq!(ack, 3);
        assert_eq!(bitfield & 0b11, 0b11);
    }

    #[test]
    fn test_receive_tracker_out_of_order() {
        let mut tracker = ReceiveTracker::new();

        tracker.record_received(3);
        tracker.record_received(1);
        tracker.record_received(2);

        let (ack, bitfield) = tracker.ack_data();
        assert_eq!(ack, 3);
        assert_eq!(bitfield & 0b11, 0b11);
    }

    #[test]
    fn test_duplicate_detection() {
        let mut tracker = ReceiveTracker::new();

        assert!(tracker.record_received(1));
        assert!(!tracker.record_received(1));
        assert!(tracker.record_received(2));
    }

    #[test]
    fn test_ack_tracker_rtt() {
        let mut tracker = AckTracker::new(32);

        tracker.track_packet(1);
        std::thread::sleep(Duration::from_millis(10));

        tracker.process_ack(1, 0);

        assert!(tracker.srtt() > 0.0);
    }
}
