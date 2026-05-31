use dual::{EntityType, NetworkClient};
use glam::{Mat4, Vec3};

use crate::render::Renderer;

use super::App;

impl App {
    pub(super) fn update_player_cubes(
        player_cube_indices: &mut Vec<usize>,
        client: &NetworkClient,
        renderer: &mut Renderer,
    ) {
        let my_entity_id = client.entity_id();

        let entities: Vec<_> = client
            .entities()
            .filter(|e| e.entity_type == EntityType::Player)
            .collect();

        while player_cube_indices.len() < entities.len() {
            if let Ok(idx) = renderer.add_player_cube() {
                player_cube_indices.push(idx);
            } else {
                break;
            }
        }

        for (i, entity) in entities.iter().enumerate() {
            if let Some(&cube_idx) = player_cube_indices.get(i) {
                let is_local = my_entity_id.is_some_and(|id| entity.id == id);

                if !is_local {
                    let transform = Mat4::from_translation(entity.position)
                        * Mat4::from_quat(entity.orientation)
                        * Mat4::from_scale(Vec3::splat(0.4));
                    renderer.set_player_cube_transform(cube_idx, transform);
                    renderer.set_player_cube_visible(cube_idx, true);
                } else {
                    renderer.set_player_cube_visible(cube_idx, false);
                }
            }
        }

        for i in entities.len()..player_cube_indices.len() {
            if let Some(&cube_idx) = player_cube_indices.get(i) {
                renderer.set_player_cube_visible(cube_idx, false);
            }
        }
    }

    pub(super) fn update_dynamic_props(
        prop_indices: &mut Vec<usize>,
        client: &NetworkClient,
        renderer: &mut Renderer,
    ) {
        let entities: Vec<_> = client
            .entities()
            .filter(|e| e.entity_type == EntityType::DynamicProp)
            .collect();

        while prop_indices.len() < entities.len() {
            if let Ok(idx) = renderer.add_player_cube() {
                prop_indices.push(idx);
            } else {
                break;
            }
        }

        for (i, entity) in entities.iter().enumerate() {
            if let Some(&cube_idx) = prop_indices.get(i) {
                let transform = Mat4::from_translation(entity.position)
                    * Mat4::from_quat(entity.orientation)
                    * Mat4::from_scale(Vec3::splat(0.5));
                renderer.set_player_cube_transform(cube_idx, transform);
                renderer.set_player_cube_visible(cube_idx, true);
            }
        }

        for i in entities.len()..prop_indices.len() {
            if let Some(&cube_idx) = prop_indices.get(i) {
                renderer.set_player_cube_visible(cube_idx, false);
            }
        }
    }
}
