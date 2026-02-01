use glam::Vec3;

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub strafe_ground_time: f32,
    pub stunned_duration: f32,
    pub crouch_amount: f32,
    pub crouch_target: f32,
    pub last_crouch_amount: f32,
    pub deferred_impulse_set: Option<Vec3>,
    pub deferred_impulse_add: Vec3,
    pub velocity: Vec3,
    pub grounded: bool,
    pub coyote_time: f32,
    pub jump_consumed: bool,
    pub jump_requested: bool,
    pub jump_held: bool,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            strafe_ground_time: 0.0,
            stunned_duration: 0.0,
            crouch_amount: 0.0,
            crouch_target: 0.0,
            last_crouch_amount: 0.0,
            deferred_impulse_set: None,
            deferred_impulse_add: Vec3::ZERO,
            velocity: Vec3::ZERO,
            grounded: true,
            coyote_time: 0.0,
            jump_consumed: false,
            jump_requested: false,
            jump_held: false,
        }
    }
}

impl PlayerState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn queue_impulse_set(&mut self, impulse: Vec3) {
        self.deferred_impulse_set = Some(impulse);
    }

    pub fn queue_impulse_add(&mut self, impulse: Vec3) {
        self.deferred_impulse_add += impulse;
    }

    pub fn has_pending_impulse(&self) -> bool {
        self.deferred_impulse_set.is_some() || self.deferred_impulse_add.length_squared() > 0.0001
    }

    pub fn consume_impulse(&mut self) -> (Option<Vec3>, Vec3) {
        let set = self.deferred_impulse_set.take();
        let add = self.deferred_impulse_add;
        self.deferred_impulse_add = Vec3::ZERO;
        (set, add)
    }

    pub fn apply_stun(&mut self, duration: f32) {
        self.stunned_duration = self.stunned_duration.max(duration);
    }

    pub fn is_stunned(&self) -> bool {
        self.stunned_duration > 0.0
    }
}
