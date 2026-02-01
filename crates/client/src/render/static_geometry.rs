use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

use super::vertex::Vertex;

/// A static geometry piece (ground, platform, etc.)
pub struct StaticMesh {
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
    #[allow(dead_code)]
    pub transform_buffer: wgpu::Buffer,
    pub transform_bind_group: wgpu::BindGroup,
}

impl StaticMesh {
    pub fn new_box(
        device: &wgpu::Device,
        transform_bind_group_layout: &wgpu::BindGroupLayout,
        position: Vec3,
        half_extents: Vec3,
        color: [f32; 3],
    ) -> Self {
        let (vertices, indices) = create_box_mesh(half_extents, color);
        Self::from_vertices(
            device,
            transform_bind_group_layout,
            &vertices,
            &indices,
            position,
        )
    }

    pub fn new_ground(
        device: &wgpu::Device,
        transform_bind_group_layout: &wgpu::BindGroupLayout,
        size: f32,
        y: f32,
    ) -> Self {
        let (vertices, indices) = create_ground_mesh(size);
        Self::from_vertices(
            device,
            transform_bind_group_layout,
            &vertices,
            &indices,
            Vec3::new(0.0, y, 0.0),
        )
    }

    fn from_vertices(
        device: &wgpu::Device,
        transform_bind_group_layout: &wgpu::BindGroupLayout,
        vertices: &[Vertex],
        indices: &[u16],
        position: Vec3,
    ) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Static Mesh Vertex Buffer"),
            contents: super::vertex::vertices_as_bytes(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Static Mesh Index Buffer"),
            contents: indices_as_bytes(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let transform = Mat4::from_translation(position);
        let transform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Static Mesh Transform Buffer"),
            contents: bytemuck::cast_slice(&transform.to_cols_array()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let transform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Static Mesh Transform Bind Group"),
            layout: transform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: transform_buffer.as_entire_binding(),
            }],
        });

        Self {
            vertex_buffer,
            index_buffer,
            num_indices: indices.len() as u32,
            transform_buffer,
            transform_bind_group,
        }
    }
}

fn indices_as_bytes(indices: &[u16]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            indices.as_ptr() as *const u8,
            indices.len() * std::mem::size_of::<u16>(),
        )
    }
}

/// Create a ground plane mesh (grid pattern for visibility)
fn create_ground_mesh(size: f32) -> (Vec<Vertex>, Vec<u16>) {
    let half = size / 2.0;
    let tile_size = 4.0; // Size of each grid tile
    let tiles = (size / tile_size) as i32;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    let color1 = [0.3, 0.35, 0.3]; // Darker green-gray
    let color2 = [0.35, 0.4, 0.35]; // Lighter green-gray

    for z in 0..tiles {
        for x in 0..tiles {
            let use_color1 = (x + z) % 2 == 0;
            let color = if use_color1 { color1 } else { color2 };

            let x0 = -half + x as f32 * tile_size;
            let z0 = -half + z as f32 * tile_size;
            let x1 = x0 + tile_size;
            let z1 = z0 + tile_size;

            let base_idx = vertices.len() as u16;

            vertices.push(Vertex {
                position: [x0, 0.0, z0],
                color,
            });
            vertices.push(Vertex {
                position: [x1, 0.0, z0],
                color,
            });
            vertices.push(Vertex {
                position: [x1, 0.0, z1],
                color,
            });
            vertices.push(Vertex {
                position: [x0, 0.0, z1],
                color,
            });

            // Left-handed system: clockwise winding for front faces (looking down at ground)
            indices.extend_from_slice(&[
                base_idx,
                base_idx + 1,
                base_idx + 2,
                base_idx,
                base_idx + 2,
                base_idx + 3,
            ]);
        }
    }

    (vertices, indices)
}

/// Create a box mesh with the given half extents
fn create_box_mesh(half_extents: Vec3, color: [f32; 3]) -> (Vec<Vertex>, Vec<u16>) {
    let hx = half_extents.x;
    let hy = half_extents.y;
    let hz = half_extents.z;

    // Slightly vary colors per face for depth perception
    let top_color = [color[0] * 1.1, color[1] * 1.1, color[2] * 1.1];
    let bottom_color = [color[0] * 0.7, color[1] * 0.7, color[2] * 0.7];
    let front_color = color;
    let back_color = [color[0] * 0.9, color[1] * 0.9, color[2] * 0.9];
    let left_color = [color[0] * 0.85, color[1] * 0.85, color[2] * 0.85];
    let right_color = [color[0] * 0.95, color[1] * 0.95, color[2] * 0.95];

    #[rustfmt::skip]
    let vertices = vec![
        // Front face
        Vertex { position: [-hx, -hy,  hz], color: front_color },
        Vertex { position: [ hx, -hy,  hz], color: front_color },
        Vertex { position: [ hx,  hy,  hz], color: front_color },
        Vertex { position: [-hx,  hy,  hz], color: front_color },
        // Back face
        Vertex { position: [-hx, -hy, -hz], color: back_color },
        Vertex { position: [ hx, -hy, -hz], color: back_color },
        Vertex { position: [ hx,  hy, -hz], color: back_color },
        Vertex { position: [-hx,  hy, -hz], color: back_color },
        // Top face
        Vertex { position: [-hx,  hy, -hz], color: top_color },
        Vertex { position: [ hx,  hy, -hz], color: top_color },
        Vertex { position: [ hx,  hy,  hz], color: top_color },
        Vertex { position: [-hx,  hy,  hz], color: top_color },
        // Bottom face
        Vertex { position: [-hx, -hy, -hz], color: bottom_color },
        Vertex { position: [ hx, -hy, -hz], color: bottom_color },
        Vertex { position: [ hx, -hy,  hz], color: bottom_color },
        Vertex { position: [-hx, -hy,  hz], color: bottom_color },
        // Right face
        Vertex { position: [ hx, -hy, -hz], color: right_color },
        Vertex { position: [ hx,  hy, -hz], color: right_color },
        Vertex { position: [ hx,  hy,  hz], color: right_color },
        Vertex { position: [ hx, -hy,  hz], color: right_color },
        // Left face
        Vertex { position: [-hx, -hy, -hz], color: left_color },
        Vertex { position: [-hx,  hy, -hz], color: left_color },
        Vertex { position: [-hx,  hy,  hz], color: left_color },
        Vertex { position: [-hx, -hy,  hz], color: left_color },
    ];

    // Left-handed system: clockwise winding when viewed from outside
    #[rustfmt::skip]
    let indices: Vec<u16> = vec![
        0,  2,  1,  0,  3,  2,   // Front (+Z)
        4,  5,  6,  4,  6,  7,   // Back (-Z)
        8,  10, 9,  8,  11, 10,  // Top (+Y)
        12, 13, 14, 12, 14, 15,  // Bottom (-Y)
        16, 17, 18, 16, 18, 19,  // Right (+X)
        20, 22, 21, 20, 23, 22,  // Left (-X)
    ];

    (vertices, indices)
}
