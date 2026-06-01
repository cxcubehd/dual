use glam::Vec3;
use rapier3d::control::{EffectiveCharacterMovement, KinematicCharacterController};
use rapier3d::prelude::*;

use crate::physics::PhysicsWorld;

use super::{PlayerController, PlayerState};

impl PlayerController {
    pub(super) fn move_character(
        &self,
        physics: &mut PhysicsWorld,
        controller: &KinematicCharacterController,
        handle: RigidBodyHandle,
        shape: &SharedShape,
        position: Pose,
        desired_translation: Vec3,
        dt: f32,
    ) -> EffectiveCharacterMovement {
        physics.move_character(
            controller,
            handle,
            shape,
            position,
            Vector::new(
                desired_translation.x,
                desired_translation.y,
                desired_translation.z,
            ),
            dt,
        )
    }

    pub(super) fn resolve_horizontal_velocity(
        &self,
        velocity: Vec3,
        desired: Vec3,
        corrected: Vec3,
    ) -> Vec3 {
        let desired_length = desired.length();
        if desired_length < 0.0001 {
            return velocity;
        }

        let corrected_length = corrected.length();
        if corrected_length < 0.0001 {
            return Vec3::ZERO;
        }

        let desired_dir = desired / desired_length;
        let corrected_dir = corrected / corrected_length;
        if corrected_dir.dot(desired_dir) > 0.95 {
            let projected_speed = velocity.dot(desired_dir);
            return desired_dir * projected_speed.max(0.0);
        }

        let projected_speed = velocity.dot(corrected_dir);

        corrected_dir * projected_speed.max(0.0)
    }

    pub(super) fn resolve_vertical_velocity(
        &self,
        velocity: f32,
        desired: f32,
        corrected: f32,
        grounded: bool,
    ) -> f32 {
        if grounded && velocity <= 0.0 {
            return 0.0;
        }

        if velocity > 0.0 {
            let blocked_upward = corrected < desired - 0.0001;
            if blocked_upward {
                return 0.0;
            }
            return velocity;
        }

        if corrected > desired + 0.0001 {
            return 0.0;
        }

        velocity
    }

    pub(super) fn handle_crouch_height_change(
        &self,
        physics: &mut PhysicsWorld,
        handle: RigidBodyHandle,
        state: &PlayerState,
    ) {
        if (state.crouch_amount - state.last_crouch_amount).abs() > 0.001 {
            let height_diff = (state.last_crouch_amount - state.crouch_amount)
                * self.config.player_height
                * (1.0 - self.config.crouch_height_factor)
                / 2.0;

            if let Some(pos) = physics.body_position(handle) {
                physics.set_body_position(handle, Vec3::new(pos.x, pos.y + height_diff, pos.z));
            }
        }
    }
}
