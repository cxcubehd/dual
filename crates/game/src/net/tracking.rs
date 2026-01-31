use std::collections::VecDeque;
use std::time::Instant;

use super::protocol::sequence_greater_than;

#[derive(Debug, Clone)]
pub struct PendingPacket {
    pub sequence: u32,
    pub send_time: Instant,
    pub acked: bool,
}

#[derive(Debug)]
pub struct AckTracker {
    pending: VecDeque<PendingPacket>,
    max_pending: usize,
    srtt: f32,
    rtt_var: f32,
}

impl AckTracker {
    pub fn new(max_pending: usize) -> Self {
        Self {
            pending: VecDeque::with_capacity(max_pending),
            max_pending,
            srtt: 100.0,
            rtt_var: 50.0,
        }
    }

    pub fn track_packet(&mut self, sequence: u32) {
        while self.pending.len() >= self.max_pending {
            self.pending.pop_front();
        }

        self.pending.push_back(PendingPacket {
            sequence,
            send_time: Instant::now(),
            acked: false,
        });
    }

    pub fn process_ack(&mut self, ack: u32, ack_bitfield: u32) -> Vec<u32> {
        let mut acked_sequences = Vec::new();
        let mut rtt_samples = Vec::new();
        let now = Instant::now();

        for pending in &mut self.pending {
            if pending.acked {
                continue;
            }

            let is_acked = if pending.sequence == ack {
                true
            } else if sequence_greater_than(ack, pending.sequence) {
                let diff = ack.wrapping_sub(pending.sequence);
                if diff <= 32 {
                    (ack_bitfield & (1 << (diff - 1))) != 0
                } else {
                    false
                }
            } else {
                false
            };

            if is_acked {
                pending.acked = true;
                acked_sequences.push(pending.sequence);

                let rtt = now.duration_since(pending.send_time).as_secs_f32() * 1000.0;
                rtt_samples.push(rtt);
            }
        }

        for rtt in rtt_samples {
            self.update_rtt(rtt);
        }

        while self.pending.front().is_some_and(|p| p.acked) {
            self.pending.pop_front();
        }

        acked_sequences
    }

    fn update_rtt(&mut self, rtt: f32) {
        const ALPHA: f32 = 0.125;
        const BETA: f32 = 0.25;

        let diff = (rtt - self.srtt).abs();
        self.rtt_var = (1.0 - BETA) * self.rtt_var + BETA * diff;
        self.srtt = (1.0 - ALPHA) * self.srtt + ALPHA * rtt;
    }

    pub fn srtt(&self) -> f32 {
        self.srtt
    }

    pub fn rtt_var(&self) -> f32 {
        self.rtt_var
    }

    pub fn unacked_count(&self) -> usize {
        self.pending.iter().filter(|p| !p.acked).count()
    }
}

#[derive(Debug)]
pub struct ReceiveTracker {
    last_received: u32,
    received_bitfield: u32,
    recent_sequences: VecDeque<u32>,
    max_recent: usize,
}

impl Default for ReceiveTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ReceiveTracker {
    pub fn new() -> Self {
        Self {
            last_received: 0,
            received_bitfield: 0,
            recent_sequences: VecDeque::with_capacity(128),
            max_recent: 128,
        }
    }

    pub fn record_received(&mut self, sequence: u32) -> bool {
        if self.recent_sequences.contains(&sequence) {
            return false;
        }

        if self.recent_sequences.len() >= self.max_recent {
            self.recent_sequences.pop_front();
        }
        self.recent_sequences.push_back(sequence);

        if sequence_greater_than(sequence, self.last_received) {
            let diff = sequence.wrapping_sub(self.last_received);
            if diff <= 32 {
                self.received_bitfield = (self.received_bitfield << diff) | 1;
            } else {
                self.received_bitfield = 0;
            }
            self.last_received = sequence;
        } else {
            let diff = self.last_received.wrapping_sub(sequence);
            if diff > 0 && diff <= 32 {
                self.received_bitfield |= 1 << (diff - 1);
            }
        }

        true
    }

    pub fn ack_data(&self) -> (u32, u32) {
        (self.last_received, self.received_bitfield)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_receive_tracker_bitfield() {
        let mut tracker = ReceiveTracker::new();

        tracker.record_received(1);
        tracker.record_received(2);
        tracker.record_received(3);

        let (ack, bitfield) = tracker.ack_data();
        assert_eq!(ack, 3);
        assert_eq!(bitfield & 0b11, 0b11);
    }

    #[test]
    fn test_receive_tracker_out_of_order() {
        let mut tracker = ReceiveTracker::new();

        tracker.record_received(3);
        tracker.record_received(1);
        tracker.record_received(2);

        let (ack, bitfield) = tracker.ack_data();
        assert_eq!(ack, 3);
        assert_eq!(bitfield & 0b11, 0b11);
    }

    #[test]
    fn test_duplicate_detection() {
        let mut tracker = ReceiveTracker::new();

        assert!(tracker.record_received(1));
        assert!(!tracker.record_received(1));
        assert!(tracker.record_received(2));
    }

    #[test]
    fn test_ack_tracker_rtt() {
        let mut tracker = AckTracker::new(32);

        tracker.track_packet(1);
        std::thread::sleep(Duration::from_millis(10));

        tracker.process_ack(1, 0);

        assert!(tracker.srtt() > 0.0);
    }
}
