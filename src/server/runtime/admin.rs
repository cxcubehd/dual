use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::{
    ConnectionState, EntityHandle, PacketLossSimulation, PacketType, Reliability, ServerClientInfo,
    ServerStats,
};

use super::GameServer;
use crate::server::{DisconnectReason, ServerEvent};

impl GameServer {
    pub fn run(&mut self) {
        while self.running.load(Ordering::SeqCst) {
            self.tick_once();
            std::thread::sleep(Duration::from_millis(1));
        }
        self.shutdown_connections();
    }

    pub fn shutdown_connections(&mut self) {
        let client_ids: Vec<u32> = self.connections.iter().map(|c| c.client_id).collect();
        for client_id in client_ids {
            self.kick_client(client_id);
        }
    }

    pub fn kick_client(&mut self, client_id: u32) {
        if let Some(client) = self.connections.get_mut(client_id) {
            let addr = client.addr;
            let packet = client.send_packet(PacketType::Disconnect, Reliability::Reliable);
            let _ = self.endpoint.send_to(&packet, addr);
        }

        if let Some(client) = self.connections.remove(client_id) {
            if let Some(entity_id) = client.entity_id {
                self.world.despawn(EntityHandle(entity_id));
            }
            self.pending_events
                .push_back(ServerEvent::ClientDisconnected {
                    client_id,
                    reason: DisconnectReason::Kicked,
                });
        }
    }

    pub fn stats(&self) -> ServerStats {
        ServerStats {
            tick: self.tick,
            client_count: self.connections.connected_count(),
            max_clients: self.config.max_clients,
            entity_count: self.world.entity_count(),
            network_stats: self.endpoint.stats().clone(),
        }
    }

    pub fn client_infos(&self) -> Vec<ServerClientInfo> {
        self.connections
            .iter()
            .filter(|c| c.state == ConnectionState::Connected)
            .map(|c| ServerClientInfo {
                client_id: c.client_id,
                addr: c.addr.to_string(),
                entity_id: c.entity_id,
                connected_secs: c.last_receive_time.elapsed().as_secs(),
                last_ping_ms: self.endpoint.stats().rtt_ms,
                packet_loss_sim: c.packet_loss_sim.clone(),
                incoming_packet_loss_sim: c.incoming_packet_loss_sim.clone(),
            })
            .collect()
    }

    pub fn set_packet_loss_sim(
        &mut self,
        client_id: u32,
        sim: PacketLossSimulation,
        incoming_sim: PacketLossSimulation,
    ) {
        if let Some(client) = self.connections.get_mut(client_id) {
            client.packet_loss_sim = sim;
            client.incoming_packet_loss_sim = incoming_sim;
        }
    }
}
