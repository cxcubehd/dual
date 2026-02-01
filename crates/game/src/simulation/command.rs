use std::collections::{HashMap, VecDeque};

use crate::net::ClientCommand;
use crate::physics::{PhysicsSync, PhysicsWorld};
use crate::player::{PlayerConfig, PlayerController, PlayerState};
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
    controller: PlayerController,
    player_states: HashMap<u32, PlayerState>,
    dt: f32,
}

impl Default for CommandProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandProcessor {
    const TICK_RATE: f32 = 1.0 / 60.0;

    pub fn new() -> Self {
        Self {
            controller: PlayerController::default(),
            player_states: HashMap::new(),
            dt: Self::TICK_RATE,
        }
    }

    pub fn with_config(config: PlayerConfig) -> Self {
        Self {
            controller: PlayerController::new(config),
            player_states: HashMap::new(),
            dt: Self::TICK_RATE,
        }
    }

    pub fn config(&self) -> &PlayerConfig {
        self.controller.config()
    }

    pub fn player_state(&self, entity_id: u32) -> Option<&PlayerState> {
        self.player_states.get(&entity_id)
    }

    pub fn player_state_mut(&mut self, entity_id: u32) -> &mut PlayerState {
        self.player_states.entry(entity_id).or_default()
    }

    pub fn process(
        &mut self,
        command: &ClientCommand,
        entity: &mut Entity,
        physics: &mut PhysicsWorld,
    ) {
        let config = self.controller.config();
        PhysicsSync::create_physics_body(
            entity,
            physics,
            config.player_radius,
            config.player_height,
        );

        let state = self.player_states.entry(entity.id).or_default();
        self.controller.process(command, entity, physics, state, self.dt);
    }

    pub fn process_all(
        &mut self,
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

    pub fn remove_player(&mut self, entity_id: u32) {
        self.player_states.remove(&entity_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_buffer_ordering() {
        let mut buffer = CommandBuffer::new(64);

        let cmd1 = ClientCommand::new(5, 1);
        let cmd2 = ClientCommand::new(3, 2);
        let cmd3 = ClientCommand::new(10, 3);

        buffer.push(1, cmd2);
        buffer.push(1, cmd1);
        buffer.push(1, cmd3);

        let drained = buffer.drain_for_tick(5);
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].command.tick, 3);
        assert_eq!(drained[1].command.tick, 5);

        assert_eq!(buffer.len(), 1);
    }
}
