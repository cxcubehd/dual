use std::collections::VecDeque;

use glam::{Quat, Vec3};

use dual::ClientCommand;

const MAX_PENDING_COMMANDS: usize = 128;

#[derive(Debug, Clone)]
struct PendingCommand {
    sequence: u32,
    command: ClientCommand,
}

#[derive(Debug, Clone)]
pub struct PredictedState {
    pub position: Vec3,
    pub velocity: Vec3,
    pub orientation: Quat,
}

impl Default for PredictedState {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 1.0, 0.0),
            velocity: Vec3::ZERO,
            orientation: Quat::IDENTITY,
        }
    }
}

pub struct ClientPrediction {
    pending_commands: VecDeque<PendingCommand>,
    base_state: PredictedState,
    frame_position: Vec3,
    frame_orientation: Quat,
    last_acked_sequence: u32,
    tick_rate: f32,
}

impl ClientPrediction {
    pub fn new(tick_rate: u32) -> Self {
        Self {
            pending_commands: VecDeque::with_capacity(MAX_PENDING_COMMANDS),
            base_state: PredictedState::default(),
            frame_position: Vec3::new(0.0, 1.0, 0.0),
            frame_orientation: Quat::IDENTITY,
            last_acked_sequence: 0,
            tick_rate: tick_rate as f32,
        }
    }

    pub fn apply_input(&mut self, command: &ClientCommand, dt: f32) {
        apply_movement_dt(
            &mut self.frame_position,
            &mut self.frame_orientation,
            command,
            dt,
        );
    }

    pub fn store_command(&mut self, command: &ClientCommand, sequence: u32) {
        self.base_state.position = self.frame_position;
        self.base_state.orientation = self.frame_orientation;

        self.pending_commands.push_back(PendingCommand {
            sequence,
            command: command.clone(),
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
            .is_some_and(|cmd| cmd.sequence <= acked_sequence)
        {
            self.pending_commands.pop_front();
        }

        let mut replay_position = server_position;
        let mut replay_orientation = server_orientation;
        let dt = 1.0 / self.tick_rate;

        for pending in &self.pending_commands {
            apply_movement_dt(
                &mut replay_position,
                &mut replay_orientation,
                &pending.command,
                dt,
            );
        }

        self.base_state.position = replay_position;
        self.base_state.orientation = replay_orientation;
        self.frame_position = replay_position;
        self.frame_orientation = replay_orientation;
    }

    pub fn predicted_position(&self) -> Vec3 {
        self.frame_position
    }

    pub fn predicted_orientation(&self) -> Quat {
        self.frame_orientation
    }

    pub fn reset(&mut self) {
        self.pending_commands.clear();
        self.base_state = PredictedState::default();
        self.frame_position = Vec3::new(0.0, 1.0, 0.0);
        self.frame_orientation = Quat::IDENTITY;
        self.last_acked_sequence = 0;
    }

    pub fn pending_command_count(&self) -> usize {
        self.pending_commands.len()
    }
}

fn apply_movement_dt(
    position: &mut Vec3,
    orientation: &mut Quat,
    command: &ClientCommand,
    dt: f32,
) {
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

        *position += world_move * speed * dt;
    }

    *orientation = Quat::from_euler(glam::EulerRot::YXZ, yaw, -pitch, 0.0);
}
