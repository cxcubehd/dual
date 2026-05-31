use dual::{PacketLossSimulation, ServerClientInfo};

use super::state::TuiState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketLossField {
    Direction,
    Enabled,
    LossPercent,
    MinLatency,
    MaxLatency,
    Jitter,
}

impl PacketLossField {
    fn all() -> &'static [Self] {
        &[
            Self::Direction,
            Self::Enabled,
            Self::LossPercent,
            Self::MinLatency,
            Self::MaxLatency,
            Self::Jitter,
        ]
    }

    fn next(&self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|f| f == self).unwrap_or(0);
        all[(idx + 1) % all.len()]
    }

    fn prev(&self) -> Self {
        let all = Self::all();
        let idx = all.iter().position(|f| f == self).unwrap_or(0);
        all[(idx + all.len() - 1) % all.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketDirection {
    Outgoing,
    Incoming,
}

#[derive(Debug, Clone)]
pub struct PacketLossPanelState {
    pub client_id: u32,
    pub selected_field: PacketLossField,
    pub sim: PacketLossSimulation,
    pub incoming_sim: PacketLossSimulation,
    pub direction: PacketDirection,
}

impl TuiState {
    pub fn is_packet_loss_panel_open(&self) -> bool {
        self.packet_loss_panel.is_some()
    }

    pub fn open_packet_loss_panel(&mut self, clients: &[ServerClientInfo]) {
        if let Some(client) = clients.get(self.selected_connection) {
            self.packet_loss_panel = Some(PacketLossPanelState {
                client_id: client.client_id,
                selected_field: PacketLossField::Direction,
                sim: client.packet_loss_sim.clone(),
                incoming_sim: client.incoming_packet_loss_sim.clone(),
                direction: PacketDirection::Outgoing,
            });
        }
    }

    pub fn close_packet_loss_panel(&mut self) {
        if let Some(panel) = self.packet_loss_panel.take() {
            self.pending_packet_loss_update =
                Some((panel.client_id, panel.sim, panel.incoming_sim));
        }
    }

    pub fn cancel_packet_loss_panel(&mut self) {
        self.packet_loss_panel = None;
    }

    pub fn packet_loss_panel_next_field(&mut self) {
        if let Some(panel) = &mut self.packet_loss_panel {
            panel.selected_field = panel.selected_field.next();
        }
    }

    pub fn packet_loss_panel_prev_field(&mut self) {
        if let Some(panel) = &mut self.packet_loss_panel {
            panel.selected_field = panel.selected_field.prev();
        }
    }

    pub fn packet_loss_panel_adjust(&mut self, delta: i32) {
        if let Some(panel) = &mut self.packet_loss_panel {
            if panel.selected_field == PacketLossField::Direction {
                if delta != 0 {
                    panel.direction = match panel.direction {
                        PacketDirection::Outgoing => PacketDirection::Incoming,
                        PacketDirection::Incoming => PacketDirection::Outgoing,
                    };
                }
                return;
            }

            let sim = match panel.direction {
                PacketDirection::Outgoing => &mut panel.sim,
                PacketDirection::Incoming => &mut panel.incoming_sim,
            };

            match panel.selected_field {
                PacketLossField::Direction => unreachable!(),
                PacketLossField::Enabled => {
                    sim.enabled = !sim.enabled;
                }
                PacketLossField::LossPercent => {
                    let new_val = sim.loss_percent + delta as f32;
                    sim.loss_percent = new_val.clamp(0.0, 100.0);
                }
                PacketLossField::MinLatency => {
                    let new_val = sim.min_latency_ms as i32 + delta * 5;
                    sim.min_latency_ms = new_val.clamp(0, 5000) as u32;
                    if sim.min_latency_ms > sim.max_latency_ms {
                        sim.max_latency_ms = sim.min_latency_ms;
                    }
                }
                PacketLossField::MaxLatency => {
                    let new_val = sim.max_latency_ms as i32 + delta * 5;
                    sim.max_latency_ms = new_val.clamp(0, 5000) as u32;
                    if sim.max_latency_ms < sim.min_latency_ms {
                        sim.min_latency_ms = sim.max_latency_ms;
                    }
                }
                PacketLossField::Jitter => {
                    let new_val = sim.jitter_ms as i32 + delta * 5;
                    sim.jitter_ms = new_val.clamp(0, 1000) as u32;
                }
            }
        }
    }

    pub fn take_pending_packet_loss_update(
        &mut self,
    ) -> Option<(u32, PacketLossSimulation, PacketLossSimulation)> {
        self.pending_packet_loss_update.take()
    }

    pub fn packet_loss_panel(&self) -> Option<&PacketLossPanelState> {
        self.packet_loss_panel.as_ref()
    }
}
