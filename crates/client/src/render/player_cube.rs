use glam::Mat4;
use wgpu::util::DeviceExt;

use super::geometry::{INDICES, Vertex, vertices_as_bytes};
use super::pipelines;

const PLAYER_CUBE_VERTICES: &[Vertex] = &[
    Vertex {
        position: [-0.5, -0.5, 0.5],
        color: [0.1, 0.1, 0.3],
    },
    Vertex {
        position: [0.5, -0.5, 0.5],
        color: [0.1, 0.1, 0.3],
    },
    Vertex {
        position: [0.5, 0.5, 0.5],
        color: [0.15, 0.15, 0.4],
    },
    Vertex {
        position: [-0.5, 0.5, 0.5],
        color: [0.15, 0.15, 0.4],
    },
    Vertex {
        position: [-0.5, -0.5, -0.5],
        color: [0.0, 0.0, 0.1],
    },
    Vertex {
        position: [0.5, -0.5, -0.5],
        color: [0.0, 0.0, 0.1],
    },
    Vertex {
        position: [0.5, 0.5, -0.5],
        color: [0.05, 0.05, 0.2],
    },
    Vertex {
        position: [-0.5, 0.5, -0.5],
        color: [0.05, 0.05, 0.2],
    },
];

pub struct PlayerCubeResources {
    pub pipeline: wgpu::RenderPipeline,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_indices: u32,
}

impl PlayerCubeResources {
    pub fn new(
        device: &wgpu::Device,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        transform_bind_group_layout: &wgpu::BindGroupLayout,
        config: &wgpu::SurfaceConfiguration,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Player Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/player.wgsl").into()),
        });

        let pipeline = pipelines::create_player_cube_pipeline(
            device,
            &shader,
            camera_bind_group_layout,
            transform_bind_group_layout,
            config,
        );

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Player Cube Vertex Buffer"),
            contents: vertices_as_bytes(PLAYER_CUBE_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Player Cube Index Buffer"),
            contents: bytemuck::cast_slice(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            num_indices: INDICES.len() as u32,
        }
    }
}

pub struct PlayerCube {
    pub transform_buffer: wgpu::Buffer,
    pub transform_bind_group: wgpu::BindGroup,
    pub visible: bool,
}

impl PlayerCube {
    pub fn new(device: &wgpu::Device, transform_bind_group_layout: &wgpu::BindGroupLayout) -> Self {
        let transform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Player Cube Transform Buffer"),
            contents: bytemuck::cast_slice(&Mat4::IDENTITY.to_cols_array()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let transform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Player Cube Transform Bind Group"),
            layout: transform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: transform_buffer.as_entire_binding(),
            }],
        });

        Self {
            transform_buffer,
            transform_bind_group,
            visible: false,
        }
    }
}
