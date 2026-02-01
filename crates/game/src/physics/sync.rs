use glam::Vec3;

use crate::snapshot::{Entity, EntityType, World};

use super::PhysicsWorld;

pub struct PhysicsSync;

impl PhysicsSync {
    pub fn entity_to_physics(entity: &Entity, physics: &mut PhysicsWorld) {
        let Some(handle) = entity.physics_handle else {
            return;
        };

        physics.set_body_position(handle, entity.position);
        physics.set_body_velocity(handle, entity.velocity);
    }

    pub fn physics_to_entity(entity: &mut Entity, physics: &PhysicsWorld) {
        let Some(handle) = entity.physics_handle else {
            return;
        };

        if let Some(pos) = physics.body_position(handle) {
            if entity.position != pos {
                entity.position = pos;
                entity.dirty = true;
            }
        }

        if let Some(vel) = physics.body_velocity(handle) {
            if entity.velocity != vel {
                entity.velocity = vel;
                entity.dirty = true;
            }
        }
    }

    pub fn sync_world_to_physics(world: &World, physics: &mut PhysicsWorld) {
        for entity in world.entities() {
            Self::entity_to_physics(entity, physics);
        }
    }

    pub fn sync_physics_to_world(physics: &PhysicsWorld, world: &mut World) {
        for entity in world.entities_mut() {
            Self::physics_to_entity(entity, physics);
        }
    }

    pub fn create_physics_body(
        entity: &mut Entity,
        physics: &mut PhysicsWorld,
        player_radius: f32,
        player_height: f32,
    ) {
        if entity.physics_handle.is_some() {
            return;
        }

        let handle = match entity.entity_type {
            EntityType::Player => physics.add_player(entity.position, player_radius, player_height),
            EntityType::Projectile => physics.add_kinematic(entity.position),
            EntityType::DynamicProp => {
                physics.add_dynamic_box(entity.position, glam::Vec3::splat(0.5), 10.0)
            }
            EntityType::Static | EntityType::Trigger | EntityType::Item => {
                return;
            }
        };

        entity.physics_handle = Some(handle);
    }

    pub fn destroy_physics_body(entity: &mut Entity, physics: &mut PhysicsWorld) {
        if let Some(handle) = entity.physics_handle.take() {
            physics.remove_body(handle);
        }
    }

    pub fn apply_movement(
        entity: &mut Entity,
        physics: &mut PhysicsWorld,
        move_direction: Vec3,
        speed: f32,
        can_jump: bool,
        jump_impulse: f32,
    ) {
        let Some(handle) = entity.physics_handle else {
            return;
        };

        let grounded = physics.is_grounded(handle, 0.1);

        let velocity = if move_direction.length_squared() > 0.001 {
            let horizontal = move_direction.normalize() * speed;
            let current_vel = physics.body_velocity(handle).unwrap_or(Vec3::ZERO);
            Vec3::new(horizontal.x, current_vel.y, horizontal.z)
        } else {
            let current_vel = physics.body_velocity(handle).unwrap_or(Vec3::ZERO);
            Vec3::new(0.0, current_vel.y, 0.0)
        };

        physics.set_body_velocity(handle, velocity);

        if can_jump && grounded {
            physics.apply_impulse(handle, Vec3::new(0.0, jump_impulse, 0.0));
        }
    }
}
