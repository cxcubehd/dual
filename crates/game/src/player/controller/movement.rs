use glam::Vec3;

use super::{MovementInput, PlayerController, PlayerState, lerp};

impl PlayerController {
    pub(super) fn compute_velocity(
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
        let strafed =
            self.apply_strafe(initial, input.world_direction, target, grounded, state, dt);
        self.apply_deceleration(strafed, target, input, grounded, &params, state, dt)
    }

    fn movement_params(&self, grounded: bool, current_speed: f32, crouch: f32) -> MovementParams {
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
            return if result.length() < target.length() {
                target
            } else {
                result
            };
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

        if result.length() < target.length() {
            target
        } else {
            result
        }
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
        let preserve = self.should_preserve_momentum(
            input,
            grounded,
            velocity.length(),
            target.length(),
            state,
        );
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
}

struct MovementParams {
    acceleration: f32,
    deceleration: f32,
    max_speed: f32,
}
