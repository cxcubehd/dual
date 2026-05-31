use std::io;

use crate::player::InputState;
use crate::{PacketType, Reliability};

use super::NetworkClient;

impl NetworkClient {
    pub(super) fn process_resends(&mut self) -> io::Result<()> {
        let packets = self.connection.collect_resends();
        for packet in packets {
            let _ = self.endpoint.send(&packet);
        }
        Ok(())
    }

    pub(super) fn send_command(&mut self, input: &InputState) -> io::Result<()> {
        let command = input.to_command(self.estimated_server_tick, self.command_sequence);
        let sequence = self.command_sequence;
        self.command_sequence = self.command_sequence.wrapping_add(1);

        self.prediction.store_command(&command, sequence);

        let packet = self
            .connection
            .send_packet(PacketType::ClientCommand(command), Reliability::Unreliable);
        self.endpoint.send(&packet)?;

        Ok(())
    }

    pub(super) fn send_ping(&mut self) -> io::Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let packet = self
            .connection
            .send_packet(PacketType::Ping { timestamp }, Reliability::Unreliable);
        self.endpoint.send(&packet)?;

        Ok(())
    }

    pub(super) fn process_network(&mut self) -> io::Result<()> {
        let packets = self.endpoint.receive()?;

        for (packet, _addr) in packets {
            let payloads = self.connection.process_packet(packet);
            for payload in payloads {
                self.handle_payload(payload)?;
            }
        }

        Ok(())
    }

    pub(super) fn handle_pong(&mut self, timestamp: u64) -> io::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let rtt = now.saturating_sub(timestamp);
        log::debug!("Ping RTT: {} ms", rtt);

        Ok(())
    }
}
