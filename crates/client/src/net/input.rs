use dual::ClientCommand;

#[derive(Debug, Clone, Default)]
pub struct InputState {
    pub move_direction: [f32; 3],
    pub view_yaw: f32,
    pub view_pitch: f32,
    pub sprint: bool,
    pub jump: bool,
    pub jump_held: bool,
    pub crouch: bool,
    pub fire1: bool,
    pub fire2: bool,
    pub use_key: bool,
    pub reload: bool,
}

impl InputState {
    pub fn to_command(&self, tick: u32, sequence: u32) -> ClientCommand {
        let mut cmd = ClientCommand::new(tick, sequence);
        cmd.encode_move_direction(self.move_direction);
        cmd.encode_view_angles(self.view_yaw, self.view_pitch);

        if self.sprint {
            cmd.set_flag(ClientCommand::FLAG_SPRINT, true);
        }
        if self.jump {
            cmd.set_flag(ClientCommand::FLAG_JUMP, true);
        }
        if self.jump_held {
            cmd.set_flag(ClientCommand::FLAG_JUMP_HELD, true);
        }
        if self.crouch {
            cmd.set_flag(ClientCommand::FLAG_CROUCH, true);
        }
        if self.fire1 {
            cmd.set_flag(ClientCommand::FLAG_FIRE1, true);
        }
        if self.fire2 {
            cmd.set_flag(ClientCommand::FLAG_FIRE2, true);
        }
        if self.use_key {
            cmd.set_flag(ClientCommand::FLAG_USE, true);
        }
        if self.reload {
            cmd.set_flag(ClientCommand::FLAG_RELOAD, true);
        }

        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_to_command() {
        let input = InputState {
            move_direction: [1.0, 0.0, 0.0],
            view_yaw: std::f32::consts::FRAC_PI_4,
            view_pitch: 0.0,
            sprint: true,
            jump: false,
            jump_held: false,
            crouch: false,
            fire1: true,
            fire2: false,
            use_key: false,
            reload: false,
        };

        let command = input.to_command(10, 1);

        assert_eq!(command.tick, 10);
        assert_eq!(command.command_sequence, 1);
        assert!(command.has_flag(ClientCommand::FLAG_SPRINT));
        assert!(command.has_flag(ClientCommand::FLAG_FIRE1));
        assert!(!command.has_flag(ClientCommand::FLAG_JUMP));

        let decoded = command.decode_move_direction();
        assert!((decoded[0] - 1.0).abs() < 0.01);
    }
}
