use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use super::protocol::{Packet, PacketHeader, PacketType};
use super::stats::{PacketLossSimulation, rand_u64};
use super::tracking::{AckTracker, ReceiveTracker};

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const RELIABLE_HISTORY_SIZE: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    ChallengeResponse,
    Connected,
    Disconnecting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reliability {
    Unreliable,
    Reliable,
    Ordered,
}

#[derive(Debug)]
pub struct ClientConnection {
    pub addr: SocketAddr,
    pub client_id: u32,
    pub state: ConnectionState,
    pub client_salt: u64,
    pub server_salt: u64,

    // Game state tracking
    pub last_command_ack: u32,
    pub last_acked_tick: u32,
    pub entity_id: Option<u32>,
    pub lobby_id: Option<u64>,

    // Network stats/simulation
    pub last_receive_time: Instant,
    pub packet_loss_sim: PacketLossSimulation,

    // Reliability - Send
    pub send_sequence: u32,
    pub ack_tracker: AckTracker,
    next_reliable_seq: u16,
    next_ordered_seq: u16,

    // WireSeq -> (Channel, ChannelSeq)
    inflight_packets: HashMap<u32, (u8, u16)>,

    // ChannelSeq -> (Payload, LastSendTime)
    pending_reliable: HashMap<u16, (PacketType, Instant)>,
    pending_ordered: HashMap<u16, (PacketType, Instant)>,

    // Reliability - Receive
    pub receive_tracker: ReceiveTracker,
    received_reliable_history: VecDeque<u16>,
    next_expected_ordered: u16,
    ordered_buffer: HashMap<u16, PacketType>,
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
            last_acked_tick: 0,
            last_receive_time: Instant::now(),
            entity_id: None,
            lobby_id: None,
            packet_loss_sim: PacketLossSimulation::default(),

            send_sequence: 0,
            ack_tracker: AckTracker::new(1024),
            next_reliable_seq: 0,
            next_ordered_seq: 0,
            inflight_packets: HashMap::new(),
            pending_reliable: HashMap::new(),
            pending_ordered: HashMap::new(),

            receive_tracker: ReceiveTracker::new(),
            received_reliable_history: VecDeque::with_capacity(RELIABLE_HISTORY_SIZE),
            next_expected_ordered: 0,
            ordered_buffer: HashMap::new(),
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

    pub fn send_packet(&mut self, payload: PacketType, reliability: Reliability) -> Packet {
        let (ack, ack_bitfield) = self.receive_tracker.ack_data();
        let sequence = self.send_sequence;
        self.send_sequence = self.send_sequence.wrapping_add(1);

        // Track for RTT
        self.ack_tracker.track_packet(sequence);

        let (channel, channel_seq) = match reliability {
            Reliability::Unreliable => (PacketHeader::CHANNEL_UNRELIABLE, 0),
            Reliability::Reliable => {
                let seq = self.next_reliable_seq;
                self.next_reliable_seq = self.next_reliable_seq.wrapping_add(1);

                self.pending_reliable
                    .insert(seq, (payload.clone(), Instant::now()));
                self.inflight_packets
                    .insert(sequence, (PacketHeader::CHANNEL_RELIABLE, seq));

                (PacketHeader::CHANNEL_RELIABLE, seq)
            }
            Reliability::Ordered => {
                let seq = self.next_ordered_seq;
                self.next_ordered_seq = self.next_ordered_seq.wrapping_add(1);

                self.pending_ordered
                    .insert(seq, (payload.clone(), Instant::now()));
                self.inflight_packets
                    .insert(sequence, (PacketHeader::CHANNEL_ORDERED, seq));

                (PacketHeader::CHANNEL_ORDERED, seq)
            }
        };

        let header = PacketHeader::new(sequence, ack, ack_bitfield, channel, channel_seq);
        Packet::new(header, payload)
    }

    pub fn process_packet(&mut self, packet: Packet) -> Vec<PacketType> {
        self.touch();

        let header = &packet.header;

        // Update ACKs
        let acked_sequences = self
            .ack_tracker
            .process_ack(header.ack, header.ack_bitfield);
        for seq in acked_sequences {
            if let Some((channel, c_seq)) = self.inflight_packets.remove(&seq) {
                match channel {
                    PacketHeader::CHANNEL_RELIABLE => {
                        self.pending_reliable.remove(&c_seq);
                    }
                    PacketHeader::CHANNEL_ORDERED => {
                        self.pending_ordered.remove(&c_seq);
                    }
                    _ => {}
                }
            }
        }

        // Update receive tracker (wire sequence)
        if !self.receive_tracker.record_received(header.sequence) {
            return Vec::new();
        }

        match header.channel {
            PacketHeader::CHANNEL_UNRELIABLE => vec![packet.payload],
            PacketHeader::CHANNEL_RELIABLE => {
                let seq = header.channel_seq;
                if self.received_reliable_history.contains(&seq) {
                    Vec::new()
                } else {
                    if self.received_reliable_history.len() >= RELIABLE_HISTORY_SIZE {
                        self.received_reliable_history.pop_front();
                    }
                    self.received_reliable_history.push_back(seq);
                    vec![packet.payload]
                }
            }
            PacketHeader::CHANNEL_ORDERED => {
                let seq = header.channel_seq;

                if seq == self.next_expected_ordered {
                    let mut result = Vec::new();
                    result.push(packet.payload);
                    self.next_expected_ordered = self.next_expected_ordered.wrapping_add(1);

                    while let Some(buffered) =
                        self.ordered_buffer.remove(&self.next_expected_ordered)
                    {
                        result.push(buffered);
                        self.next_expected_ordered = self.next_expected_ordered.wrapping_add(1);
                    }
                    result
                } else if self.sequence_greater_than_u16(seq, self.next_expected_ordered) {
                    self.ordered_buffer.insert(seq, packet.payload);
                    Vec::new()
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    pub fn collect_resends(&mut self) -> Vec<Packet> {
        let mut packets = Vec::new();
        let now = Instant::now();
        let rtt = self.ack_tracker.srtt();
        let timeout = if rtt > 0.0 {
            Duration::from_secs_f32(rtt * 1.5 / 1000.0).max(Duration::from_millis(50))
        } else {
            Duration::from_millis(200)
        };

        // Check reliable pending
        // Iterate and clone to avoid borrow issues
        let mut reliable_resends = Vec::new();
        for (seq, (_, last_send)) in &self.pending_reliable {
            if now.duration_since(*last_send) > timeout {
                reliable_resends.push(*seq);
            }
        }

        for seq in reliable_resends {
            if let Some((payload, last_send)) = self.pending_reliable.get_mut(&seq) {
                *last_send = now;
                let wire_seq = self.send_sequence;
                self.send_sequence = self.send_sequence.wrapping_add(1);
                self.inflight_packets
                    .insert(wire_seq, (PacketHeader::CHANNEL_RELIABLE, seq));

                let (ack, ack_bitfield) = self.receive_tracker.ack_data();
                let header = PacketHeader::new(
                    wire_seq,
                    ack,
                    ack_bitfield,
                    PacketHeader::CHANNEL_RELIABLE,
                    seq,
                );
                packets.push(Packet::new(header, payload.clone()));
            }
        }

        // Check ordered pending
        let mut ordered_resends = Vec::new();
        for (seq, (_, last_send)) in &self.pending_ordered {
            if now.duration_since(*last_send) > timeout {
                ordered_resends.push(*seq);
            }
        }

        for seq in ordered_resends {
            if let Some((payload, last_send)) = self.pending_ordered.get_mut(&seq) {
                *last_send = now;
                let wire_seq = self.send_sequence;
                self.send_sequence = self.send_sequence.wrapping_add(1);
                self.inflight_packets
                    .insert(wire_seq, (PacketHeader::CHANNEL_ORDERED, seq));

                let (ack, ack_bitfield) = self.receive_tracker.ack_data();
                let header = PacketHeader::new(
                    wire_seq,
                    ack,
                    ack_bitfield,
                    PacketHeader::CHANNEL_ORDERED,
                    seq,
                );
                packets.push(Packet::new(header, payload.clone()));
            }
        }

        packets
    }

    fn sequence_greater_than_u16(&self, s1: u16, s2: u16) -> bool {
        let half = u16::MAX / 2;
        ((s1 > s2) && (s1 - s2 <= half)) || ((s1 < s2) && (s2 - s1 > half))
    }
}

#[derive(Debug)]
pub struct ConnectionManager {
    clients_by_addr: HashMap<SocketAddr, u32>,
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
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }

    pub fn with_timeout(max_clients: usize, timeout_secs: u64) -> Self {
        Self {
            clients_by_addr: HashMap::new(),
            clients: HashMap::new(),
            next_client_id: 1,
            max_clients,
            timeout: Duration::from_secs(timeout_secs),
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

    pub fn get_by_addr(&self, addr: &SocketAddr) -> Option<&ClientConnection> {
        self.clients_by_addr
            .get(addr)
            .and_then(|id| self.clients.get(id))
    }

    pub fn get_by_addr_mut(&mut self, addr: &SocketAddr) -> Option<&mut ClientConnection> {
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

    pub fn remove_by_addr(&mut self, addr: &SocketAddr) -> Option<ClientConnection> {
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
