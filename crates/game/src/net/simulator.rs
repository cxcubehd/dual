use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use super::protocol::Packet;
use super::stats::{rand_percent, PacketLossSimulation};

#[derive(Debug)]
struct DelayedPacket {
    release_time: Instant,
    packet: Packet,
    addr: SocketAddr,
}

impl PartialEq for DelayedPacket {
    fn eq(&self, other: &Self) -> bool {
        self.release_time == other.release_time
    }
}

impl Eq for DelayedPacket {}

impl PartialOrd for DelayedPacket {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DelayedPacket {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse order for min-heap
        other.release_time.cmp(&self.release_time)
    }
}

#[derive(Debug, Default)]
pub struct NetworkSimulator {
    configs: HashMap<SocketAddr, PacketLossSimulation>,
    inbound_queue: BinaryHeap<DelayedPacket>,
    outbound_queue: BinaryHeap<DelayedPacket>,
}

impl NetworkSimulator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_config(&mut self, addr: SocketAddr, config: PacketLossSimulation) {
        if config.enabled {
            self.configs.insert(addr, config);
        } else {
            self.configs.remove(&addr);
        }
    }

    pub fn get_config(&self, addr: &SocketAddr) -> Option<&PacketLossSimulation> {
        self.configs.get(addr)
    }

    pub fn should_drop(&self, addr: &SocketAddr) -> bool {
        self.configs
            .get(addr)
            .map_or(false, |sim| sim.should_drop())
    }

    pub fn delay_for(&self, addr: &SocketAddr) -> Duration {
        self.configs.get(addr).map_or(Duration::ZERO, |sim| {
            Duration::from_millis(sim.delay_ms() as u64)
        })
    }

    pub fn enqueue_inbound(&mut self, packet: Packet, addr: SocketAddr) {
        let delay = self.delay_for(&addr);
        if delay.is_zero() {
            // We still use the queue to keep things simple or we could return it immediately.
            // But for consistency let's just queue it with now.
            self.inbound_queue.push(DelayedPacket {
                release_time: Instant::now(),
                packet,
                addr,
            });
        } else {
            self.inbound_queue.push(DelayedPacket {
                release_time: Instant::now() + delay,
                packet,
                addr,
            });
        }
    }

    pub fn enqueue_outbound(&mut self, packet: Packet, addr: SocketAddr) {
        let delay = self.delay_for(&addr);
        if delay.is_zero() {
            self.outbound_queue.push(DelayedPacket {
                release_time: Instant::now(),
                packet,
                addr,
            });
        } else {
            self.outbound_queue.push(DelayedPacket {
                release_time: Instant::now() + delay,
                packet,
                addr,
            });
        }
    }

    pub fn take_inbound(&mut self) -> Vec<(Packet, SocketAddr)> {
        let mut packets = Vec::new();
        let now = Instant::now();
        while let Some(delayed) = self.inbound_queue.peek() {
            if delayed.release_time <= now {
                let delayed = self.inbound_queue.pop().unwrap();
                packets.push((delayed.packet, delayed.addr));
            } else {
                break;
            }
        }
        packets
    }

    pub fn take_outbound(&mut self) -> Vec<(Packet, SocketAddr)> {
        let mut packets = Vec::new();
        let now = Instant::now();
        while let Some(delayed) = self.outbound_queue.peek() {
            if delayed.release_time <= now {
                let delayed = self.outbound_queue.pop().unwrap();
                packets.push((delayed.packet, delayed.addr));
            } else {
                break;
            }
        }
        packets
    }
}
