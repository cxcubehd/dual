use glam::{Quat, Vec3};

use crate::{EntityState, EntityType};

use super::InterpolatedEntity;

pub(super) fn interpolate_entity_states(
    from: &EntityState,
    to: &EntityState,
    t: f32,
) -> InterpolatedEntity {
    let from_pos = Vec3::from(from.position);
    let to_pos = Vec3::from(to.position);
    let position = from_pos.lerp(to_pos, t);

    let from_vel = Vec3::from(from.decode_velocity());
    let to_vel = Vec3::from(to.decode_velocity());
    let velocity = from_vel.lerp(to_vel, t);

    let from_quat = decode_quat(from);
    let to_quat = decode_quat(to);
    let orientation = if from_quat.dot(to_quat) < 0.0 {
        from_quat.slerp(-to_quat, t)
    } else {
        from_quat.slerp(to_quat, t)
    };

    let from_anim = from.animation_frame as f32 / 255.0;
    let to_anim = to.animation_frame as f32 / 255.0;
    let animation_time = lerp_wrapped(from_anim, to_anim, t);

    InterpolatedEntity {
        id: from.entity_id,
        entity_type: EntityType::from(from.entity_type),
        position,
        velocity,
        orientation,
        animation_state: if t < 0.5 {
            from.animation_state
        } else {
            to.animation_state
        },
        animation_time,
        flags: if t < 0.5 { from.flags } else { to.flags },
    }
}

fn decode_quat(state: &EntityState) -> Quat {
    let arr = state.decode_orientation();
    Quat::from_xyzw(arr[0], arr[1], arr[2], arr[3]).normalize()
}

fn lerp_wrapped(from: f32, to: f32, t: f32) -> f32 {
    if (to - from).abs() > 0.5 {
        if to < from {
            (from + (to + 1.0 - from) * t) % 1.0
        } else {
            (from + 1.0 + (to - from - 1.0) * t) % 1.0
        }
    } else {
        from + (to - from) * t
    }
}
