use glam::Vec3;

use crate::net::ClientCommand;

use super::{MovementInput, PITCH_LIMIT, PlayerController};

impl PlayerController {
    pub(super) fn parse_input(&self, command: &ClientCommand) -> MovementInput {
        let move_dir = command.decode_move_direction();
        let (yaw, pitch) = command.decode_view_angles();
        let pitch = pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);

        let local_input = Vec3::new(move_dir[0], 0.0, move_dir[2]);
        let world_direction = self.local_to_world_direction(local_input, yaw);

        MovementInput {
            world_direction,
            is_active: world_direction.length_squared() > 0.001,
            wants_jump: command.has_flag(ClientCommand::FLAG_JUMP),
            jump_held: command.has_flag(ClientCommand::FLAG_JUMP_HELD),
            is_crouching: command.has_flag(ClientCommand::FLAG_CROUCH),
            yaw,
            pitch,
        }
    }

    fn local_to_world_direction(&self, local: Vec3, yaw: f32) -> Vec3 {
        if local.length_squared() < 0.001 {
            return Vec3::ZERO;
        }

        let normalized = local.normalize();
        let (sin_yaw, cos_yaw) = yaw.sin_cos();

        Vec3::new(
            normalized.x * cos_yaw + normalized.z * sin_yaw,
            0.0,
            -normalized.x * sin_yaw + normalized.z * cos_yaw,
        )
    }
}
