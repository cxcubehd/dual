use glam::Vec3;

use dual::{ClientCommand, Entity, EntityType, World};

pub fn apply_command(entity: &mut Entity, command: &ClientCommand, dt: f32) {
    let move_dir = command.decode_move_direction();
    let (yaw, pitch) = command.decode_view_angles();

    let speed = if command.has_flag(ClientCommand::FLAG_SPRINT) {
        10.0
    } else {
        5.0
    };

    let move_vec = Vec3::new(move_dir[0], move_dir[1], move_dir[2]);
    if move_vec.length_squared() > 0.001 {
        let normalized = move_vec.normalize();

        let (sin_yaw, cos_yaw) = yaw.sin_cos();
        let world_move = Vec3::new(
            normalized.x * cos_yaw + normalized.z * sin_yaw,
            normalized.y,
            -normalized.x * sin_yaw + normalized.z * cos_yaw,
        );

        entity.velocity = world_move * speed;
        entity.position += entity.velocity * dt;
    } else {
        entity.velocity = Vec3::ZERO;
    }

    entity.orientation = glam::Quat::from_euler(glam::EulerRot::YXZ, yaw, -pitch, 0.0);
    entity.dirty = true;
}

pub fn simulate_world(world: &mut World, dt: f32) {
    for entity in world.entities_mut() {
        match entity.entity_type {
            EntityType::Projectile => {
                simulate_projectile(entity, dt);
            }
            EntityType::Player => {}
            _ => {}
        }
    }
}

fn simulate_projectile(entity: &mut Entity, dt: f32) {
    entity.velocity.y -= 9.8 * dt;
    entity.position += entity.velocity * dt;

    if entity.position.y < 0.0 {
        entity.position.y = 0.0;
        entity.velocity = Vec3::ZERO;
    }

    entity.dirty = true;
}
