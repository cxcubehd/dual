use std::io;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::connection::ConnectionState;
use super::protocol::{MAX_PACKET_SIZE, Packet};
use super::stats::NetworkStats;

const DEFAULT_TIMEOUT_SECS: u64 = 120;

pub struct NetworkEndpoint {
    socket: UdpSocket,
    local_addr: SocketAddr,
    remote_addr: Option<SocketAddr>,
    state: ConnectionState,
    stats: NetworkStats,
    recv_buffer: [u8; MAX_PACKET_SIZE],
    timeout: Duration,
    last_receive_time: Instant,
    running: Arc<AtomicBool>,
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
            stats: NetworkStats::default(),
            recv_buffer: [0u8; MAX_PACKET_SIZE],
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            last_receive_time: Instant::now(),
            running: Arc::new(AtomicBool::new(true)),
        })
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

                            self.stats.packets_received += 1;
                            self.stats.bytes_received += size as u64;

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
