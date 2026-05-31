use std::time::Instant;

use crate::PhysicsSync;

use super::GameServer;
use crate::server::{DisconnectReason, ServerEvent};

impl GameServer {
    pub fn tick_once(&mut self) {
        let now = Instant::now();
        let delta = now - self.last_tick_time;
        self.last_tick_time = now;
        self.accumulator += delta;

        if let Err(e) = self.process_network() {
            self.pending_events.push_back(ServerEvent::Error {
                message: format!("Network error: {}", e),
            });
        }

        self.process_resends();
        self.process_delayed_packets();

        while self.accumulator >= self.tick_duration {
            self.accumulator -= self.tick_duration;
            self.tick();
        }
    }

    fn tick(&mut self) {
        self.process_commands();

        self.physics.step();
        PhysicsSync::sync_physics_to_world(&self.physics, &mut self.world);

        self.world.advance_tick();
        self.tick = self.world.tick();

        let snapshot = self.world.snapshot(0);
        self.snapshot_history.push(snapshot);

        if self.tick % self.config.snapshot_send_rate == 0 {
            self.broadcast_snapshots();
        }

        let timed_out = self.connections.cleanup_timed_out();
        for client_id in timed_out {
            self.pending_events
                .push_back(ServerEvent::ClientDisconnected {
                    client_id,
                    reason: DisconnectReason::Timeout,
                });
        }
    }

    fn process_commands(&mut self) {
        while let Some(queued) = self.command_queue.pop_front() {
            if let Some(client) = self.connections.get_mut(queued.client_id) {
                if queued.command.command_sequence > client.last_command_ack {
                    client.last_command_ack = queued.command.command_sequence;
                }

                if let Some(entity_id) = client.entity_id {
                    if let Some(entity) = self.world.get_by_id_mut(entity_id) {
                        self.command_processor
                            .process(&queued.command, entity, &mut self.physics);
                    }
                }
            }
        }
    }
}
