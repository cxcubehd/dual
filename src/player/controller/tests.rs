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

#[test]
fn controller_snaps_down_when_walking_down_slope() {
    let controller = PlayerController::default();
    let mut physics = PhysicsWorld::new();

    let ramp_position = Vec3::new(0.0, 1.0, 0.0);
    let ramp_half_extents = Vec3::new(2.0, 1.0, 5.0);
    physics.add_static_ramp(ramp_position, ramp_half_extents);

    let start_z = 4.0;
    let surface_y = ramp_surface_y(start_z, ramp_position, ramp_half_extents);
    let start_y = surface_y + controller.config().player_height * 0.5 + 0.02;
    let mut entity = Entity::player(1, Vec3::new(0.0, start_y, start_z));

    let handle = physics.add_player(
        entity.position,
        controller.config().player_radius,
        controller.config().player_height,
    );
    entity.physics_handle = Some(handle);
    physics.step();

    let mut state = PlayerState::new();
    state.velocity = Vec3::new(0.0, 0.0, -controller.config().move_speed_ground);
    state.grounded = true;

    let mut command = ClientCommand::new(0, 1);
    command.encode_move_direction([0.0, 0.0, -1.0]);

    controller.process(&command, &mut entity, &mut physics, &mut state, 1.0 / 60.0);

    assert!(
        entity.position.z < start_z,
        "expected z to decrease from {start_z}, got {}",
        entity.position.z
    );
    assert!(
        entity.position.y < start_y - 0.01,
        "expected y to decrease from {start_y}, got {}, grounded: {}",
        entity.position.y,
        state.grounded
    );
    assert!(
        state.grounded,
        "expected controller to stay grounded at {:?}",
        entity.position
    );
}

#[test]
fn controller_walks_up_slope() {
    let controller = PlayerController::default();
    let mut physics = PhysicsWorld::new();

    let ramp_position = Vec3::new(0.0, 1.0, 0.0);
    let ramp_half_extents = Vec3::new(2.0, 1.0, 5.0);
    physics.add_static_ramp(ramp_position, ramp_half_extents);

    let start_z = -4.0;
    let surface_y = ramp_surface_y(start_z, ramp_position, ramp_half_extents);
    let start_y = surface_y + controller.config().player_height * 0.5 + 0.02;
    let mut entity = Entity::player(1, Vec3::new(0.0, start_y, start_z));

    let handle = physics.add_player(
        entity.position,
        controller.config().player_radius,
        controller.config().player_height,
    );
    entity.physics_handle = Some(handle);
    physics.step();

    let mut state = PlayerState::new();
    let mut command = ClientCommand::new(0, 1);
    command.encode_move_direction([0.0, 0.0, 1.0]);

    for _ in 0..20 {
        controller.process(&command, &mut entity, &mut physics, &mut state, 1.0 / 60.0);
    }

    assert!(
        entity.position.z > start_z + 0.2,
        "expected z to increase from {start_z}, got {}",
        entity.position.z
    );
    assert!(
        entity.position.y > start_y,
        "expected y to increase from {start_y}, got {}",
        entity.position.y
    );
    assert!(
        state.grounded,
        "expected controller to stay grounded at {:?}",
        entity.position
    );
}

#[test]
fn controller_keeps_straight_velocity_when_slope_correction_drifts_sideways() {
    let controller = PlayerController::default();
    let velocity = Vec3::new(0.0, 0.0, 5.0);
    let desired = Vec3::new(0.0, 0.0, 1.0);
    let corrected = Vec3::new(0.05, 0.0, 1.0);

    let resolved = controller.resolve_horizontal_velocity(velocity, desired, corrected);

    assert!(
        resolved.x.abs() < 0.001,
        "unexpected sideways velocity: {resolved:?}"
    );
    assert!(resolved.z > 0.0, "expected forward velocity: {resolved:?}");
}

#[test]
fn controller_walks_up_stairs() {
    let controller = PlayerController::default();
    let mut physics = PhysicsWorld::new();
    physics.add_ground(0.0, 10.0);

    let stair_start = Vec3::new(0.0, 0.0, 0.0);
    let step_count = 6;
    let step_height = 0.2;
    let step_depth = 0.5;
    let step_width = 2.0;

    for i in 0..step_count {
        let y = step_height * (i as f32 + 0.5);
        let z = stair_start.z + step_depth * i as f32;
        physics.add_static_box(
            Vec3::new(stair_start.x, y, z),
            Vec3::new(step_width, step_height * 0.5, step_depth * 0.5),
        );
    }

    let start_y = 0.1 + controller.config().player_height * 0.5 + 0.02;
    let mut entity = Entity::player(1, Vec3::new(0.0, start_y, -0.35));

    let handle = physics.add_player(
        entity.position,
        controller.config().player_radius,
        controller.config().player_height,
    );
    entity.physics_handle = Some(handle);
    physics.step();

    let mut state = PlayerState::new();
    let mut command = ClientCommand::new(0, 1);
    command.encode_move_direction([0.0, 0.0, 1.0]);

    for _ in 0..20 {
        controller.process(&command, &mut entity, &mut physics, &mut state, 1.0 / 60.0);
    }

    assert!(
        entity.position.z > 0.5,
        "expected to climb stairs, got position {:?}",
        entity.position
    );
    assert!(
        entity.position.x.abs() < 0.05,
        "expected stair climb to stay straight, got position {:?}",
        entity.position
    );
    assert!(
        state.grounded,
        "expected controller to stay grounded at {:?}",
        entity.position
    );
}

#[test]
fn controller_keeps_walking_on_flat_ground() {
    let controller = PlayerController::default();
    let mut physics = PhysicsWorld::new();
    physics.add_ground(0.0, 10.0);

    let start_y = 0.1 + controller.config().player_height * 0.5 + 0.02;
    let mut entity = Entity::player(1, Vec3::new(0.0, start_y, 0.0));

    let handle = physics.add_player(
        entity.position,
        controller.config().player_radius,
        controller.config().player_height,
    );
    entity.physics_handle = Some(handle);
    physics.step();

    let mut state = PlayerState::new();
    let mut command = ClientCommand::new(0, 1);
    command.encode_move_direction([0.0, 0.0, 1.0]);

    for _ in 0..20 {
        controller.process(&command, &mut entity, &mut physics, &mut state, 1.0 / 60.0);
    }

    assert!(
        entity.position.z > 0.2,
        "expected z to increase on flat ground, got {}",
        entity.position.z
    );
    assert!(state.grounded, "expected controller to stay grounded");
}

fn ramp_surface_y(z: f32, ramp_position: Vec3, ramp_half_extents: Vec3) -> f32 {
    let slope_t = (z - (ramp_position.z - ramp_half_extents.z)) / (ramp_half_extents.z * 2.0);
    ramp_position.y - ramp_half_extents.y + slope_t * ramp_half_extents.y * 2.0
}
