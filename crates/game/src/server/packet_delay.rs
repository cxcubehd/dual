use std::net::SocketAddr;
use std::time::Instant;

use crate::Packet;

#[derive(Debug)]
pub struct DelayedPacket {
    pub send_time: Instant,
    pub packet: Packet,
    pub addr: SocketAddr,
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
