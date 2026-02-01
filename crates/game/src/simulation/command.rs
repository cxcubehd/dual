use std::collections::VecDeque;

use glam::Vec3;

use crate::net::ClientCommand;
use crate::physics::{PhysicsSync, PhysicsWorld};
use crate::snapshot::{Entity, World};

#[derive(Debug, Clone)]
pub struct PendingCommand {
    pub entity_id: u32,
    pub command: ClientCommand,
}

pub struct CommandBuffer {
    commands: VecDeque<PendingCommand>,
    max_size: usize,
}

impl CommandBuffer {
    pub fn new(max_size: usize) -> Self {
        Self {
            commands: VecDeque::with_capacity(max_size),
            max_size,
        }
    }

    pub fn push(&mut self, entity_id: u32, command: ClientCommand) {
        if self.commands.len() >= self.max_size {
            self.commands.pop_front();
        }
        self.commands
            .push_back(PendingCommand { entity_id, command });
    }

    pub fn drain_for_tick(&mut self, tick: u32) -> Vec<PendingCommand> {
        let mut result = Vec::new();
        while let Some(cmd) = self.commands.front() {
            if cmd.command.tick <= tick {
                result.push(self.commands.pop_front().unwrap());
            } else {
                break;
            }
        }
        result
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

pub struct CommandProcessor {
    move_speed: f32,
    sprint_multiplier: f32,
    jump_impulse: f32,
    player_radius: f32,
    player_height: f32,
}

impl Default for CommandProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandProcessor {
    pub fn new() -> Self {
        Self {
            move_speed: 5.0,
            sprint_multiplier: 2.0,
            jump_impulse: 5.0,
            player_radius: 0.3,
            player_height: 1.8,
        }
    }

    pub fn with_speeds(move_speed: f32, sprint_multiplier: f32, jump_impulse: f32) -> Self {
        Self {
            move_speed,
            sprint_multiplier,
            jump_impulse,
            ..Self::new()
        }
    }

    pub fn process(
        &self,
        command: &ClientCommand,
        entity: &mut Entity,
        physics: &mut PhysicsWorld,
    ) {
        PhysicsSync::create_physics_body(entity, physics, self.player_radius, self.player_height);

        let move_dir = command.decode_move_direction();
        let (yaw, pitch) = command.decode_view_angles();

        let speed = if command.has_flag(ClientCommand::FLAG_SPRINT) {
            self.move_speed * self.sprint_multiplier
        } else {
            self.move_speed
        };

        let move_vec = Vec3::new(move_dir[0], 0.0, move_dir[2]);
        let world_move = if move_vec.length_squared() > 0.001 {
            let normalized = move_vec.normalize();
            let (sin_yaw, cos_yaw) = yaw.sin_cos();
            Vec3::new(
                normalized.x * cos_yaw + normalized.z * sin_yaw,
                0.0,
                -normalized.x * sin_yaw + normalized.z * cos_yaw,
            )
        } else {
            Vec3::ZERO
        };

        let wants_jump = command.has_flag(ClientCommand::FLAG_JUMP);

        PhysicsSync::apply_movement(
            entity,
            physics,
            world_move,
            speed,
            wants_jump,
            self.jump_impulse,
        );

        entity.orientation = glam::Quat::from_euler(glam::EulerRot::YXZ, yaw, -pitch, 0.0);
        entity.dirty = true;
    }

    pub fn process_all(
        &self,
        commands: &[PendingCommand],
        world: &mut World,
        physics: &mut PhysicsWorld,
    ) {
        for pending in commands {
            if let Some(entity) = world.get_by_id_mut(pending.entity_id) {
                self.process(&pending.command, entity, physics);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_buffer_ordering() {
        let mut buffer = CommandBuffer::new(64);

        let mut cmd1 = ClientCommand::new(5, 1);
        let mut cmd2 = ClientCommand::new(3, 2);
        let mut cmd3 = ClientCommand::new(10, 3);

        buffer.push(1, cmd2.clone());
        buffer.push(1, cmd1.clone());
        buffer.push(1, cmd3.clone());

        let drained = buffer.drain_for_tick(5);
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].command.tick, 3);
        assert_eq!(drained[1].command.tick, 5);

        assert_eq!(buffer.len(), 1);
    }
}
