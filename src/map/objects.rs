use glam::Vec3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MapObjectKind {
    Ground,
    StaticBox,
    DynamicBox,
}

#[derive(Debug, Clone)]
pub struct MapObject {
    pub kind: MapObjectKind,
    pub position: Vec3,
    pub half_extents: Vec3,
    pub mass: Option<f32>,
    pub entity_id: Option<u32>,
}

impl MapObject {
    pub fn ground(position: Vec3, half_size: f32) -> Self {
        Self {
            kind: MapObjectKind::Ground,
            position,
            half_extents: Vec3::new(half_size, 0.1, half_size),
            mass: None,
            entity_id: None,
        }
    }

    pub fn static_box(position: Vec3, half_extents: Vec3) -> Self {
        Self {
            kind: MapObjectKind::StaticBox,
            position,
            half_extents,
            mass: None,
            entity_id: None,
        }
    }

    pub fn dynamic_box(position: Vec3, half_extents: Vec3, mass: f32) -> Self {
        Self {
            kind: MapObjectKind::DynamicBox,
            position,
            half_extents,
            mass: Some(mass),
            entity_id: None,
        }
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self.kind, MapObjectKind::DynamicBox)
    }
}
