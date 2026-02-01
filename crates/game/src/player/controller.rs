use glam::Vec3;

use crate::net::ClientCommand;
use crate::physics::PhysicsWorld;
use crate::snapshot::Entity;

use super::{PlayerConfig, PlayerState};

struct MovementInput {
    world_direction: Vec3,
    is_active: bool,
    wants_jump: bool,
    jump_just_pressed: bool,
    is_crouching: bool,
    yaw: f32,
    pitch: f32,
}

struct MovementParams {
    acceleration: f32,
    deceleration: f32,
    max_speed: f32,
    crouch_factor: f32,
}

pub struct PlayerController {
    config: PlayerConfig,
}

impl Default for PlayerController {
    fn default() -> Self {
        Self::new(PlayerConfig::default())
    }
}

impl PlayerController {
    pub fn new(config: PlayerConfig) -> Self {
        Self { config }
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

        let grounded = physics.is_grounded(handle, self.config.ground_check_threshold);
        let current_velocity = physics.body_velocity(handle).unwrap_or(Vec3::ZERO);
        let input = self.parse_input(command, state);

        state.crouch_amount = if input.is_crouching { 1.0 } else { 0.0 };
        self.tick_strafe_ground_time(state, grounded, dt);

        let velocity = self.compute_velocity(current_velocity, &input, grounded, state, dt);

        physics.set_body_velocity(handle, velocity);
        self.tick_stun(state, grounded, dt);

        state.last_grounded = grounded;
        state.jump_held_last_frame = input.wants_jump;

        entity.orientation =
            glam::Quat::from_euler(glam::EulerRot::YXZ, input.yaw, -input.pitch, 0.0);
        entity.dirty = true;
    }

    fn parse_input(&self, command: &ClientCommand, state: &PlayerState) -> MovementInput {
        let move_dir = command.decode_move_direction();
        let (yaw, pitch) = command.decode_view_angles();
        let local_input = Vec3::new(move_dir[0], 0.0, move_dir[2]);
        let world_direction = self.local_to_world_direction(local_input, yaw);
        let wants_jump = command.has_flag(ClientCommand::FLAG_JUMP);

        MovementInput {
            world_direction,
            is_active: world_direction.length_squared() > 0.001,
            wants_jump,
            jump_just_pressed: wants_jump && !state.jump_held_last_frame,
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

    fn compute_velocity(
        &self,
        current: Vec3,
        input: &MovementInput,
        grounded: bool,
        state: &PlayerState,
        dt: f32,
    ) -> Vec3 {
        let mut velocity = current;

        if !grounded {
            velocity = self.apply_gravity(velocity, input.wants_jump, dt);
        }

        if input.jump_just_pressed && grounded {
            velocity.y = self.config.jump_power;
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
        let params = self.movement_params(grounded, initial.length(), state.crouch_amount);
        let target = self.calculate_target_velocity(initial, input, &params, state, dt);
        let strafed = self.apply_strafe(initial, input.world_direction, target, grounded, state, dt);
        self.apply_deceleration(strafed, target, input, grounded, &params, state, dt)
    }

    fn movement_params(&self, grounded: bool, current_speed: f32, crouch: f32) -> MovementParams {
        let crouch = crouch.clamp(0.0, 1.0);

        let (acceleration, mut deceleration, max_speed) = if grounded {
            (
                lerp(
                    self.config.accelerate_ground,
                    self.config.accelerate_crouch_ground,
                    crouch,
                ),
                lerp(
                    self.config.decelerate_ground,
                    self.config.decelerate_crouch_ground,
                    crouch,
                ),
                lerp(
                    self.config.move_speed_ground,
                    self.config.move_speed_crouch_ground,
                    crouch,
                ),
            )
        } else {
            (
                lerp(
                    self.config.accelerate_air,
                    self.config.accelerate_crouch_air,
                    crouch,
                ),
                lerp(
                    self.config.decelerate_air,
                    self.config.decelerate_crouch_air,
                    crouch,
                ),
                lerp(
                    self.config.move_speed_air,
                    self.config.move_speed_crouch_air,
                    crouch,
                ),
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
            crouch_factor: crouch,
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

    fn apply_air_strafe(
        &self,
        initial: Vec3,
        move_dir: Vec3,
        target: Vec3,
        dt: f32,
    ) -> Vec3 {
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
            let allowed = strafe_limit - projected_speed;
            initial + strafe_velocity.normalize_or_zero() * allowed
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
        let should_decelerate =
            !self.should_preserve_momentum(input, grounded, velocity.length(), target.length(), state)
                || state.is_stunned();

        if !should_decelerate {
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

    fn tick_strafe_ground_time(&self, state: &mut PlayerState, grounded: bool, dt: f32) {
        if grounded {
            state.strafe_ground_time =
                (state.strafe_ground_time + dt).min(self.config.strafe_ground_time_max);
        } else {
            state.strafe_ground_time = 0.0;
        }
    }

    fn tick_stun(&self, state: &mut PlayerState, grounded: bool, dt: f32) {
        let decay_rate = if grounded {
            self.config.stunned_delta_ground_factor
        } else {
            1.0
        };
        state.stunned_duration = (state.stunned_duration - dt * decay_rate).max(0.0);
    }
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
