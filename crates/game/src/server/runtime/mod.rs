mod admin;
mod connections;
mod packets;
mod snapshots;
mod tick;

use std::collections::{BinaryHeap, VecDeque};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crate::{
    ClientCommand, CommandProcessor, ConnectionManager, NetworkEndpoint, PhysicsWorld,
    SnapshotBuffer, TestingGround, World,
};

use super::packet_delay::DelayedPacket;
use super::{ServerConfig, ServerEvent};

#[derive(Debug)]
struct QueuedCommand {
    client_id: u32,
    command: ClientCommand,
}

pub struct GameServer {
    endpoint: NetworkEndpoint,
    connections: ConnectionManager,
    config: ServerConfig,
    world: World,
    physics: PhysicsWorld,
    command_processor: CommandProcessor,
    snapshot_history: SnapshotBuffer,
    command_queue: VecDeque<QueuedCommand>,
    delayed_packets: BinaryHeap<DelayedPacket>,
    delayed_incoming_packets: BinaryHeap<DelayedPacket>,
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
        let endpoint = NetworkEndpoint::bind(bind_addr)?;
        let tick_duration = Duration::from_secs_f64(1.0 / config.tick_rate as f64);

        let mut pending_events = VecDeque::new();
        pending_events.push_back(ServerEvent::ClientConnecting {
            addr: endpoint.local_addr(),
        });

        let mut world = World::new();
        let mut physics = PhysicsWorld::new();

        let mut testing_ground = TestingGround::new();
        testing_ground.spawn(&mut world, &mut physics);

        Ok(Self {
            endpoint,
            connections: ConnectionManager::new(config.max_clients),
            world,
            physics,
            command_processor: CommandProcessor::new(),
            snapshot_history: SnapshotBuffer::new(config.snapshot_buffer_size),
            command_queue: VecDeque::new(),
            delayed_packets: BinaryHeap::new(),
            delayed_incoming_packets: BinaryHeap::new(),
            tick: 0,
            tick_duration,
            last_tick_time: Instant::now(),
            accumulator: Duration::ZERO,
            running: Arc::new(AtomicBool::new(true)),
            start_time: Instant::now(),
            pending_events,
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
}
