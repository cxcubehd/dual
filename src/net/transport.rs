//! Network Transport Layer
//!
//! Provides UDP socket abstraction, connection management, and reliable
//! packet sequencing with acknowledgment tracking.
//!
//! # Acknowledgment System
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                 Sliding Window ACK System                    │
//! ├─────────────────────────────────────────────────────────────┤
//! │                                                              │
//! │  Received packets: [✓][✓][✗][✓][✓][✓][✗][✓] ...             │
//! │                     │  │     │  │  │     │                   │
//! │                     └──┴─────┴──┴──┴─────┴──► ack_bitfield  │
//! │                                                              │
//! │  ack = 42 (latest received)                                  │
//! │  ack_bitfield = 0b11011101 (packets 41,40,38,37,36,34)      │
//! │                                                              │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::protocol::{
    MAX_PACKET_SIZE, PROTOCOL_MAGIC, Packet, PacketHeader, PacketType, sequence_greater_than,
};

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Not connected
    Disconnected,
    /// Sent connection request, awaiting challenge
    Connecting,
    /// Received challenge, sent response
    ChallengeResponse,
    /// Fully connected
    Connected,
    /// Disconnecting gracefully
    Disconnecting,
}

/// Packet delivery statistics
#[derive(Debug, Clone, Default)]
pub struct NetworkStats {
    /// Packets sent
    pub packets_sent: u64,
    /// Packets received
    pub packets_received: u64,
    /// Packets lost (estimated from ACKs)
    pub packets_lost: u64,
    /// Bytes sent
    pub bytes_sent: u64,
    /// Bytes received
    pub bytes_received: u64,
    /// Current round-trip time estimate (ms)
    pub rtt_ms: f32,
    /// RTT variance for jitter estimation
    pub rtt_variance: f32,
    /// Packet loss percentage (0-100)
    pub packet_loss_percent: f32,
}

/// Pending packet awaiting acknowledgment
#[derive(Debug, Clone)]
struct PendingPacket {
    /// Sequence number
    sequence: u32,
    /// When the packet was sent
    send_time: Instant,
    /// Packet data for potential retransmission
    data: Vec<u8>,
    /// Whether this packet has been acknowledged
    acked: bool,
}

/// Tracks acknowledgments and calculates RTT
#[derive(Debug)]
struct AckTracker {
    /// Packets awaiting acknowledgment
    pending: VecDeque<PendingPacket>,
    /// Maximum pending packets to track
    max_pending: usize,
    /// Smoothed RTT (exponential moving average)
    srtt: f32,
    /// RTT variance
    rtt_var: f32,
}

impl AckTracker {
    fn new(max_pending: usize) -> Self {
        Self {
            pending: VecDeque::with_capacity(max_pending),
            max_pending,
            srtt: 100.0, // Initial estimate: 100ms
            rtt_var: 50.0,
        }
    }

    fn track_packet(&mut self, sequence: u32, data: Vec<u8>) {
        // Remove old packets if at capacity
        while self.pending.len() >= self.max_pending {
            self.pending.pop_front();
        }

        self.pending.push_back(PendingPacket {
            sequence,
            send_time: Instant::now(),
            data,
            acked: false,
        });
    }

    fn process_ack(&mut self, ack: u32, ack_bitfield: u32) -> Vec<u32> {
        let mut acked_sequences = Vec::new();
        let mut rtt_samples = Vec::new();
        let now = Instant::now();

        for pending in &mut self.pending {
            if pending.acked {
                continue;
            }

            // Check if this sequence is acknowledged
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

                // Collect RTT sample for later update
                let rtt = now.duration_since(pending.send_time).as_secs_f32() * 1000.0;
                rtt_samples.push(rtt);
            }
        }

        // Update RTT estimates after the borrow ends
        for rtt in rtt_samples {
            self.update_rtt(rtt);
        }

        // Clean up old acknowledged packets
        while self.pending.front().map(|p| p.acked).unwrap_or(false) {
            self.pending.pop_front();
        }

        acked_sequences
    }

    fn update_rtt(&mut self, rtt: f32) {
        // RFC 6298 RTT estimation
        const ALPHA: f32 = 0.125;
        const BETA: f32 = 0.25;

        let diff = (rtt - self.srtt).abs();
        self.rtt_var = (1.0 - BETA) * self.rtt_var + BETA * diff;
        self.srtt = (1.0 - ALPHA) * self.srtt + ALPHA * rtt;
    }

    fn get_unacked_count(&self) -> usize {
        self.pending.iter().filter(|p| !p.acked).count()
    }
}

/// Tracks received sequences for generating acknowledgment bitfields
#[derive(Debug)]
pub struct ReceiveTracker {
    /// Last received sequence number
    last_received: u32,
    /// Bitfield of received packets before last_received
    received_bitfield: u32,
    /// Set of recently received sequences (for duplicate detection)
    recent_sequences: VecDeque<u32>,
    /// Max recent sequences to track
    max_recent: usize,
}

impl ReceiveTracker {
    fn new() -> Self {
        Self {
            last_received: 0,
            received_bitfield: 0,
            recent_sequences: VecDeque::with_capacity(128),
            max_recent: 128,
        }
    }

    fn record_received(&mut self, sequence: u32) -> bool {
        // Check for duplicate
        if self.recent_sequences.contains(&sequence) {
            return false; // Duplicate
        }

        // Add to recent
        if self.recent_sequences.len() >= self.max_recent {
            self.recent_sequences.pop_front();
        }
        self.recent_sequences.push_back(sequence);

        if sequence_greater_than(sequence, self.last_received) {
            // New most recent packet
            let diff = sequence.wrapping_sub(self.last_received);
            if diff <= 32 {
                // Shift bitfield and add old last_received
                self.received_bitfield = (self.received_bitfield << diff) | 1;
            } else {
                // Too big a gap, reset bitfield
                self.received_bitfield = 0;
            }
            self.last_received = sequence;
        } else {
            // Older packet, set bit in bitfield
            let diff = self.last_received.wrapping_sub(sequence);
            if diff <= 32 {
                self.received_bitfield |= 1 << (diff - 1);
            }
        }

        true // New packet
    }

    fn get_ack_data(&self) -> (u32, u32) {
        (self.last_received, self.received_bitfield)
    }
}

/// Network endpoint for UDP communication with reliability layer
pub struct NetworkEndpoint {
    /// UDP socket
    socket: UdpSocket,
    /// Local address
    local_addr: SocketAddr,
    /// Remote peer address (for client mode)
    remote_addr: Option<SocketAddr>,
    /// Connection state
    state: ConnectionState,
    /// Next outgoing sequence number
    send_sequence: u32,
    /// ACK tracking for sent packets
    ack_tracker: AckTracker,
    /// Receive tracking for incoming packets
    receive_tracker: ReceiveTracker,
    /// Network statistics
    stats: NetworkStats,
    /// Receive buffer
    recv_buffer: [u8; MAX_PACKET_SIZE],
    /// Connection timeout duration
    timeout: Duration,
    /// Last packet receive time
    last_receive_time: Instant,
    /// Running flag for async operations
    running: Arc<AtomicBool>,
}

impl NetworkEndpoint {
    /// Create a new network endpoint bound to the specified address
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
        })
    }

    /// Get local address
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Get remote address (if connected)
    pub fn remote_addr(&self) -> Option<SocketAddr> {
        self.remote_addr
    }

    /// Get connection state
    pub fn state(&self) -> ConnectionState {
        self.state
    }

    /// Set connection state
    pub fn set_state(&mut self, state: ConnectionState) {
        self.state = state;
    }

    /// Set remote address (for client connecting to server)
    pub fn set_remote(&mut self, addr: SocketAddr) {
        self.remote_addr = Some(addr);
    }

    /// Get current network statistics
    pub fn stats(&self) -> &NetworkStats {
        &self.stats
    }

    /// Send a packet to the specified address
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

        // Track for acknowledgment
        self.ack_tracker.track_packet(packet.header.sequence, data);

        // Update stats
        self.stats.packets_sent += 1;
        self.stats.bytes_sent += bytes as u64;

        Ok(bytes)
    }

    /// Send a packet to the connected remote (client mode)
    pub fn send(&mut self, packet: &Packet) -> io::Result<usize> {
        let addr = self
            .remote_addr
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "No remote address set"))?;
        self.send_to(packet, addr)
    }

    /// Create a packet with proper header
    pub fn create_packet(&mut self, payload: PacketType) -> Packet {
        let sequence = self.send_sequence;
        self.send_sequence = self.send_sequence.wrapping_add(1);

        let (ack, ack_bitfield) = self.receive_tracker.get_ack_data();
        let header = PacketHeader::new(sequence, ack, ack_bitfield);

        Packet::new(header, payload)
    }

    /// Receive packets (non-blocking)
    pub fn receive(&mut self) -> io::Result<Vec<(Packet, SocketAddr)>> {
        let mut packets = Vec::new();

        loop {
            match self.socket.recv_from(&mut self.recv_buffer) {
                Ok((size, addr)) => {
                    if size < std::mem::size_of::<PacketHeader>() {
                        continue; // Too small to be valid
                    }

                    // Validate magic number quickly
                    let magic = u32::from_le_bytes([
                        self.recv_buffer[0],
                        self.recv_buffer[1],
                        self.recv_buffer[2],
                        self.recv_buffer[3],
                    ]);
                    if magic != PROTOCOL_MAGIC {
                        continue; // Not our protocol
                    }

                    match Packet::deserialize(&self.recv_buffer[..size]) {
                        Ok(packet) => {
                            if !packet.header.is_valid() {
                                continue;
                            }

                            // Check for duplicate
                            if !self.receive_tracker.record_received(packet.header.sequence) {
                                continue; // Duplicate packet
                            }

                            // Process acknowledgments
                            let acked = self
                                .ack_tracker
                                .process_ack(packet.header.ack, packet.header.ack_bitfield);

                            // Update stats
                            self.stats.packets_received += 1;
                            self.stats.bytes_received += size as u64;
                            self.stats.rtt_ms = self.ack_tracker.srtt;
                            self.stats.rtt_variance = self.ack_tracker.rtt_var;

                            // Estimate packet loss
                            if self.stats.packets_sent > 0 {
                                let unacked = self.ack_tracker.get_unacked_count() as f32;
                                let sent = self.stats.packets_sent as f32;
                                self.stats.packet_loss_percent = (unacked / sent.max(1.0)) * 100.0;
                            }

                            self.last_receive_time = Instant::now();
                            packets.push((packet, addr));
                        }
                        Err(_) => continue, // Invalid packet
                    }
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }

        Ok(packets)
    }

    /// Check if connection has timed out
    pub fn is_timed_out(&self) -> bool {
        self.last_receive_time.elapsed() > self.timeout
    }

    /// Set timeout duration
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    /// Reset connection state
    pub fn reset(&mut self) {
        self.state = ConnectionState::Disconnected;
        self.send_sequence = 0;
        self.ack_tracker = AckTracker::new(256);
        self.receive_tracker = ReceiveTracker::new();
        self.stats = NetworkStats::default();
        self.last_receive_time = Instant::now();
    }

    /// Get running flag for async shutdown
    pub fn running(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.running)
    }

    /// Signal shutdown
    pub fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

/// Connection handler for server-side client management
#[derive(Debug)]
pub struct ClientConnection {
    /// Client's network address
    pub addr: SocketAddr,
    /// Assigned client ID
    pub client_id: u32,
    /// Connection state
    pub state: ConnectionState,
    /// Client's challenge salt
    pub client_salt: u64,
    /// Server's challenge salt
    pub server_salt: u64,
    /// Last acknowledged command sequence from this client
    pub last_command_ack: u32,
    /// Last packet receive time
    pub last_receive_time: Instant,
    /// Assigned entity ID for this player
    pub entity_id: Option<u32>,
    /// Per-client ACK tracking
    pub receive_tracker: ReceiveTracker,
    /// Send sequence for this client
    pub send_sequence: u32,
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

/// Simple random u64 generator (for salts)
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

/// Server-side connection manager
#[derive(Debug)]
pub struct ConnectionManager {
    /// Connected clients by address
    clients_by_addr: HashMap<SocketAddr, u32>,
    /// Client connections by ID
    clients: HashMap<u32, ClientConnection>,
    /// Next client ID
    next_client_id: u32,
    /// Maximum clients
    max_clients: usize,
    /// Connection timeout
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

    /// Get or create a pending connection for an address
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

    /// Get client by address
    pub fn get_by_addr(&self, addr: &SocketAddr) -> Option<&ClientConnection> {
        self.clients_by_addr
            .get(addr)
            .and_then(|id| self.clients.get(id))
    }

    /// Get mutable client by address
    pub fn get_by_addr_mut(&mut self, addr: &SocketAddr) -> Option<&mut ClientConnection> {
        if let Some(&id) = self.clients_by_addr.get(addr) {
            self.clients.get_mut(&id)
        } else {
            None
        }
    }

    /// Get client by ID
    pub fn get(&self, client_id: u32) -> Option<&ClientConnection> {
        self.clients.get(&client_id)
    }

    /// Get mutable client by ID
    pub fn get_mut(&mut self, client_id: u32) -> Option<&mut ClientConnection> {
        self.clients.get_mut(&client_id)
    }

    /// Remove a client
    pub fn remove(&mut self, client_id: u32) -> Option<ClientConnection> {
        if let Some(conn) = self.clients.remove(&client_id) {
            self.clients_by_addr.remove(&conn.addr);
            Some(conn)
        } else {
            None
        }
    }

    /// Remove client by address
    pub fn remove_by_addr(&mut self, addr: &SocketAddr) -> Option<ClientConnection> {
        if let Some(client_id) = self.clients_by_addr.remove(addr) {
            self.clients.remove(&client_id)
        } else {
            None
        }
    }

    /// Iterate over all connected clients
    pub fn iter(&self) -> impl Iterator<Item = &ClientConnection> {
        self.clients.values()
    }

    /// Iterate mutably over all connected clients
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut ClientConnection> {
        self.clients.values_mut()
    }

    /// Remove timed out connections, returns removed client IDs
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

    /// Get number of connected clients
    pub fn client_count(&self) -> usize {
        self.clients
            .values()
            .filter(|c| c.state == ConnectionState::Connected)
            .count()
    }

    /// Get total number of clients (including pending)
    pub fn total_count(&self) -> usize {
        self.clients.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_receive_tracker_bitfield() {
        let mut tracker = ReceiveTracker::new();

        // Receive packets 1, 2, 3
        tracker.record_received(1);
        tracker.record_received(2);
        tracker.record_received(3);

        let (ack, bitfield) = tracker.get_ack_data();
        assert_eq!(ack, 3);
        assert_eq!(bitfield & 0b11, 0b11); // Packets 1 and 2 in bitfield
    }

    #[test]
    fn test_receive_tracker_out_of_order() {
        let mut tracker = ReceiveTracker::new();

        // Receive out of order
        tracker.record_received(3);
        tracker.record_received(1);
        tracker.record_received(2);

        let (ack, bitfield) = tracker.get_ack_data();
        assert_eq!(ack, 3);
        assert_eq!(bitfield & 0b11, 0b11);
    }

    #[test]
    fn test_duplicate_detection() {
        let mut tracker = ReceiveTracker::new();

        assert!(tracker.record_received(1));
        assert!(!tracker.record_received(1)); // Duplicate
        assert!(tracker.record_received(2));
    }

    #[test]
    fn test_ack_tracker_rtt() {
        let mut tracker = AckTracker::new(32);

        tracker.track_packet(1, vec![]);
        std::thread::sleep(Duration::from_millis(10));

        tracker.process_ack(1, 0);

        // RTT should be updated
        assert!(tracker.srtt > 0.0);
    }
}
