use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use super::stats::{PacketLossSimulation, rand_u64};
use super::tracking::ReceiveTracker;

const DEFAULT_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    ChallengeResponse,
    Connected,
    Disconnecting,
}

#[derive(Debug)]
pub struct ClientConnection {
    pub addr: SocketAddr,
    pub client_id: u32,
    pub state: ConnectionState,
    pub client_salt: u64,
    pub server_salt: u64,
    pub last_command_ack: u32,
    pub last_acked_tick: u32,
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
            last_acked_tick: 0,
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
