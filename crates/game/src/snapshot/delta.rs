use std::collections::HashMap;

use rkyv::{Archive, Deserialize, Serialize};

use super::entity::{EntityState, NetId, FIELD_ANIMATION, FIELD_FLAGS, FIELD_HEALTH, FIELD_ORIENTATION, FIELD_POSITION, FIELD_VELOCITY, FIELD_WEAPON};

/// Delta for a single entity between baseline and current snapshot.
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[rkyv(derive(Debug))]
pub struct EntityDelta {
    pub net_id: NetId,
    /// Bitfield indicating which fields changed.
    pub changed_fields: u8,
    /// Full entity state (only changed fields are valid).
    pub state: EntityState,
}

/// Encodes / decodes delta-compressed snapshots.
pub struct DeltaEncoder;

impl DeltaEncoder {
    /// Build a delta-compressed set of entity changes.
    /// Returns (deltas, despawned_ids).
    pub fn encode(
        baseline: &[EntityState],
        current: &[EntityState],
    ) -> (Vec<EntityDelta>, Vec<NetId>) {
        let baseline_map: HashMap<NetId, &EntityState> =
            baseline.iter().map(|e| (e.entity_id, e)).collect();
        let current_map: HashMap<NetId, &EntityState> =
            current.iter().map(|e| (e.entity_id, e)).collect();

        let mut deltas = Vec::new();

        for entity in current {
            if let Some(base) = baseline_map.get(&entity.entity_id) {
                let changed = base.diff(entity);
                if changed != 0 {
                    deltas.push(EntityDelta {
                        net_id: entity.entity_id,
                        changed_fields: changed,
                        state: *entity,
                    });
                }
            } else {
                // New entity – send full state.
                deltas.push(EntityDelta {
                    net_id: entity.entity_id,
                    changed_fields: 0xFF,
                    state: *entity,
                });
            }
        }

        let despawned: Vec<NetId> = baseline
            .iter()
            .filter(|e| !current_map.contains_key(&e.entity_id))
            .map(|e| e.entity_id)
            .collect();

        (deltas, despawned)
    }

    /// Apply deltas on top of a baseline to reconstruct the current snapshot.
    pub fn decode(
        baseline: &[EntityState],
        deltas: &[EntityDelta],
        despawned: &[NetId],
    ) -> Vec<EntityState> {
        let mut result: Vec<EntityState> = baseline
            .iter()
            .filter(|e| !despawned.contains(&e.entity_id))
            .copied()
            .collect();

        for delta in deltas {
            if let Some(entity) = result.iter_mut().find(|e| e.entity_id == delta.net_id) {
                apply_delta(entity, delta);
            } else {
                // New entity.
                result.push(delta.state);
            }
        }

        result
    }
}

fn apply_delta(entity: &mut EntityState, delta: &EntityDelta) {
    let f = delta.changed_fields;
    if f & FIELD_POSITION != 0 {
        entity.position = delta.state.position;
    }
    if f & FIELD_VELOCITY != 0 {
        entity.velocity = delta.state.velocity;
    }
    if f & FIELD_ORIENTATION != 0 {
        entity.orientation = delta.state.orientation;
    }
    if f & FIELD_HEALTH != 0 {
        entity.health = delta.state.health;
    }
    if f & FIELD_WEAPON != 0 {
        entity.weapon_id = delta.state.weapon_id;
        entity.weapon_state = delta.state.weapon_state;
        entity.ammo = delta.state.ammo;
    }
    if f & FIELD_ANIMATION != 0 {
        entity.animation_state = delta.state.animation_state;
        entity.animation_frame = delta.state.animation_frame;
    }
    if f & FIELD_FLAGS != 0 {
        entity.flags = delta.state.flags;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_delta() {
        let baseline = vec![
            EntityState { entity_id: 1, position: [1.0, 0.0, 0.0], ..EntityState::new(1, 0) },
            EntityState { entity_id: 2, position: [5.0, 0.0, 0.0], ..EntityState::new(2, 0) },
        ];

        let current = vec![
            EntityState { entity_id: 1, position: [2.0, 0.0, 0.0], ..EntityState::new(1, 0) },
            EntityState { entity_id: 2, position: [5.0, 0.0, 0.0], ..EntityState::new(2, 0) },
            EntityState { entity_id: 3, position: [10.0, 0.0, 0.0], ..EntityState::new(3, 1) },
        ];

        let (deltas, despawned) = DeltaEncoder::encode(&baseline, &current);
        assert_eq!(deltas.len(), 2); // entity 1 changed, entity 3 new
        assert!(despawned.is_empty());

        let reconstructed = DeltaEncoder::decode(&baseline, &deltas, &despawned);
        assert_eq!(reconstructed.len(), 3);
        assert_eq!(reconstructed[0].position[0], 2.0);
    }

    #[test]
    fn delta_tracks_despawn() {
        let baseline = vec![
            EntityState::new(1, 0),
            EntityState::new(2, 0),
        ];
        let current = vec![
            EntityState::new(1, 0),
        ];

        let (_, despawned) = DeltaEncoder::encode(&baseline, &current);
        assert_eq!(despawned, vec![2]);

        let reconstructed = DeltaEncoder::decode(&baseline, &[], &despawned);
        assert_eq!(reconstructed.len(), 1);
        assert_eq!(reconstructed[0].entity_id, 1);
    }
}
