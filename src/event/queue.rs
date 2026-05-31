use std::collections::VecDeque;

use super::types::{GameEvent, ReliabilityMode};

#[derive(Debug, Clone)]
pub struct PendingEvent {
    pub tick: u32,
    pub timestamp_ms: u64,
    pub event: GameEvent,
    pub sequence: u32,
    pub acked: bool,
}

impl PendingEvent {
    pub fn is_expired(&self, current_time_ms: u64) -> bool {
        match self.event.reliability() {
            ReliabilityMode::UnreliableExpiring { ttl_ms } => {
                current_time_ms.saturating_sub(self.timestamp_ms) > ttl_ms
            }
            ReliabilityMode::Unreliable => true,
            ReliabilityMode::Reliable => false,
        }
    }
}

pub struct EventQueue {
    pending: VecDeque<PendingEvent>,
    next_sequence: u32,
    max_pending: usize,
}

impl EventQueue {
    pub fn new(max_pending: usize) -> Self {
        Self {
            pending: VecDeque::with_capacity(max_pending),
            next_sequence: 0,
            max_pending,
        }
    }

    pub fn push(&mut self, tick: u32, timestamp_ms: u64, event: GameEvent) -> u32 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);

        if self.pending.len() >= self.max_pending {
            self.evict_oldest_unreliable();
        }

        self.pending.push_back(PendingEvent {
            tick,
            timestamp_ms,
            event,
            sequence,
            acked: false,
        });

        sequence
    }

    pub fn ack(&mut self, sequence: u32) {
        for event in &mut self.pending {
            if event.sequence == sequence {
                event.acked = true;
                break;
            }
        }
    }

    pub fn ack_up_to(&mut self, sequence: u32) {
        for event in &mut self.pending {
            if sequence_lte(event.sequence, sequence) {
                event.acked = true;
            }
        }
    }

    pub fn cleanup(&mut self, current_time_ms: u64) {
        self.pending.retain(|e| {
            if e.acked {
                return false;
            }
            !e.is_expired(current_time_ms)
        });
    }

    pub fn pending_for_send(&self) -> impl Iterator<Item = &PendingEvent> {
        self.pending.iter().filter(|e| !e.acked)
    }

    pub fn reliable_pending(&self) -> impl Iterator<Item = &PendingEvent> {
        self.pending
            .iter()
            .filter(|e| !e.acked && e.event.reliability().is_reliable())
    }

    pub fn drain_events_for_tick(&mut self, tick: u32) -> Vec<GameEvent> {
        let mut result = Vec::new();
        let mut i = 0;
        while i < self.pending.len() {
            if self.pending[i].tick == tick {
                result.push(self.pending.remove(i).unwrap().event);
            } else {
                i += 1;
            }
        }
        result
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }

    fn evict_oldest_unreliable(&mut self) {
        if let Some(idx) = self
            .pending
            .iter()
            .position(|e| !e.event.reliability().is_reliable())
        {
            self.pending.remove(idx);
        }
    }
}

fn sequence_lte(a: u32, b: u32) -> bool {
    let diff = b.wrapping_sub(a);
    diff < u32::MAX / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_expiration() {
        let event = PendingEvent {
            tick: 0,
            timestamp_ms: 1000,
            event: GameEvent::PlayerKill {
                killer_id: 1,
                victim_id: 2,
                weapon_id: 0,
            },
            sequence: 0,
            acked: false,
        };

        assert!(!event.is_expired(5000));
        assert!(event.is_expired(15000));
    }

    #[test]
    fn reliable_never_expires() {
        let event = PendingEvent {
            tick: 0,
            timestamp_ms: 0,
            event: GameEvent::ChatMessage {
                sender_id: 1,
                channel: 0,
                message: "test".to_string(),
            },
            sequence: 0,
            acked: false,
        };

        assert!(!event.is_expired(1_000_000));
    }

    #[test]
    fn queue_ack_cleanup() {
        let mut queue = EventQueue::new(64);

        queue.push(0, 0, GameEvent::PlayerDeath { player_id: 1 });
        queue.push(
            0,
            0,
            GameEvent::ChatMessage {
                sender_id: 1,
                channel: 0,
                message: "test".to_string(),
            },
        );

        assert_eq!(queue.len(), 2);

        queue.ack(0);
        queue.cleanup(0);

        assert_eq!(queue.len(), 1);
    }
}
