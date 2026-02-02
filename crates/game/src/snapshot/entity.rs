use glam::{Quat, Vec3};
use rapier3d::dynamics::RigidBodyHandle;
use serde::{Deserialize, Serialize};

use crate::net::EntityState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[repr(u8)]
pub enum EntityType {
    #[default]
    Player = 0,
    Projectile = 1,
    Item = 2,
    Static = 3,
    Trigger = 4,
    DynamicProp = 5,
}

impl From<u8> for EntityType {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Player,
            1 => Self::Projectile,
            2 => Self::Item,
            3 => Self::Static,
            4 => Self::Trigger,
            5 => Self::DynamicProp,
            _ => Self::Static,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityHandle(pub u32);

impl EntityHandle {
    pub fn id(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: u32,
    pub entity_type: EntityType,
    pub position: Vec3,
    pub velocity: Vec3,
    pub orientation: Quat,
    pub scale: Vec3,
    pub shape: u8, // 0 = Box, 1 = Sphere
    pub physics_handle: Option<RigidBodyHandle>,
    pub animation_state: u8,
    pub animation_time: f32,
    pub flags: u16,
    pub dirty: bool,
}

impl Entity {
    pub fn new(id: u32, entity_type: EntityType) -> Self {
        Self {
            id,
            entity_type,
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            scale: Vec3::ONE,
            shape: 0,
            physics_handle: None,
            animation_state: 0,
            animation_time: 0.0,
            flags: 0,
            dirty: true,
        }
    }

    pub fn player(id: u32, spawn_position: Vec3) -> Self {
        Self {
            id,
            entity_type: EntityType::Player,
            position: spawn_position,
            velocity: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            scale: Vec3::ONE,
            shape: 0,
            physics_handle: None,
            animation_state: 0,
            animation_time: 0.0,
            flags: 0,
            dirty: true,
        }
    }

    pub fn handle(&self) -> EntityHandle {
        EntityHandle(self.id)
    }

    pub fn to_network_state(&self) -> EntityState {
        let mut state = EntityState::new(self.id, self.entity_type as u8);
        state.position = self.position.into();
        state.encode_velocity(self.velocity.into());
        state.encode_orientation([
            self.orientation.x,
            self.orientation.y,
            self.orientation.z,
            self.orientation.w,
        ]);
        state.encode_scale(self.scale.into());
        state.shape = self.shape;
        state.animation_state = self.animation_state;
        state.animation_frame = (self.animation_time.fract() * 255.0) as u8;
        state.flags = self.flags;
        state
    }

    pub fn from_network_state(state: &EntityState) -> Self {
        let vel = state.decode_velocity();
        let quat = state.decode_orientation();
        let scale = state.decode_scale();

        Self {
            id: state.entity_id,
            entity_type: EntityType::from(state.entity_type),
            position: Vec3::from(state.position),
            velocity: Vec3::from(vel),
            orientation: Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]).normalize(),
            scale: Vec3::from(scale),
            shape: state.shape,
            physics_handle: None,
            animation_state: state.animation_state,
            animation_time: state.animation_frame as f32 / 255.0,
            flags: state.flags,
            dirty: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_roundtrip() {
        let mut entity = Entity::player(42, Vec3::new(10.0, 5.0, -3.0));
        entity.velocity = Vec3::new(2.5, -1.0, 0.5);
        entity.orientation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
        entity.scale = Vec3::new(0.5, 2.0, 0.5);

        let network_state = entity.to_network_state();
        let reconstructed = Entity::from_network_state(&network_state);

        assert_eq!(entity.id, reconstructed.id);
        assert!((entity.position - reconstructed.position).length() < 0.001);
        assert!((entity.velocity - reconstructed.velocity).length() < 0.02);
        assert!((entity.scale - reconstructed.scale).length() < 0.02);
    }
}
