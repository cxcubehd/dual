use glam::Vec3;
use rapier3d::control::{CharacterAutostep, CharacterLength, EffectiveCharacterMovement, KinematicCharacterController};
use rapier3d::prelude::*;

use crate::net::ClientCommand;
use crate::physics::PhysicsWorld;
use crate::snapshot::Entity;

use super::{PlayerConfig, PlayerState};

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
        let mut character_controller = KinematicCharacterController::default();
        character_controller.offset = CharacterLength::Absolute(0.02);
        character_controller.up = Vector::Y;
        character_controller.max_slope_climb_angle = 50_f32.to_radians();
        character_controller.min_slope_slide_angle = 35_f32.to_radians();
        character_controller.snap_to_ground = Some(CharacterLength::Absolute(0.2));
        character_controller.autostep = Some(CharacterAutostep {
            max_height: CharacterLength::Absolute(0.35),
            min_width: CharacterLength::Absolute(0.15),
            include_dynamic_bodies: false,
        });

        Self {
            config,
            character_controller,
        }
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

        let corrected = self.move_character(
            physics,
            handle,
            &character_shape,
            character_pos,
            desired_translation,
            dt,
        );

        state.grounded = corrected.grounded;
        state.velocity = velocity;
        state.velocity.y = if corrected.grounded && velocity.y <= 0.0 {
            0.0
        } else {
            velocity.y + (corrected.translation.y - desired_translation.y) / dt
        };

        let current_pos = character_pos.translation;
        let new_position = current_pos + corrected.translation;
        physics.set_body_position(handle, Vec3::new(new_position.x, new_position.y, new_position.z));

        self.handle_crouch_height_change(physics, handle, state, current_height);
        self.tick_stun(state, grounded, dt);

        state.jump_requested = input.wants_jump;
        state.jump_held = input.jump_held;

        // Update entity with new position and velocity
        if let Some(pos) = physics.body_position(handle) {
            entity.position = pos;
        }
        entity.velocity = state.velocity;
        entity.orientation =
            glam::Quat::from_euler(glam::EulerRot::YXZ, input.yaw, -input.pitch, 0.0);
        entity.dirty = true;
    }

    fn parse_input(&self, command: &ClientCommand) -> MovementInput {
        let move_dir = command.decode_move_direction();
        let (yaw, pitch) = command.decode_view_angles();
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

    fn compute_velocity(
        &self,
        state: &mut PlayerState,
        input: &MovementInput,
        grounded: bool,
        dt: f32,
    ) -> Vec3 {
        let mut velocity = state.velocity;

        if !grounded {
            velocity = self.apply_gravity(velocity, input.jump_held, dt);
        }

        let can_jump = grounded || state.coyote_time > 0.0;
        if input.wants_jump && can_jump && !state.jump_consumed {
            velocity.y = self.config.jump_power;
            state.jump_consumed = true;
            state.coyote_time = 0.0;
        }

        if !input.wants_jump {
            state.jump_consumed = false;
        }

        if grounded {
            state.coyote_time = self.config.coyote_time;
        } else {
            state.coyote_time = (state.coyote_time - dt).max(0.0);
        }

        let horizontal = self.compute_horizontal_velocity(
            Vec3::new(velocity.x, 0.0, velocity.z),
            input,
            grounded,
            state,
            dt,
        );

        Vec3::new(horizontal.x, velocity.y, horizontal.z)
    }

    fn apply_gravity(&self, mut velocity: Vec3, jump_held: bool, dt: f32) -> Vec3 {
        let gravity_acc = self.calculate_gravity_acceleration(velocity.y, jump_held);
        velocity.y = (velocity.y - gravity_acc * dt).max(-self.config.max_fall_speed);
        velocity
    }

    fn calculate_gravity_acceleration(&self, vertical_velocity: f32, jump_held: bool) -> f32 {
        if jump_held && vertical_velocity > 0.0 && vertical_velocity < self.config.jump_power {
            return self.config.gravity_jump_hold;
        }

        let fall_blend = ((vertical_velocity.abs() - self.config.gravity_fall_vel_start)
            / self.config.gravity_fall_vel_span)
            .clamp(0.0, 1.0);

        lerp(self.config.gravity_fall, self.config.gravity, fall_blend)
    }

    fn compute_horizontal_velocity(
        &self,
        initial: Vec3,
        input: &MovementInput,
        grounded: bool,
        state: &PlayerState,
        dt: f32,
    ) -> Vec3 {
        let crouch = state.crouch_amount.clamp(0.0, 1.0);
        let params = self.movement_params(grounded, initial.length(), crouch);
        let target = self.calculate_target_velocity(initial, input, &params, state, dt);
        let strafed = self.apply_strafe(initial, input.world_direction, target, grounded, state, dt);
        self.apply_deceleration(strafed, target, input, grounded, &params, state, dt)
    }

    fn movement_params(&self, grounded: bool, current_speed: f32, crouch: f32) -> MovementParams {
        let (acceleration, mut deceleration, max_speed) = if grounded {
            (
                lerp(self.config.accelerate_ground, self.config.accelerate_crouch_ground, crouch),
                lerp(self.config.decelerate_ground, self.config.decelerate_crouch_ground, crouch),
                lerp(self.config.move_speed_ground, self.config.move_speed_crouch_ground, crouch),
            )
        } else {
            (
                lerp(self.config.accelerate_air, self.config.accelerate_crouch_air, crouch),
                lerp(self.config.decelerate_air, self.config.decelerate_crouch_air, crouch),
                lerp(self.config.move_speed_air, self.config.move_speed_crouch_air, crouch),
            )
        };

        if grounded {
            let slow_decel = lerp(
                self.config.decelerate_ground_slow,
                self.config.decelerate_crouch_ground_slow,
                crouch,
            );
            let speed_blend = ((current_speed - self.config.decelerate_slow_start)
                / self.config.decelerate_slow_span)
                .clamp(0.0, 1.0);
            deceleration = lerp(slow_decel, deceleration, speed_blend);
        }

        MovementParams {
            acceleration,
            deceleration,
            max_speed,
        }
    }

    fn calculate_target_velocity(
        &self,
        initial: Vec3,
        input: &MovementInput,
        params: &MovementParams,
        state: &PlayerState,
        dt: f32,
    ) -> Vec3 {
        if input.is_active && !state.is_stunned() {
            let blend = (params.acceleration * dt).min(1.0);
            initial.lerp(input.world_direction * params.max_speed, blend)
        } else {
            initial
        }
    }

    fn apply_strafe(
        &self,
        initial: Vec3,
        move_dir: Vec3,
        target: Vec3,
        grounded: bool,
        state: &PlayerState,
        dt: f32,
    ) -> Vec3 {
        let air_strafed = self.apply_air_strafe(initial, move_dir, target, dt);

        if grounded {
            self.blend_ground_strafe(air_strafed, target, state)
        } else {
            air_strafed
        }
    }

    fn apply_air_strafe(&self, initial: Vec3, move_dir: Vec3, target: Vec3, dt: f32) -> Vec3 {
        if move_dir.length_squared() < 0.001 {
            return target;
        }

        let initial_speed = initial.length();
        if initial_speed < 0.001 {
            let result = initial + move_dir * self.config.strafe_air_acceleration * dt;
            return if result.length() < target.length() { target } else { result };
        }

        let strafe_accel = self.config.strafe_air_acceleration * dt;
        let strafe_limit = self.config.strafe_air_limit * dt;
        let strafe_velocity = move_dir * strafe_accel;

        let angle = initial.angle_between(strafe_velocity);
        let projected_speed = initial_speed * angle.cos();

        let result = if projected_speed < strafe_limit - strafe_accel {
            initial + strafe_velocity
        } else if projected_speed < strafe_limit {
            initial + strafe_velocity.normalize_or_zero() * (strafe_limit - projected_speed)
        } else {
            initial
        };

        if result.length() < target.length() { target } else { result }
    }

    fn blend_ground_strafe(&self, velocity: Vec3, target: Vec3, state: &PlayerState) -> Vec3 {
        if velocity.length() <= target.length() {
            return velocity;
        }

        let blend = ((state.strafe_ground_time - self.config.strafe_ground_time_start)
            / self.config.strafe_ground_time_span)
            .clamp(0.0, 1.0);

        velocity.lerp(target, blend)
    }

    fn apply_deceleration(
        &self,
        velocity: Vec3,
        target: Vec3,
        input: &MovementInput,
        grounded: bool,
        params: &MovementParams,
        state: &PlayerState,
        dt: f32,
    ) -> Vec3 {
        let preserve = self.should_preserve_momentum(input, grounded, velocity.length(), target.length(), state);
        if preserve && !state.is_stunned() {
            return velocity;
        }

        let speed = velocity.length();
        if speed < 0.0001 {
            return velocity;
        }

        let decel_amount = (params.deceleration * dt).min(speed);
        velocity - velocity.normalize() * decel_amount
    }

    fn should_preserve_momentum(
        &self,
        input: &MovementInput,
        grounded: bool,
        current_speed: f32,
        target_speed: f32,
        state: &PlayerState,
    ) -> bool {
        if input.is_active {
            return true;
        }

        if !grounded {
            return false;
        }

        if current_speed <= target_speed {
            let grace_period = if input.wants_jump {
                self.config.strafe_ground_time_space_hold
            } else {
                self.config.strafe_ground_time_no_input
            };
            return state.strafe_ground_time < grace_period;
        }

        false
    }

    fn move_character(
        &self,
        physics: &mut PhysicsWorld,
        handle: RigidBodyHandle,
        shape: &SharedShape,
        position: Pose,
        desired_translation: Vec3,
        dt: f32,
    ) -> EffectiveCharacterMovement {
        physics.move_character(
            &self.character_controller,
            handle,
            shape,
            position,
            Vector::new(desired_translation.x, desired_translation.y, desired_translation.z),
            dt,
        )
    }

    fn handle_crouch_height_change(
        &self,
        physics: &mut PhysicsWorld,
        handle: RigidBodyHandle,
        state: &PlayerState,
        _current_height: f32,
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

    fn tick_crouch(&self, state: &mut PlayerState, input: &MovementInput, dt: f32) {
        state.last_crouch_amount = state.crouch_amount;
        state.crouch_target = if input.is_crouching { 1.0 } else { 0.0 };

        let (rate, target) = if state.crouch_target > state.crouch_amount {
            (1.0 / self.config.crouch_time_down, state.crouch_target)
        } else {
            (1.0 / self.config.crouch_time_up, state.crouch_target)
        };

        let diff = target - state.crouch_amount;
        let max_change = rate * dt;
        state.crouch_amount += diff.clamp(-max_change, max_change);
    }

    fn tick_strafe_ground_time(&self, state: &mut PlayerState, grounded: bool, dt: f32) {
        if grounded {
            state.strafe_ground_time = (state.strafe_ground_time + dt).min(self.config.strafe_ground_time_max);
        } else {
            state.strafe_ground_time = 0.0;
        }
    }

    fn tick_stun(&self, state: &mut PlayerState, grounded: bool, dt: f32) {
        let decay_rate = if grounded { self.config.stunned_delta_ground_factor } else { 1.0 };
        state.stunned_duration = (state.stunned_duration - dt * decay_rate).max(0.0);
    }
}

struct MovementParams {
    acceleration: f32,
    deceleration: f32,
    max_speed: f32,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::PhysicsWorld;
    use crate::snapshot::Entity;

    #[test]
    fn controller_processes_without_panic() {
        let controller = PlayerController::default();
        let mut physics = PhysicsWorld::new();
        let mut entity = Entity::player(1, Vec3::new(0.0, 1.0, 0.0));

        let handle = physics.add_player(entity.position, 0.3, 1.8);
        entity.physics_handle = Some(handle);

        let mut state = PlayerState::new();
        let command = ClientCommand::new(0, 1);

        controller.process(&command, &mut entity, &mut physics, &mut state, 1.0 / 60.0);

        assert!(entity.dirty);
    }
}
