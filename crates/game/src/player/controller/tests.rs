use glam::Vec3;

use crate::net::ClientCommand;
use crate::physics::PhysicsWorld;
use crate::snapshot::Entity;

use super::*;

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
