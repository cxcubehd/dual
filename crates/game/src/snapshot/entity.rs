use glam::{Quat, Vec3};
use rapier3d::dynamics::RigidBodyHandle;
use rkyv::{Archive, Deserialize, Serialize};

/// Stable network entity identifier.
pub type NetId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[derive(serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum EntityKind {
    #[default]
    Player = 0,
    Projectile = 1,
    Item = 2,
    Static = 3,
    Trigger = 4,
    DynamicProp = 5,
}

impl From<u8> for EntityKind {
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

// ---------------------------------------------------------------------------
// Entity handle (lightweight reference into World)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityHandle(pub u32);

impl EntityHandle {
    pub fn id(self) -> u32 {
        self.0
    }
}

// ---------------------------------------------------------------------------
// Entity – the full in-memory representation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Entity {
    pub id: u32,
    pub kind: EntityKind,
    pub position: Vec3,
    pub velocity: Vec3,
    pub orientation: Quat,
    pub physics_handle: Option<RigidBodyHandle>,
    pub health: u8,
    pub max_health: u8,
    pub weapon_id: u8,
    pub weapon_state: u8,
    pub ammo: u16,
    pub animation_state: u8,
    pub animation_time: f32,
    pub flags: u16,
    pub dirty: bool,
}

impl Entity {
    pub fn new(id: u32, kind: EntityKind) -> Self {
        Self {
            id,
            kind,
            position: Vec3::ZERO,
            velocity: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            physics_handle: None,
            health: 100,
            max_health: 100,
            weapon_id: 0,
            weapon_state: 0,
            ammo: 0,
            animation_state: 0,
            animation_time: 0.0,
            flags: 0,
            dirty: true,
        }
    }

    pub fn player(id: u32, spawn_position: Vec3) -> Self {
        Self {
            id,
            kind: EntityKind::Player,
            position: spawn_position,
            velocity: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            physics_handle: None,
            health: 100,
            max_health: 100,
            weapon_id: 0,
            weapon_state: 0,
            ammo: 0,
            animation_state: 0,
            animation_time: 0.0,
            flags: 0,
            dirty: true,
        }
    }

    pub fn handle(&self) -> EntityHandle {
        EntityHandle(self.id)
    }

    /// Pack into compact wire format for network transmission.
    pub fn to_entity_state(&self) -> EntityState {
        let mut state = EntityState::new(self.id, self.kind as u8);
        state.position = self.position.into();
        state.encode_velocity(self.velocity.into());
        state.encode_orientation([
            self.orientation.x,
            self.orientation.y,
            self.orientation.z,
            self.orientation.w,
        ]);
        state.health = self.health;
        state.weapon_id = self.weapon_id;
        state.weapon_state = self.weapon_state;
        state.ammo = self.ammo;
        state.animation_state = self.animation_state;
        state.animation_frame = (self.animation_time.fract() * 255.0) as u8;
        state.flags = self.flags;
        state
    }

    /// Reconstruct from compact wire format.
    pub fn from_entity_state(state: &EntityState) -> Self {
        let vel = state.decode_velocity();
        let quat = state.decode_orientation();

        Self {
            id: state.entity_id,
            kind: EntityKind::from(state.entity_type),
            position: Vec3::from(state.position),
            velocity: Vec3::from(vel),
            orientation: Quat::from_xyzw(quat[0], quat[1], quat[2], quat[3]).normalize(),
            physics_handle: None,
            health: state.health,
            max_health: 100,
            weapon_id: state.weapon_id,
            weapon_state: state.weapon_state,
            ammo: state.ammo,
            animation_state: state.animation_state,
            animation_time: state.animation_frame as f32 / 255.0,
            flags: state.flags,
            dirty: false,
        }
    }
}

// ---------------------------------------------------------------------------
// EntityState – compact wire format for network transmission
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct EntityState {
    pub entity_id: u32,
    pub entity_type: u8,
    pub position: [f32; 3],
    /// Velocity compressed to i16 (scaled by 100).
    pub velocity: [i16; 3],
    /// Quaternion compressed to i16 (scaled by 32767).
    pub orientation: [i16; 4],
    pub health: u8,
    pub weapon_id: u8,
    pub weapon_state: u8,
    pub ammo: u16,
    pub animation_state: u8,
    pub animation_frame: u8,
    pub flags: u16,
}

impl EntityState {
    pub const MAX_VELOCITY: f32 = 327.67;

    pub fn new(entity_id: u32, entity_type: u8) -> Self {
        Self {
            entity_id,
            entity_type,
            position: [0.0; 3],
            velocity: [0; 3],
            orientation: [0, 0, 0, 32767],
            health: 100,
            weapon_id: 0,
            weapon_state: 0,
            ammo: 0,
            animation_state: 0,
            animation_frame: 0,
            flags: 0,
        }
    }

    pub fn encode_velocity(&mut self, vel: [f32; 3]) {
        self.velocity = [
            (vel[0].clamp(-Self::MAX_VELOCITY, Self::MAX_VELOCITY) * 100.0) as i16,
            (vel[1].clamp(-Self::MAX_VELOCITY, Self::MAX_VELOCITY) * 100.0) as i16,
            (vel[2].clamp(-Self::MAX_VELOCITY, Self::MAX_VELOCITY) * 100.0) as i16,
        ];
    }

    pub fn decode_velocity(&self) -> [f32; 3] {
        [
            self.velocity[0] as f32 / 100.0,
            self.velocity[1] as f32 / 100.0,
            self.velocity[2] as f32 / 100.0,
        ]
    }

    pub fn encode_orientation(&mut self, quat: [f32; 4]) {
        self.orientation = [
            (quat[0].clamp(-1.0, 1.0) * 32767.0) as i16,
            (quat[1].clamp(-1.0, 1.0) * 32767.0) as i16,
            (quat[2].clamp(-1.0, 1.0) * 32767.0) as i16,
            (quat[3].clamp(-1.0, 1.0) * 32767.0) as i16,
        ];
    }

    pub fn decode_orientation(&self) -> [f32; 4] {
        [
            self.orientation[0] as f32 / 32767.0,
            self.orientation[1] as f32 / 32767.0,
            self.orientation[2] as f32 / 32767.0,
            self.orientation[3] as f32 / 32767.0,
        ]
    }

    /// Compare two entity states field-by-field, returning a changed-fields bitfield.
    pub fn diff(&self, other: &EntityState) -> u8 {
        let mut changed = 0u8;
        if self.position != other.position {
            changed |= FIELD_POSITION;
        }
        if self.velocity != other.velocity {
            changed |= FIELD_VELOCITY;
        }
        if self.orientation != other.orientation {
            changed |= FIELD_ORIENTATION;
        }
        if self.health != other.health {
            changed |= FIELD_HEALTH;
        }
        if self.weapon_id != other.weapon_id
            || self.weapon_state != other.weapon_state
            || self.ammo != other.ammo
        {
            changed |= FIELD_WEAPON;
        }
        if self.animation_state != other.animation_state
            || self.animation_frame != other.animation_frame
        {
            changed |= FIELD_ANIMATION;
        }
        if self.flags != other.flags {
            changed |= FIELD_FLAGS;
        }
        changed
    }
}

// Changed-field bitfield constants.
pub const FIELD_POSITION: u8 = 1 << 0;
pub const FIELD_VELOCITY: u8 = 1 << 1;
pub const FIELD_ORIENTATION: u8 = 1 << 2;
pub const FIELD_HEALTH: u8 = 1 << 3;
pub const FIELD_WEAPON: u8 = 1 << 4;
pub const FIELD_ANIMATION: u8 = 1 << 5;
pub const FIELD_FLAGS: u8 = 1 << 6;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_roundtrip() {
        let mut entity = Entity::player(42, Vec3::new(10.0, 5.0, -3.0));
        entity.velocity = Vec3::new(2.5, -1.0, 0.5);
        entity.orientation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);

        let state = entity.to_entity_state();
        let reconstructed = Entity::from_entity_state(&state);

        assert_eq!(entity.id, reconstructed.id);
        assert!((entity.position - reconstructed.position).length() < 0.001);
        assert!((entity.velocity - reconstructed.velocity).length() < 0.02);
    }

    #[test]
    fn diff_detects_changes() {
        let mut a = EntityState::new(1, 0);
        a.position = [1.0, 2.0, 3.0];
        a.health = 100;

        let mut b = a;
        b.position = [1.1, 2.0, 3.0];
        b.health = 90;

        let changed = a.diff(&b);
        assert!(changed & FIELD_POSITION != 0);
        assert!(changed & FIELD_HEALTH != 0);
        assert!(changed & FIELD_VELOCITY == 0);
    }
}
