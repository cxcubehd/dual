use std::io;

use glam::{Quat, Vec3};

use crate::{PacketType, Reliability, WorldSnapshot};

use super::NetworkClient;

impl NetworkClient {
    pub(super) fn handle_snapshot(&mut self, snapshot: WorldSnapshot) -> io::Result<()> {
        let received_tick = snapshot.tick;

        self.estimated_server_tick = snapshot
            .tick
            .saturating_add(self.config.interpolation_delay);
        self.last_server_ack = snapshot.last_command_ack;
        self.clock_offset_ms = snapshot.server_time_ms as i64 - current_time_ms();

        if let Some(entity_id) = self.entity_id {
            if let Some(local_state) = snapshot.entities.iter().find(|e| e.entity_id == entity_id) {
                let position = Vec3::from(local_state.position);
                let orientation_arr = local_state.decode_orientation();
                let orientation = Quat::from_xyzw(
                    orientation_arr[0],
                    orientation_arr[1],
                    orientation_arr[2],
                    orientation_arr[3],
                );
                self.prediction
                    .reconcile(position, orientation, snapshot.last_command_ack);
            }
        }

        self.interpolation.push_snapshot(snapshot);

        self.send_snapshot_ack(received_tick)?;

        Ok(())
    }

    fn send_snapshot_ack(&mut self, received_tick: u32) -> io::Result<()> {
        let packet = self.connection.send_packet(
            PacketType::SnapshotAck { received_tick },
            Reliability::Unreliable,
        );
        self.endpoint.send(&packet)?;
        Ok(())
    }
}

fn current_time_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
