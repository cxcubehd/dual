//! Skybox rendering using a cubemap texture.
//!
//! Renders a unit cube centered at the camera position, sampled using a cubemap texture.
//! Drawn with depth write disabled so all other geometry renders on top.

use anyhow::{bail, Context, Result};

use crate::assets::Assets;

/// Cubemap face filenames in wgpu's expected order: +X, -X, +Y, -Y, +Z, -Z.
const CUBEMAP_FACE_NAMES: [&str; 6] = ["px.png", "nx.png", "py.png", "ny.png", "pz.png", "nz.png"];

/// Number of vertices for a cube (6 faces × 2 triangles × 3 vertices).
const CUBE_VERTEX_COUNT: u32 = 36;

/// Depth format used for the skybox pipeline.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// A skybox rendered using a cubemap texture.
pub struct Skybox {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    texture: wgpu::Texture,
}

impl Skybox {
    /// Loads a skybox from a folder containing cubemap face images.
    ///
    /// # Arguments
    /// * `device` - The wgpu device
    /// * `queue` - The wgpu queue for texture uploads
    /// * `camera_bind_group_layout` - Layout for the camera uniform (group 0)
    /// * `surface_format` - The render target format
    /// * `folder` - Asset folder path containing: px.png, nx.png, py.png, ny.png, pz.png, nz.png
    /// * `msaa_samples` - MSAA sample count for the pipeline
    ///
    /// # Errors
    /// Returns an error if any face image fails to load or has mismatched dimensions.
    pub fn load(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        surface_format: wgpu::TextureFormat,
        folder: &str,
        msaa_samples: u32,
    ) -> Result<Self> {
        let (texture, view) = Self::load_cubemap_texture(device, queue, folder)?;
        let sampler = Self::create_sampler(device);
        let texture_bind_group_layout = Self::create_texture_bind_group_layout(device);
        let bind_group =
            Self::create_bind_group(device, &texture_bind_group_layout, &view, &sampler);
        let pipeline = Self::create_pipeline(
            device,
            camera_bind_group_layout,
            &texture_bind_group_layout,
            surface_format,
            msaa_samples,
        );

        Ok(Self {
            pipeline,
            bind_group,
            texture,
        })
    }

    /// Records skybox draw commands to a render pass.
    pub fn draw<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera_bind_group: &'a wgpu::BindGroup,
    ) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera_bind_group, &[]);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.draw(0..CUBE_VERTEX_COUNT, 0..1);
    }

    /// Loads all 6 cubemap faces and creates the texture.
    fn load_cubemap_texture(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        folder: &str,
    ) -> Result<(wgpu::Texture, wgpu::TextureView)> {
        let faces = Self::load_face_images(folder)?;
        let size = Self::validate_face_dimensions(&faces)?;

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Skybox Cubemap"),
            size: wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        Self::upload_faces(queue, &texture, &faces, size);

        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Skybox Cubemap View"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });

        Ok((texture, view))
    }

    /// Loads and decodes all 6 face images.
    fn load_face_images(folder: &str) -> Result<Vec<CubemapFace>> {
        CUBEMAP_FACE_NAMES
            .iter()
            .map(|name| {
                let path = format!("{folder}/{name}");
                let bytes = Assets::load_bytes(&path)
                    .with_context(|| format!("Failed to load skybox face: {path}"))?;

                let img = image::load_from_memory(&bytes)
                    .with_context(|| format!("Failed to decode skybox face: {path}"))?;

                let rgba = img.to_rgba8();
                Ok(CubemapFace {
                    data: rgba.to_vec(),
                    width: rgba.width(),
                    height: rgba.height(),
                })
            })
            .collect()
    }

    /// Validates that all faces have identical square dimensions.
    fn validate_face_dimensions(faces: &[CubemapFace]) -> Result<u32> {
        let first = &faces[0];

        if first.width != first.height {
            bail!(
                "Skybox faces must be square, got {}x{}",
                first.width,
                first.height
            );
        }

        for (i, face) in faces.iter().enumerate().skip(1) {
            if face.width != first.width || face.height != first.height {
                bail!(
                    "Skybox face '{}' has dimensions {}x{}, expected {}x{}",
                    CUBEMAP_FACE_NAMES[i],
                    face.width,
                    face.height,
                    first.width,
                    first.height
                );
            }
        }

        Ok(first.width)
    }

    /// Uploads all face data to the cubemap texture.
    fn upload_faces(
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        faces: &[CubemapFace],
        size: u32,
    ) {
        for (layer, face) in faces.iter().enumerate() {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &face.data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * size),
                    rows_per_image: Some(size),
                },
                wgpu::Extent3d {
                    width: size,
                    height: size,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    fn create_sampler(device: &wgpu::Device) -> wgpu::Sampler {
        device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Skybox Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        })
    }

    fn create_texture_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Skybox Texture Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::Cube,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

    fn create_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Skybox Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    }

    fn create_pipeline(
        device: &wgpu::Device,
        camera_bind_group_layout: &wgpu::BindGroupLayout,
        texture_bind_group_layout: &wgpu::BindGroupLayout,
        surface_format: wgpu::TextureFormat,
        msaa_samples: u32,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Skybox Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/skybox.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Skybox Pipeline Layout"),
            bind_group_layouts: &[camera_bind_group_layout, texture_bind_group_layout],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Skybox Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: msaa_samples,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    }
}

/// Decoded image data for a single cubemap face.
struct CubemapFace {
    data: Vec<u8>,
    width: u32,
    height: u32,
}
