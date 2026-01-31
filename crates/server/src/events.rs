use std::net::SocketAddr;

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
