use std::collections::VecDeque;

use glam::{Quat, Vec3};

use dual::{
    ClientCommand, Entity, EntityType, PhysicsHandle, PhysicsWorld, PlayerConfig, PlayerController,
    PlayerState, TestingGround,
};

const MAX_PENDING_COMMANDS: usize = 128;
const ERROR_CORRECTION_SPEED: f32 = 20.0;
const ERROR_THRESHOLD: f32 = 0.0001;
const SNAP_THRESHOLD: f32 = 1.0;

#[derive(Debug, Clone)]
struct PendingCommand {
    sequence: u32,
    #[allow(dead_code)]
    command: ClientCommand,
    position_after: Vec3,
}

pub struct ClientPrediction {
    pending_commands: VecDeque<PendingCommand>,
    position: Vec3,        // Logical position (current tick)
    prev_position: Vec3,   // Logical position (previous tick)
    visual_position: Vec3, // Interpolated + Smoothed position for rendering
    orientation: Quat,
    position_error: Vec3,
    last_acked_sequence: u32,
    // Physics-based prediction
    physics: PhysicsWorld,
    controller: PlayerController,
    player_state: PlayerState,
    player_handle: Option<PhysicsHandle>,
    dt: f32,
}

impl ClientPrediction {
    pub fn new(tick_rate: u32) -> Self {
        let dt = 1.0 / tick_rate as f32;
        let mut physics = PhysicsWorld::new();

        // Load the same testing ground geometry as the server
        TestingGround::spawn_physics_only(&mut physics);

        Self {
            pending_commands: VecDeque::with_capacity(MAX_PENDING_COMMANDS),
            position: Vec3::new(0.0, 2.0, 0.0),
            prev_position: Vec3::new(0.0, 2.0, 0.0),
            visual_position: Vec3::new(0.0, 2.0, 0.0),
            orientation: Quat::IDENTITY,
            position_error: Vec3::ZERO,
            last_acked_sequence: 0,
            physics,
            controller: PlayerController::new(PlayerConfig::default()),
            player_state: PlayerState::default(),
            player_handle: None,
            dt,
        }
    }

    fn ensure_player_body(&mut self) {
        if self.player_handle.is_none() {
            let config = self.controller.config();
            let handle =
                self.physics
                    .add_player(self.position, config.player_radius, config.player_height);
            self.player_handle = Some(handle);
        }
    }

    pub fn prepare_tick(&mut self) {
        self.prev_position = self.position;
    }

    pub fn apply_input(&mut self, command: &ClientCommand, _dt: f32) {
        self.ensure_player_body();

        let (yaw, pitch) = command.decode_view_angles();

        // Create a temporary entity for physics processing
        let mut entity = Entity {
            id: 0,
            entity_type: EntityType::Player,
            position: self.position,
            velocity: Vec3::ZERO,
            orientation: self.orientation,
            physics_handle: self.player_handle,
            animation_state: 0,
            animation_time: 0.0,
            flags: 0,
            dirty: false,
        };

        // Sync entity position to physics
        if let Some(handle) = self.player_handle {
            self.physics.set_body_position(handle, self.position);
        }

        // Process movement through physics
        self.controller.process(
            command,
            &mut entity,
            &mut self.physics,
            &mut self.player_state,
            self.dt,
        );

        // Step physics
        self.physics.step();

        // Read back position from physics
        if let Some(handle) = self.player_handle {
            if let Some(pos) = self.physics.body_position(handle) {
                self.position = pos;
            }
        }

        self.orientation = Quat::from_euler(glam::EulerRot::YXZ, yaw, -pitch, 0.0);
    }

    pub fn update(&mut self, dt: f32) {
        // Exponential decay of error
        let decay = (-ERROR_CORRECTION_SPEED * dt).exp();
        self.position_error *= decay;
    }

    pub fn update_visuals(&mut self, alpha: f32) {
        let interpolated = self.prev_position.lerp(self.position, alpha);
        self.visual_position = interpolated + self.position_error;
    }

    pub fn store_command(&mut self, command: &ClientCommand, sequence: u32) {
        self.pending_commands.push_back(PendingCommand {
            sequence,
            command: command.clone(),
            position_after: self.position,
        });

        while self.pending_commands.len() > MAX_PENDING_COMMANDS {
            self.pending_commands.pop_front();
        }
    }

    pub fn reconcile(
        &mut self,
        server_position: Vec3,
        server_orientation: Quat,
        acked_sequence: u32,
    ) {
        if acked_sequence <= self.last_acked_sequence {
            return;
        }
        self.last_acked_sequence = acked_sequence;

        while self
            .pending_commands
            .front()
            .is_some_and(|cmd| cmd.sequence < acked_sequence)
        {
            self.pending_commands.pop_front();
        }

        let acked_position = if let Some(acked_cmd) = self
            .pending_commands
            .front()
            .filter(|cmd| cmd.sequence == acked_sequence)
        {
            acked_cmd.position_after
        } else {
            return;
        };

        if self
            .pending_commands
            .front()
            .is_some_and(|cmd| cmd.sequence == acked_sequence)
        {
            self.pending_commands.pop_front();
        }

        let server_error = server_position - acked_position;
        let error_magnitude = server_error.length();

        if error_magnitude < ERROR_THRESHOLD {
            return;
        }

        // Apply correction to Logic
        self.position += server_error;
        self.prev_position += server_error; // Shift history to match
        for cmd in &mut self.pending_commands {
            cmd.position_after += server_error;
        }

        if error_magnitude > SNAP_THRESHOLD {
            self.position_error = Vec3::ZERO;
        } else {
            // Smooth correction: Visual should not change instantly.
            // Visual = Logic + Error.
            // NewLogic = OldLogic + Diff.
            // NewVisual = NewLogic + NewError = OldLogic + Diff + NewError.
            // We want NewVisual == OldVisual (OldLogic + OldError).
            // OldLogic + Diff + NewError = OldLogic + OldError.
            // NewError = OldError - Diff.
            self.position_error -= server_error;
        }

        let _ = server_orientation;
    }

    pub fn predicted_position(&self) -> Vec3 {
        self.visual_position
    }

    pub fn predicted_orientation(&self) -> Quat {
        self.orientation
    }

    pub fn reset(&mut self) {
        self.pending_commands.clear();
        self.position = Vec3::new(0.0, 2.0, 0.0);
        self.prev_position = Vec3::new(0.0, 2.0, 0.0);
        self.visual_position = Vec3::new(0.0, 2.0, 0.0);
        self.orientation = Quat::IDENTITY;
        self.position_error = Vec3::ZERO;
        self.last_acked_sequence = 0;
        self.player_state = PlayerState::default();

        // Reset physics body position
        if let Some(handle) = self.player_handle {
            self.physics.set_body_position(handle, self.position);
        }
    }

    pub fn pending_command_count(&self) -> usize {
        self.pending_commands.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smoothing() {
        let mut prediction = ClientPrediction::new(60);

        // Initial state - position starts at spawn point (0, 2, 0) with physics
        prediction.prepare_tick();
        prediction.store_command(&ClientCommand::new(0, 0), 1);

        // Get actual starting position (may differ due to physics)
        let start_pos = prediction.position;

        // Server says position shifted by 0.5 on X
        let server_pos = Vec3::new(start_pos.x + 0.5, start_pos.y, start_pos.z);
        prediction.reconcile(server_pos, Quat::IDENTITY, 1);

        // Logic should have shifted by the error
        assert!((prediction.position - server_pos).length() < 0.01); // Logic shifted
        assert!((prediction.position_error - Vec3::new(-0.5, 0.0, 0.0)).length() < 0.01); // Error negative

        // Check visual before update (alpha 0)
        prediction.update_visuals(0.0);
        // Visual = prev_position + error = start_pos + (-0.5, 0, 0) approximately = start position
        let visual = prediction.predicted_position();
        // Visual should be close to start position (error cancels out the correction)
        assert!((visual.x - start_pos.x).abs() < 0.1);

        // Update error decay
        prediction.update(0.05); // Error decays. -0.5 becomes closer to 0 (e.g. -0.18 using speed 20).
        prediction.update_visuals(0.0);

        // Visual moves towards Logic (server_pos.x).
        let visual_after = prediction.predicted_position();
        assert!(visual_after.x > start_pos.x + 0.1);
        assert!(visual_after.x < server_pos.x);
    }

    #[test]
    fn test_interpolation() {
        let mut prediction = ClientPrediction::new(60);
        // Start at spawn position
        let start = prediction.position;
        prediction.prepare_tick();
        // Move by 1 on X
        prediction.position = start + Vec3::new(1.0, 0.0, 0.0);

        // Prev = start. Curr = start + 1.

        // Alpha 0.5 - should be halfway
        prediction.update_visuals(0.5);
        assert!((prediction.predicted_position().x - (start.x + 0.5)).abs() < 0.01);
    }
}
