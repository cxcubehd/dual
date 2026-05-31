use std::io;

use crate::{ConnectionState, PacketType, Reliability};

use super::NetworkClient;

impl NetworkClient {
    pub(super) fn send_connection_request(&mut self) -> io::Result<()> {
        let packet = self.connection.send_packet(
            PacketType::ConnectionRequest {
                client_salt: self.client_salt,
            },
            Reliability::Unreliable,
        );
        self.endpoint.send(&packet)?;
        Ok(())
    }

    pub(super) fn handle_payload(&mut self, payload: PacketType) -> io::Result<()> {
        match payload {
            PacketType::ConnectionChallenge {
                server_salt,
                challenge,
            } => {
                self.handle_challenge(server_salt, challenge)?;
            }
            PacketType::ConnectionAccepted {
                client_id,
                entity_id,
            } => {
                self.handle_connection_accepted(client_id, entity_id)?;
            }
            PacketType::ConnectionDenied { reason } => {
                self.handle_connection_denied(&reason)?;
            }
            PacketType::WorldSnapshot(snapshot) => {
                self.handle_snapshot(snapshot)?;
            }
            PacketType::Pong { timestamp } => {
                self.handle_pong(timestamp)?;
            }
            PacketType::Disconnect => {
                log::info!("Disconnected by server");
                self.reset();
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_challenge(&mut self, server_salt: u64, challenge: u64) -> io::Result<()> {
        log::debug!("Received challenge from server");

        self.server_salt = Some(server_salt);
        self.state = ConnectionState::ChallengeResponse;
        self.connection.state = ConnectionState::ChallengeResponse;

        let expected_challenge = self.client_salt ^ server_salt;
        if challenge != expected_challenge {
            log::warn!("Challenge mismatch");
            return Ok(());
        }

        let packet = self.connection.send_packet(
            PacketType::ChallengeResponse {
                combined_salt: expected_challenge,
            },
            Reliability::Reliable,
        );
        self.endpoint.send(&packet)?;

        Ok(())
    }

    fn handle_connection_accepted(&mut self, client_id: u32, entity_id: u32) -> io::Result<()> {
        log::info!(
            "Connected to server with client ID {}, entity ID {}",
            client_id,
            entity_id
        );

        self.client_id = Some(client_id);
        self.entity_id = Some(entity_id);
        self.state = ConnectionState::Connected;
        self.connection.state = ConnectionState::Connected;
        self.connection.client_id = client_id;
        self.connection.entity_id = Some(entity_id);
        self.endpoint.set_state(ConnectionState::Connected);

        Ok(())
    }

    fn handle_connection_denied(&mut self, reason: &str) -> io::Result<()> {
        log::warn!("Connection denied: {}", reason);
        self.reset();
        Ok(())
    }
}
