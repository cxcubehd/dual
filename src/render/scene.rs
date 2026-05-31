use dual::{MapObjectKind, TestingGround};

use super::geometry::StaticMesh;

pub fn create_testing_ground_meshes(
    device: &wgpu::Device,
    transform_bind_group_layout: &wgpu::BindGroupLayout,
) -> Vec<StaticMesh> {
    let testing_ground = TestingGround::new();
    testing_ground
        .objects()
        .iter()
        .filter_map(|object| match object.kind {
            MapObjectKind::Ground => Some(StaticMesh::new_ground(
                device,
                transform_bind_group_layout,
                object.half_extents.x * 2.0,
                object.position.y,
            )),
            MapObjectKind::StaticBox => Some(StaticMesh::new_box(
                device,
                transform_bind_group_layout,
                object.position,
                object.half_extents,
                [0.5, 0.5, 0.55],
            )),
            MapObjectKind::DynamicBox => None,
        })
        .collect()
}
