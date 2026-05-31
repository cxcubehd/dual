use std::net::SocketAddr;

use crate::{ConnectionState, PacketType, Reliability, WorldSnapshot};

use super::GameServer;

impl GameServer {
    pub(super) fn broadcast_snapshots(&mut self) {
        let client_data: Vec<(SocketAddr, u32, u32)> = self
            .connections
            .iter()
            .filter(|c| c.state == ConnectionState::Connected)
            .map(|c| (c.addr, c.last_command_ack, c.last_acked_tick))
            .collect();

        let current_tick = self.tick;
        let max_delta_age = self.config.snapshot_buffer_size as u32 / 2;

        for (addr, last_cmd_ack, last_acked_tick) in client_data {
            let snapshot = self.generate_client_snapshot(
                last_cmd_ack,
                last_acked_tick,
                current_tick,
                max_delta_age,
            );

            if let Some(client) = self.connections.get_by_addr_mut(&addr) {
                let packet = client
                    .send_packet(PacketType::WorldSnapshot(snapshot), Reliability::Unreliable);
                let _ = self.send_packet_simulated(packet, addr);
            }
        }
    }

    fn generate_client_snapshot(
        &self,
        last_cmd_ack: u32,
        last_acked_tick: u32,
        current_tick: u32,
        max_delta_age: u32,
    ) -> WorldSnapshot {
        let baseline_age = current_tick.saturating_sub(last_acked_tick);

        if last_acked_tick > 0 && baseline_age < max_delta_age {
            if let Some(baseline) = self.snapshot_history.get(last_acked_tick) {
                return self.world.delta_from_baseline(baseline, last_cmd_ack);
            }
        }

        self.world.snapshot(last_cmd_ack)
    }
}
