use glam::{Quat, Vec3};

use crate::{Entity, EntityState, EntityType};

#[derive(Debug, Clone)]
pub struct InterpolatedEntity {
    pub id: u32,
    pub entity_type: EntityType,
    pub position: Vec3,
    pub velocity: Vec3,
    pub orientation: Quat,
    pub animation_state: u8,
    pub animation_time: f32,
    pub flags: u16,
}

impl From<&Entity> for InterpolatedEntity {
    fn from(entity: &Entity) -> Self {
        Self {
            id: entity.id,
            entity_type: entity.entity_type,
            position: entity.position,
            velocity: entity.velocity,
            orientation: entity.orientation,
            animation_state: entity.animation_state,
            animation_time: entity.animation_time,
            flags: entity.flags,
        }
    }
}

impl InterpolatedEntity {
    pub fn from_network_state(state: &EntityState) -> Self {
        let entity = Entity::from_network_state(state);
        Self::from(&entity)
    }
}
