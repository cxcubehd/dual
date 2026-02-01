pub struct PlayerConfig {
    pub move_speed_ground: f32,
    pub move_speed_air: f32,

    pub accelerate_ground: f32,
    pub decelerate_ground: f32,

    pub accelerate_air: f32,
    pub decelerate_air: f32,

    pub decelerate_ground_slow: f32,
    pub decelerate_slow_start: f32,
    pub decelerate_slow_span: f32,

    pub move_speed_crouch_ground: f32,
    pub move_speed_crouch_air: f32,

    pub accelerate_crouch_ground: f32,
    pub decelerate_crouch_ground: f32,

    pub accelerate_crouch_air: f32,
    pub decelerate_crouch_air: f32,

    pub decelerate_crouch_ground_slow: f32,

    pub crouch_time_down: f32,
    pub crouch_time_up: f32,
    pub crouch_height_factor: f32,

    pub strafe_air_acceleration: f32,
    pub strafe_air_limit: f32,

    pub strafe_ground_time_start: f32,
    pub strafe_ground_time_span: f32,
    pub strafe_ground_time_space_hold: f32,
    pub strafe_ground_time_no_input: f32,
    pub strafe_ground_time_max: f32,

    pub gravity: f32,
    pub gravity_fall: f32,
    pub gravity_fall_vel_start: f32,
    pub gravity_fall_vel_span: f32,
    pub gravity_jump_hold: f32,

    pub max_fall_speed: f32,

    pub jump_power: f32,

    pub stunned_delta_ground_factor: f32,

    pub player_radius: f32,
    pub player_height: f32,
    pub ground_check_threshold: f32,

    pub coyote_time: f32,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            move_speed_ground: 9.0,
            move_speed_air: 7.0,

            accelerate_ground: 7.0,
            decelerate_ground: 25.0,

            accelerate_air: 2.0,
            decelerate_air: 0.0,

            decelerate_ground_slow: 0.2,
            decelerate_slow_start: 0.01,
            decelerate_slow_span: 3.0,

            move_speed_crouch_ground: 4.0,
            move_speed_crouch_air: 3.0,

            accelerate_crouch_ground: 5.0,
            decelerate_crouch_ground: 30.0,

            accelerate_crouch_air: 1.0,
            decelerate_crouch_air: 0.0,

            decelerate_crouch_ground_slow: 3.0,

            crouch_time_down: 0.15,
            crouch_time_up: 0.2,
            crouch_height_factor: 0.65,

            strafe_air_acceleration: 200.0,
            strafe_air_limit: 30.0,

            strafe_ground_time_start: 0.14,
            strafe_ground_time_span: 0.08,
            strafe_ground_time_space_hold: 0.078,
            strafe_ground_time_no_input: 0.055,
            strafe_ground_time_max: 5.0,

            gravity: 9.8,
            gravity_fall: 16.0,
            gravity_fall_vel_start: 8.0,
            gravity_fall_vel_span: 9.0,
            gravity_jump_hold: 11.0,

            max_fall_speed: 70.0,

            jump_power: 6.0,

            stunned_delta_ground_factor: 2.5,

            player_radius: 0.3,
            player_height: 1.8,
            ground_check_threshold: 1.0,

            coyote_time: 0.05,
        }
    }
}
