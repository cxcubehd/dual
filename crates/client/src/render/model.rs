//! GLTF/GLB model loading and rendering.
//!
//! Provides `Model`, `Mesh`, and `Material` abstractions for loading and rendering 3D models.

use std::ops::Range;

use anyhow::{Context, Result};
use glam::Mat4;
use wgpu::util::DeviceExt;

use super::texture::Texture;
use super::vertex::ModelVertex;

/// Uniform buffer data for model transformation.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelUniform {
    pub transform: [[f32; 4]; 4],
}

impl ModelUniform {
    pub fn new(transform: Mat4) -> Self {
        Self {
            transform: transform.to_cols_array_2d(),
        }
    }
}

/// A material with a diffuse texture.
pub struct Material {
    pub name: String,
    pub diffuse_texture: Texture,
    pub bind_group: wgpu::BindGroup,
}

impl Material {
    pub fn new(
        device: &wgpu::Device,
        name: &str,
        diffuse_texture: Texture,
        layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let bind_group = diffuse_texture.bind_group(device, layout);
        Self {
            name: name.to_string(),
            diffuse_texture,
            bind_group,
        }
    }
}

/// A mesh is a collection of vertices and indices that reference a material.
pub struct Mesh {
    pub name: String,
    pub vertex_buffer: wgpu::Buffer,
    pub index_buffer: wgpu::Buffer,
    pub num_elements: u32,
    pub material_index: usize,
}

/// A complete 3D model consisting of meshes and materials.
pub struct Model {
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
    pub transform: Mat4,
    pub uniform_buffer: wgpu::Buffer,
    pub transform_bind_group: wgpu::BindGroup,
}

impl Model {
    /// Load a GLTF/GLB model from bytes.
    pub fn from_glb(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
        texture_layout: &wgpu::BindGroupLayout,
        transform_layout: &wgpu::BindGroupLayout,
        label: &str,
    ) -> Result<Self> {
        let gltf = gltf::Gltf::from_slice(bytes)
            .with_context(|| format!("Failed to parse GLTF: {}", label))?;

        let buffer_data = Self::load_buffers(&gltf, bytes)?;
        let materials = Self::load_materials(&gltf, &buffer_data, device, queue, texture_layout)?;
        let meshes = Self::load_meshes(&gltf, &buffer_data, device, label)?;

        let transform = Mat4::IDENTITY;
        let uniform = ModelUniform::new(transform);
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{} Transform Buffer", label)),
            contents: bytemuck::cast_slice(&[uniform]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let transform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("{} Transform Bind Group", label)),
            layout: transform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        Ok(Self {
            meshes,
            materials,
            transform,
            uniform_buffer,
            transform_bind_group,
        })
    }

    pub fn set_transform(&mut self, queue: &wgpu::Queue, transform: Mat4) {
        self.transform = transform;
        let uniform = ModelUniform::new(transform);
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniform]));
    }


    fn load_buffers(gltf: &gltf::Gltf, glb_bytes: &[u8]) -> Result<Vec<Vec<u8>>> {
        let mut buffer_data = Vec::new();

        for buffer in gltf.buffers() {
            let data = match buffer.source() {
                gltf::buffer::Source::Bin => {
                    // For GLB files, the binary data is embedded
                    gltf.blob
                        .as_ref()
                        .map(|b| b.clone())
                        .or_else(|| {
                            // Try to extract from the original bytes
                            // GLB header is 12 bytes, then JSON chunk, then BIN chunk
                            if glb_bytes.len() > 12 {
                                let json_length = u32::from_le_bytes([
                                    glb_bytes[12],
                                    glb_bytes[13],
                                    glb_bytes[14],
                                    glb_bytes[15],
                                ]) as usize;
                                let bin_offset = 12 + 8 + json_length + 8; // header + json header + json + bin header
                                if bin_offset < glb_bytes.len() {
                                    Some(glb_bytes[bin_offset..].to_vec())
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        })
                        .context("GLB missing binary blob")?
                }
                gltf::buffer::Source::Uri(_uri) => {
                    // For external files, we'd need to load them
                    // For now, we only support embedded GLB
                    anyhow::bail!("External buffer URIs not supported, use GLB format");
                }
            };
            buffer_data.push(data);
        }

        Ok(buffer_data)
    }

    fn load_materials(
        gltf: &gltf::Gltf,
        buffer_data: &[Vec<u8>],
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture_layout: &wgpu::BindGroupLayout,
    ) -> Result<Vec<Material>> {
        let mut materials = Vec::new();

        for material in gltf.materials() {
            let pbr = material.pbr_metallic_roughness();

            let diffuse_texture = if let Some(info) = pbr.base_color_texture() {
                let texture = info.texture();
                let image = texture.source();

                match image.source() {
                    gltf::image::Source::View { view, mime_type: _ } => {
                        let buffer = &buffer_data[view.buffer().index()];
                        let start = view.offset();
                        let end = start + view.length();
                        let image_data = &buffer[start..end];
                        Texture::from_bytes(device, queue, image_data, &material.name().unwrap_or("texture"))?
                    }
                    gltf::image::Source::Uri { uri: _, mime_type: _ } => {
                        // External textures not supported
                        Texture::white(device, queue)
                    }
                }
            } else {
                // No texture, use white fallback (color comes from base_color_factor)
                Texture::white(device, queue)
            };

            let material = Material::new(
                device,
                material.name().unwrap_or("unnamed"),
                diffuse_texture,
                texture_layout,
            );
            materials.push(material);
        }

        // Add a default material if none exist
        if materials.is_empty() {
            let white = Texture::white(device, queue);
            materials.push(Material::new(device, "default", white, texture_layout));
        }

        Ok(materials)
    }

    fn load_meshes(
        gltf: &gltf::Gltf,
        buffer_data: &[Vec<u8>],
        device: &wgpu::Device,
        label: &str,
    ) -> Result<Vec<Mesh>> {
        let mut meshes = Vec::new();

        for mesh in gltf.meshes() {
            for primitive in mesh.primitives() {
                let reader = primitive.reader(|buffer| Some(&buffer_data[buffer.index()]));

                // Read positions
                let positions: Vec<[f32; 3]> = reader
                    .read_positions()
                    .context("Mesh missing positions")?
                    .collect();

                // Read texture coordinates (default to 0,0 if missing)
                let tex_coords: Vec<[f32; 2]> = reader
                    .read_tex_coords(0)
                    .map(|tc| tc.into_f32().collect())
                    .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);

                // Read normals (default to up if missing)
                let normals: Vec<[f32; 3]> = reader
                    .read_normals()
                    .map(|n| n.collect())
                    .unwrap_or_else(|| vec![[0.0, 1.0, 0.0]; positions.len()]);

                // Build vertices
                let vertices: Vec<ModelVertex> = positions
                    .iter()
                    .zip(tex_coords.iter())
                    .zip(normals.iter())
                    .map(|((pos, tex), norm)| ModelVertex {
                        position: *pos,
                        tex_coords: *tex,
                        normal: *norm,
                    })
                    .collect();

                // Read indices
                let indices: Vec<u32> = reader
                    .read_indices()
                    .context("Mesh missing indices")?
                    .into_u32()
                    .collect();

                let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("{} Vertex Buffer", label)),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });

                let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("{} Index Buffer", label)),
                    contents: bytemuck::cast_slice(&indices),
                    usage: wgpu::BufferUsages::INDEX,
                });

                let material_index = primitive.material().index().unwrap_or(0);

                meshes.push(Mesh {
                    name: mesh.name().unwrap_or("unnamed").to_string(),
                    vertex_buffer,
                    index_buffer,
                    num_elements: indices.len() as u32,
                    material_index,
                });
            }
        }

        Ok(meshes)
    }
}

/// Trait for types that can be drawn as models.
pub trait DrawModel<'a> {
    fn draw_mesh(
        &mut self,
        mesh: &'a Mesh,
        material: &'a Material,
        camera_bind_group: &'a wgpu::BindGroup,
    );

    fn draw_mesh_instanced(
        &mut self,
        mesh: &'a Mesh,
        material: &'a Material,
        instances: Range<u32>,
        camera_bind_group: &'a wgpu::BindGroup,
    );

    fn draw_model(&mut self, model: &'a Model, camera_bind_group: &'a wgpu::BindGroup);

    fn draw_model_instanced(
        &mut self,
        model: &'a Model,
        instances: Range<u32>,
        camera_bind_group: &'a wgpu::BindGroup,
    );
}

impl<'a, 'b> DrawModel<'b> for wgpu::RenderPass<'a>
where
    'b: 'a,
{
    fn draw_mesh(
        &mut self,
        mesh: &'b Mesh,
        material: &'b Material,
        camera_bind_group: &'b wgpu::BindGroup,
    ) {
        self.draw_mesh_instanced(mesh, material, 0..1, camera_bind_group);
    }

    fn draw_mesh_instanced(
        &mut self,
        mesh: &'b Mesh,
        material: &'b Material,
        instances: Range<u32>,
        camera_bind_group: &'b wgpu::BindGroup,
    ) {
        self.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
        self.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        self.set_bind_group(0, camera_bind_group, &[]);
        self.set_bind_group(1, &material.bind_group, &[]);
        self.draw_indexed(0..mesh.num_elements, 0, instances);
    }

    fn draw_model(&mut self, model: &'b Model, camera_bind_group: &'b wgpu::BindGroup) {
        self.draw_model_instanced(model, 0..1, camera_bind_group);
    }

    fn draw_model_instanced(
        &mut self,
        model: &'b Model,
        instances: Range<u32>,
        camera_bind_group: &'b wgpu::BindGroup,
    ) {
        for mesh in &model.meshes {
            let material = &model.materials[mesh.material_index.min(model.materials.len() - 1)];
            self.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
            self.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
            self.set_bind_group(0, camera_bind_group, &[]);
            self.set_bind_group(1, &material.bind_group, &[]);
            self.set_bind_group(2, &model.transform_bind_group, &[]);
            self.draw_indexed(0..mesh.num_elements, 0, instances.clone());
        }
    }
}
