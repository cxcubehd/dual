use std::collections::VecDeque;

use glam::{Quat, Vec3};

use dual::ClientCommand;

const MAX_PENDING_COMMANDS: usize = 128;
const ERROR_CORRECTION_SPEED: f32 = 20.0;
const ERROR_THRESHOLD: f32 = 0.0001;
const SNAP_THRESHOLD: f32 = 1.0;

#[derive(Debug, Clone)]
struct PendingCommand {
    sequence: u32,
    position_after: Vec3,
}

pub struct ClientPrediction {
    pending_commands: VecDeque<PendingCommand>,
    position: Vec3,        // Logical position (current tick)
    prev_position: Vec3,   // Logical position (previous tick)
    visual_position: Vec3, // Interpolated + Smoothed position for rendering
    orientation: Quat,
    position_error: Vec3,
    last_acked_sequence: u32,
}

impl ClientPrediction {
    pub fn new(_tick_rate: u32) -> Self {
        Self {
            pending_commands: VecDeque::with_capacity(MAX_PENDING_COMMANDS),
            position: Vec3::new(0.0, 1.0, 0.0),
            prev_position: Vec3::new(0.0, 1.0, 0.0),
            visual_position: Vec3::new(0.0, 1.0, 0.0),
            orientation: Quat::IDENTITY,
            position_error: Vec3::ZERO,
            last_acked_sequence: 0,
        }
    }

    pub fn prepare_tick(&mut self) {
        self.prev_position = self.position;
    }

    pub fn apply_input(&mut self, command: &ClientCommand, dt: f32) {
        let move_dir = command.decode_move_direction();
        let (yaw, pitch) = command.decode_view_angles();

        let speed = if command.has_flag(ClientCommand::FLAG_SPRINT) {
            10.0
        } else {
            5.0
        };

        let move_vec = Vec3::new(move_dir[0], move_dir[1], move_dir[2]);
        if move_vec.length_squared() > 0.001 {
            let normalized = move_vec.normalize();

            let (sin_yaw, cos_yaw) = yaw.sin_cos();
            let world_move = Vec3::new(
                normalized.x * cos_yaw + normalized.z * sin_yaw,
                normalized.y,
                -normalized.x * sin_yaw + normalized.z * cos_yaw,
            );

            self.position += world_move * speed * dt;
        }

        self.orientation = Quat::from_euler(glam::EulerRot::YXZ, yaw, -pitch, 0.0);
    }

    pub fn update(&mut self, dt: f32) {
        // Exponential decay of error
        let decay = (-ERROR_CORRECTION_SPEED * dt).exp();
        self.position_error *= decay;
    }

    pub fn update_visuals(&mut self, alpha: f32) {
        let interpolated = self.prev_position.lerp(self.position, alpha);
        self.visual_position = interpolated + self.position_error;
    }

    pub fn store_command(&mut self, _command: &ClientCommand, sequence: u32) {
        self.pending_commands.push_back(PendingCommand {
            sequence,
            position_after: self.position,
        });

        while self.pending_commands.len() > MAX_PENDING_COMMANDS {
            self.pending_commands.pop_front();
        }
    }

    pub fn reconcile(
        &mut self,
        server_position: Vec3,
        server_orientation: Quat,
        acked_sequence: u32,
    ) {
        if acked_sequence <= self.last_acked_sequence {
            return;
        }
        self.last_acked_sequence = acked_sequence;

        while self
            .pending_commands
            .front()
            .is_some_and(|cmd| cmd.sequence < acked_sequence)
        {
            self.pending_commands.pop_front();
        }

        let acked_position = if let Some(acked_cmd) = self
            .pending_commands
            .front()
            .filter(|cmd| cmd.sequence == acked_sequence)
        {
            acked_cmd.position_after
        } else {
            return;
        };

        if self
            .pending_commands
            .front()
            .is_some_and(|cmd| cmd.sequence == acked_sequence)
        {
            self.pending_commands.pop_front();
        }

        let server_error = server_position - acked_position;
        let error_magnitude = server_error.length();

        if error_magnitude < ERROR_THRESHOLD {
            return;
        }

        // Apply correction to Logic
        self.position += server_error;
        self.prev_position += server_error; // Shift history to match
        for cmd in &mut self.pending_commands {
            cmd.position_after += server_error;
        }

        if error_magnitude > SNAP_THRESHOLD {
            self.position_error = Vec3::ZERO;
        } else {
            // Smooth correction: Visual should not change instantly.
            // Visual = Logic + Error.
            // NewLogic = OldLogic + Diff.
            // NewVisual = NewLogic + NewError = OldLogic + Diff + NewError.
            // We want NewVisual == OldVisual (OldLogic + OldError).
            // OldLogic + Diff + NewError = OldLogic + OldError.
            // NewError = OldError - Diff.
            self.position_error -= server_error;
        }

        let _ = server_orientation;
    }

    pub fn predicted_position(&self) -> Vec3 {
        self.visual_position
    }

    pub fn predicted_orientation(&self) -> Quat {
        self.orientation
    }

    pub fn reset(&mut self) {
        self.pending_commands.clear();
        self.position = Vec3::new(0.0, 1.0, 0.0);
        self.prev_position = Vec3::new(0.0, 1.0, 0.0);
        self.visual_position = Vec3::new(0.0, 1.0, 0.0);
        self.orientation = Quat::IDENTITY;
        self.position_error = Vec3::ZERO;
        self.last_acked_sequence = 0;
    }

    pub fn pending_command_count(&self) -> usize {
        self.pending_commands.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smoothing() {
        let mut prediction = ClientPrediction::new(60);

        // Initial state
        prediction.prepare_tick();
        prediction.store_command(&ClientCommand::new(0, 0), 1);

        // Logic starts at (0, 1, 0).
        // Server says (0.5, 1, 0). Error 0.5.
        // Reconcile: Logic += 0.5 => 0.5. Error -= 0.5 => -0.5.
        // Visual = Logic + Error = 0.5 - 0.5 = 0.

        prediction.reconcile(Vec3::new(0.5, 1.0, 0.0), Quat::IDENTITY, 1);

        assert_eq!(prediction.position, Vec3::new(0.5, 1.0, 0.0)); // Logic shifted
        assert!((prediction.position_error - Vec3::new(-0.5, 0.0, 0.0)).length() < 0.0001); // Error negative

        // Check visual before update (alpha 0)
        prediction.update_visuals(0.0);
        // Prev was updated to 0.5 in reconcile.
        // Lerp(0.5, 0.5, 0) = 0.5.
        // Visual = 0.5 + (-0.5) = 0.0.
        assert!((prediction.predicted_position() - Vec3::new(0.0, 1.0, 0.0)).length() < 0.0001);

        // Update error decay
        prediction.update(0.05); // Error decays. -0.5 becomes closer to 0 (e.g. -0.18 using speed 20).
        prediction.update_visuals(0.0);

        // Visual moves towards Logic (0.5).
        assert!(prediction.predicted_position().x > 0.1);
        assert!(prediction.predicted_position().x < 0.5);
    }

    #[test]
    fn test_interpolation() {
        let mut prediction = ClientPrediction::new(60);
        // Start at 0.
        prediction.prepare_tick();
        // Move to 1.
        prediction.position = Vec3::new(1.0, 1.0, 0.0);

        // Prev = 0. Curr = 1.

        // Alpha 0.5
        prediction.update_visuals(0.5);
        assert!((prediction.predicted_position().x - 0.5).abs() < 0.0001);
    }
}
