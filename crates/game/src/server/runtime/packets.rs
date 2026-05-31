use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use crate::{Packet, PacketType};

use super::GameServer;
use crate::server::ServerEvent;
use crate::server::packet_delay::DelayedPacket;

impl GameServer {
    pub(super) fn process_resends(&mut self) {
        let mut packets_to_send = Vec::new();
        for client in self.connections.iter_mut() {
            let resends = client.collect_resends();
            for packet in resends {
                packets_to_send.push((client.addr, packet));
            }
        }

        for (addr, packet) in packets_to_send {
            let _ = self.send_packet_simulated(packet, addr);
        }
    }

    pub(super) fn process_delayed_packets(&mut self) {
        let now = Instant::now();
        while let Some(packet) = self.delayed_packets.peek() {
            if packet.send_time <= now {
                let DelayedPacket { packet, addr, .. } = self.delayed_packets.pop().unwrap();
                if let Err(e) = self.endpoint.send_to(&packet, addr) {
                    self.pending_events.push_back(ServerEvent::Error {
                        message: format!("Failed to send delayed packet to {}: {}", addr, e),
                    });
                }
            } else {
                break;
            }
        }
    }

    pub(super) fn send_packet_simulated(
        &mut self,
        packet: Packet,
        addr: SocketAddr,
    ) -> io::Result<()> {
        let mut delay = 0;
        let mut should_drop = false;

        if let Some(client) = self.connections.get_by_addr(&addr) {
            should_drop = client.packet_loss_sim.should_drop();
            delay = client.packet_loss_sim.delay_ms();
        } else if let Some(ref sim) = self.config.global_packet_loss {
            should_drop = sim.should_drop();
            delay = sim.delay_ms();
        }

        if should_drop {
            return Ok(());
        }

        if delay == 0 {
            self.endpoint.send_to(&packet, addr).map(|_| ())
        } else {
            self.delayed_packets.push(DelayedPacket {
                send_time: Instant::now() + Duration::from_millis(delay as u64),
                packet,
                addr,
            });
            Ok(())
        }
    }

    pub(super) fn process_network(&mut self) -> io::Result<()> {
        let packets = self.endpoint.receive()?;

        for (packet, addr) in packets {
            let mut delay = 0;
            let mut should_drop = false;

            if let Some(client) = self.connections.get_by_addr(&addr) {
                should_drop = client.incoming_packet_loss_sim.should_drop();
                delay = client.incoming_packet_loss_sim.delay_ms();
            } else if let Some(ref sim) = self.config.global_packet_loss {
                should_drop = sim.should_drop();
                delay = sim.delay_ms();
            }

            if should_drop {
                continue;
            }

            if delay > 0 {
                self.delayed_incoming_packets.push(DelayedPacket {
                    send_time: Instant::now() + Duration::from_millis(delay as u64),
                    packet,
                    addr,
                });
            } else {
                self.handle_received_packet(packet, addr)?;
            }
        }

        let now = Instant::now();
        while let Some(packet) = self.delayed_incoming_packets.peek() {
            if packet.send_time <= now {
                let DelayedPacket { packet, addr, .. } =
                    self.delayed_incoming_packets.pop().unwrap();
                self.handle_received_packet(packet, addr)?;
            } else {
                break;
            }
        }

        Ok(())
    }

    fn handle_received_packet(&mut self, packet: Packet, addr: SocketAddr) -> io::Result<()> {
        if let Some(client) = self.connections.get_by_addr_mut(&addr) {
            let payloads = client.process_packet(packet);
            for payload in payloads {
                self.handle_payload(payload, addr)?;
            }
        } else if let PacketType::ConnectionRequest { .. } = packet.payload {
            self.handle_payload(packet.payload, addr)?;
        }
        Ok(())
    }
}
