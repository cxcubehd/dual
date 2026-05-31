use super::{MovementInput, PlayerController, PlayerState};

impl PlayerController {
    pub(super) fn tick_crouch(&self, state: &mut PlayerState, input: &MovementInput, dt: f32) {
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

    pub(super) fn tick_strafe_ground_time(&self, state: &mut PlayerState, grounded: bool, dt: f32) {
        if grounded {
            state.strafe_ground_time =
                (state.strafe_ground_time + dt).min(self.config.strafe_ground_time_max);
        } else {
            state.strafe_ground_time = 0.0;
        }
    }

    pub(super) fn tick_stun(&self, state: &mut PlayerState, grounded: bool, dt: f32) {
        let decay_rate = if grounded {
            self.config.stunned_delta_ground_factor
        } else {
            1.0
        };
        state.stunned_duration = (state.stunned_duration - dt * decay_rate).max(0.0);
    }
}
