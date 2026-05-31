use std::io;
use std::net::SocketAddr;

use glam::Vec3;

use crate::{
    ClientCommand, ConnectionState, EntityHandle, Packet, PacketHeader, PacketType, PhysicsSync,
    Reliability,
};

use super::{GameServer, QueuedCommand};
use crate::server::{DisconnectReason, ServerEvent};

impl GameServer {
    pub(super) fn handle_payload(
        &mut self,
        payload: PacketType,
        addr: SocketAddr,
    ) -> io::Result<()> {
        match payload {
            PacketType::ConnectionRequest { client_salt } => {
                self.handle_connection_request(addr, client_salt)?;
            }
            PacketType::ChallengeResponse { combined_salt } => {
                self.handle_challenge_response(addr, combined_salt)?;
            }
            PacketType::ClientCommand(command) => {
                self.handle_client_command(addr, command)?;
            }
            PacketType::Ping { timestamp } => {
                self.handle_ping(addr, timestamp)?;
            }
            PacketType::SnapshotAck { received_tick } => {
                self.handle_snapshot_ack(addr, received_tick)?;
            }
            PacketType::Disconnect => {
                self.handle_disconnect(addr)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_connection_request(&mut self, addr: SocketAddr, client_salt: u64) -> io::Result<()> {
        self.pending_events
            .push_back(ServerEvent::ClientConnecting { addr });

        let global_packet_loss = self.config.global_packet_loss.clone();

        let client = match self.connections.get_or_create_pending(addr, client_salt) {
            Ok(c) => c,
            Err(reason) => {
                let header = PacketHeader::new(0, 0, 0, PacketHeader::CHANNEL_UNRELIABLE, 0);
                let packet = Packet::new(
                    header,
                    PacketType::ConnectionDenied {
                        reason: reason.to_string(),
                    },
                );
                self.send_packet_simulated(packet, addr)?;
                self.pending_events
                    .push_back(ServerEvent::ConnectionDenied {
                        addr,
                        reason: reason.to_string(),
                    });
                return Ok(());
            }
        };

        if let Some(sim) = global_packet_loss {
            client.packet_loss_sim = sim.clone();
            client.incoming_packet_loss_sim = sim;
        }

        let server_salt = client.server_salt;
        let challenge = client.combined_salt();

        let packet = client.send_packet(
            PacketType::ConnectionChallenge {
                server_salt,
                challenge,
            },
            Reliability::Reliable,
        );

        self.send_packet_simulated(packet, addr)?;

        Ok(())
    }

    fn handle_challenge_response(
        &mut self,
        addr: SocketAddr,
        combined_salt: u64,
    ) -> io::Result<()> {
        let Some(client) = self.connections.get_by_addr_mut(&addr) else {
            return Ok(());
        };

        if combined_salt != client.combined_salt() {
            self.pending_events.push_back(ServerEvent::Error {
                message: format!("Invalid challenge response from {}", addr),
            });
            return Ok(());
        }

        client.state = ConnectionState::Connected;
        let client_id = client.client_id;

        let spawn_pos = Vec3::new(0.0, 2.0, 0.0);
        let entity_handle = self.world.spawn_player(spawn_pos);
        let entity_id = entity_handle.id();

        let config = self.command_processor.config();
        if let Some(entity) = self.world.get_by_id_mut(entity_id) {
            PhysicsSync::create_physics_body(
                entity,
                &mut self.physics,
                config.player_radius,
                config.player_height,
            );
        }

        client.entity_id = Some(entity_id);

        self.pending_events.push_back(ServerEvent::ClientConnected {
            client_id,
            addr,
            entity_id,
        });

        let packet = client.send_packet(
            PacketType::ConnectionAccepted {
                client_id,
                entity_id,
            },
            Reliability::Reliable,
        );

        self.send_packet_simulated(packet, addr)?;

        Ok(())
    }

    fn handle_client_command(
        &mut self,
        addr: SocketAddr,
        command: ClientCommand,
    ) -> io::Result<()> {
        let Some(client) = self.connections.get_by_addr(&addr) else {
            return Ok(());
        };

        if client.state != ConnectionState::Connected {
            return Ok(());
        }

        self.command_queue.push_back(QueuedCommand {
            client_id: client.client_id,
            command,
        });

        Ok(())
    }

    fn handle_ping(&mut self, addr: SocketAddr, timestamp: u64) -> io::Result<()> {
        if let Some(client) = self.connections.get_by_addr_mut(&addr) {
            let packet =
                client.send_packet(PacketType::Pong { timestamp }, Reliability::Unreliable);
            self.send_packet_simulated(packet, addr)?;
        }
        Ok(())
    }

    fn handle_snapshot_ack(&mut self, addr: SocketAddr, received_tick: u32) -> io::Result<()> {
        if let Some(client) = self.connections.get_by_addr_mut(&addr) {
            if received_tick > client.last_acked_tick {
                client.last_acked_tick = received_tick;
            }
        }
        Ok(())
    }

    fn handle_disconnect(&mut self, addr: SocketAddr) -> io::Result<()> {
        if let Some(client) = self.connections.remove_by_addr(&addr) {
            if let Some(entity_id) = client.entity_id {
                self.world.despawn(EntityHandle(entity_id));
            }
            self.pending_events
                .push_back(ServerEvent::ClientDisconnected {
                    client_id: client.client_id,
                    reason: DisconnectReason::Graceful,
                });
        }
        Ok(())
    }
}
