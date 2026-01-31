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
    position: Vec3,
    orientation: Quat,
    position_error: Vec3,
    last_acked_sequence: u32,
}

impl ClientPrediction {
    pub fn new(_tick_rate: u32) -> Self {
        Self {
            pending_commands: VecDeque::with_capacity(MAX_PENDING_COMMANDS),
            position: Vec3::new(0.0, 1.0, 0.0),
            orientation: Quat::IDENTITY,
            position_error: Vec3::ZERO,
            last_acked_sequence: 0,
        }
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

        if self.position_error.length_squared() > ERROR_THRESHOLD * ERROR_THRESHOLD {
            let correction_factor = (ERROR_CORRECTION_SPEED * dt).min(1.0);
            let correction = self.position_error * correction_factor;
            self.position += correction;
            self.position_error -= correction;
        } else {
            self.position_error = Vec3::ZERO;
        }
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

        if error_magnitude > SNAP_THRESHOLD {
            self.position += server_error;
            self.position_error = Vec3::ZERO;
            for cmd in &mut self.pending_commands {
                cmd.position_after += server_error;
            }
        } else {
            self.position_error = server_error;
            for cmd in &mut self.pending_commands {
                cmd.position_after += server_error;
            }
        }

        let _ = server_orientation;
    }

    pub fn predicted_position(&self) -> Vec3 {
        self.position
    }

    pub fn predicted_orientation(&self) -> Quat {
        self.orientation
    }

    pub fn reset(&mut self) {
        self.pending_commands.clear();
        self.position = Vec3::new(0.0, 1.0, 0.0);
        self.orientation = Quat::IDENTITY;
        self.position_error = Vec3::ZERO;
        self.last_acked_sequence = 0;
    }

    pub fn pending_command_count(&self) -> usize {
        self.pending_commands.len()
    }
}
