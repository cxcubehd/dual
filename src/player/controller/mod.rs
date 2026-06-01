mod collision;
mod input;
mod movement;
mod timers;

#[cfg(test)]
mod tests;

use glam::Vec3;
use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::prelude::*;

use crate::net::ClientCommand;
use crate::physics::PhysicsWorld;
use crate::snapshot::Entity;

use super::{PlayerConfig, PlayerState};

const PITCH_LIMIT: f32 = 89.9999_f32.to_radians();
const CHARACTER_CONTROLLER_OFFSET: f32 = 0.02;
const GROUND_SNAP_DISTANCE: f32 = 0.25;
const MIN_GROUND_SNAP_PROBE: f32 = 0.001;

struct MovementInput {
    world_direction: Vec3,
    is_active: bool,
    wants_jump: bool,
    jump_held: bool,
    is_crouching: bool,
    yaw: f32,
    pitch: f32,
}

pub struct PlayerController {
    config: PlayerConfig,
    character_controller: KinematicCharacterController,
}

impl Default for PlayerController {
    fn default() -> Self {
        Self::new(PlayerConfig::default())
    }
}

impl PlayerController {
    pub fn new(config: PlayerConfig) -> Self {
        let character_controller = Self::create_base_character_controller();

        Self {
            config,
            character_controller,
        }
    }

    fn create_base_character_controller() -> KinematicCharacterController {
        let mut controller = KinematicCharacterController::default();
        controller.offset = CharacterLength::Absolute(CHARACTER_CONTROLLER_OFFSET);
        controller.up = Vector::Y;
        controller.max_slope_climb_angle = 50_f32.to_radians();
        controller.min_slope_slide_angle = 40_f32.to_radians();
        controller.snap_to_ground = Some(CharacterLength::Absolute(GROUND_SNAP_DISTANCE));
        controller.autostep = Some(CharacterAutostep {
            max_height: CharacterLength::Absolute(0.38),
            min_width: CharacterLength::Absolute(0.08),
            include_dynamic_bodies: false,
        });
        controller
    }

    pub fn config(&self) -> &PlayerConfig {
        &self.config
    }

    pub fn process(
        &self,
        command: &ClientCommand,
        entity: &mut Entity,
        physics: &mut PhysicsWorld,
        state: &mut PlayerState,
        dt: f32,
    ) {
        let Some(handle) = entity.physics_handle else {
            return;
        };

        let input = self.parse_input(command);
        self.tick_crouch(state, &input, dt);

        let current_height = self.current_player_height(state);
        let character_shape = self.create_character_shape(current_height);
        let character_pos = self.get_character_position(physics, handle);

        let grounded = state.grounded;
        self.tick_strafe_ground_time(state, grounded, dt);

        let velocity = self.compute_velocity(state, &input, grounded, dt);
        let desired_translation = velocity * dt;

        let mut character_controller = self.character_controller.clone();

        if velocity.y > 0.0 {
            character_controller.snap_to_ground = None;
            character_controller.autostep = None;
        }

        let mut corrected = self.move_character(
            physics,
            &character_controller,
            handle,
            &character_shape,
            character_pos,
            desired_translation,
            dt,
        );

        if grounded && velocity.y <= 0.0 && corrected.translation.y <= 0.0 {
            let horizontal_distance =
                Vec3::new(corrected.translation.x, 0.0, corrected.translation.z).length();
            let slope_probe =
                horizontal_distance * self.character_controller.max_slope_climb_angle.tan();
            let snap_probe = slope_probe.clamp(MIN_GROUND_SNAP_PROBE, GROUND_SNAP_DISTANCE);
            let position_after_move = character_pos.translation + corrected.translation;
            let ground_clearance = current_height * 0.5 + CHARACTER_CONTROLLER_OFFSET;
            let max_ray_distance = ground_clearance + snap_probe;

            if let Some((_, distance)) = physics.raycast_excluding_body(
                handle,
                Vec3::new(
                    position_after_move.x,
                    position_after_move.y,
                    position_after_move.z,
                ),
                Vec3::NEG_Y,
                max_ray_distance,
            ) {
                let snap_distance = distance - ground_clearance;
                if snap_distance > 0.0 && snap_distance <= snap_probe {
                    corrected.translation.y -= snap_distance;
                }
                corrected.grounded = true;
            }
        }

        state.grounded = corrected.grounded;

        let horizontal_velocity = self.resolve_horizontal_velocity(
            Vec3::new(velocity.x, 0.0, velocity.z),
            Vec3::new(desired_translation.x, 0.0, desired_translation.z),
            Vec3::new(corrected.translation.x, 0.0, corrected.translation.z),
        );

        let vertical_velocity = self.resolve_vertical_velocity(
            velocity.y,
            desired_translation.y,
            corrected.translation.y,
            corrected.grounded,
        );

        state.velocity = Vec3::new(
            horizontal_velocity.x,
            vertical_velocity,
            horizontal_velocity.z,
        );

        let current_pos = character_pos.translation;
        let new_position = current_pos + corrected.translation;
        physics.set_body_position(
            handle,
            Vec3::new(new_position.x, new_position.y, new_position.z),
        );

        self.handle_crouch_height_change(physics, handle, state);
        self.tick_stun(state, grounded, dt);

        state.jump_requested = input.wants_jump;
        state.jump_held = input.jump_held;

        if let Some(pos) = physics.body_position(handle) {
            entity.position = pos;
        }
        entity.velocity = state.velocity;
        entity.orientation =
            glam::Quat::from_euler(glam::EulerRot::YXZ, input.yaw, -input.pitch, 0.0);
        entity.dirty = true;
    }

    fn create_character_shape(&self, height: f32) -> SharedShape {
        let half_height = height / 2.0;
        SharedShape::cylinder(half_height, self.config.player_radius)
    }

    fn current_player_height(&self, state: &PlayerState) -> f32 {
        let standing = self.config.player_height;
        let crouched = standing * self.config.crouch_height_factor;
        lerp(standing, crouched, state.crouch_amount)
    }

    fn get_character_position(&self, physics: &PhysicsWorld, handle: RigidBodyHandle) -> Pose {
        physics
            .body(handle)
            .map(|b| *b.position())
            .unwrap_or(Pose::IDENTITY)
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
