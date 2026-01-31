use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::connection::ConnectionState;
use super::protocol::{Packet, PacketHeader, PacketType, MAX_PACKET_SIZE};
use super::stats::NetworkStats;
use super::tracking::{AckTracker, ReceiveTracker};

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
