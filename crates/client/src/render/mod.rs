mod camera;
mod cube;
mod debug_overlay;
mod menu_overlay;
mod model;
mod skybox;
mod texture;
mod vertex;

pub use camera::Camera;
use glam::{Mat4, Vec3};
pub use menu_overlay::{MenuOption, MenuOverlay};
pub use model::{DrawModel, Model};
pub use texture::Texture;
pub use vertex::ModelVertex;

use camera::CameraUniform;
use cube::{INDICES, VERTICES};
use debug_overlay::DebugOverlay;
use skybox::Skybox;
use vertex::Vertex;

use crate::assets::Assets;

use std::sync::Arc;

use anyhow::Result;
use wgpu::util::DeviceExt;
use winit::window::Window;

fn indices_as_bytes(indices: &[u16]) -> &[u8] {
    unsafe {
        std::slice::from_raw_parts(
            indices.as_ptr() as *const u8,
            indices.len() * std::mem::size_of::<u16>(),
        )
    }
}

const CLEAR_COLOR: wgpu::Color = wgpu::Color {
    r: 0.1,
    g: 0.1,
    b: 0.15,
    a: 1.0,
};

/// MSAA sample count (4x anti-aliasing)
const MSAA_SAMPLE_COUNT: u32 = 4;

/// Red cube vertices for player representation
const PLAYER_CUBE_VERTICES: &[Vertex] = &[
    // Front face (red)
    Vertex { position: [-0.5, -0.5, 0.5], color: [0.1, 0.1, 0.3] },
    Vertex { position: [0.5, -0.5, 0.5], color: [0.1, 0.1, 0.3] },
    Vertex { position: [0.5, 0.5, 0.5], color: [0.15, 0.15, 0.4] },
    Vertex { position: [-0.5, 0.5, 0.5], color: [0.15, 0.15, 0.4] },
    // Back face (darker blue/black)
    Vertex { position: [-0.5, -0.5, -0.5], color: [0.0, 0.0, 0.1] },
    Vertex { position: [0.5, -0.5, -0.5], color: [0.0, 0.0, 0.1] },
    Vertex { position: [0.5, 0.5, -0.5], color: [0.05, 0.05, 0.2] },
    Vertex { position: [-0.5, 0.5, -0.5], color: [0.05, 0.05, 0.2] },
];

/// A player cube instance with its own transform
struct PlayerCube {
    transform_buffer: wgpu::Buffer,
    transform_bind_group: wgpu::BindGroup,
    visible: bool,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    // Legacy basic pipeline (for colored cubes)
    basic_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    num_indices: u32,
    // Camera
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    camera_bind_group_layout: wgpu::BindGroupLayout,
    // Model pipeline (for textured models)
    model_pipeline: wgpu::RenderPipeline,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    model_transform_bind_group_layout: wgpu::BindGroupLayout,
    // Models
    models: Vec<Model>,
    // Player cubes
    player_cube_pipeline: wgpu::RenderPipeline,
    player_cube_vertex_buffer: wgpu::Buffer,
    player_cube_index_buffer: wgpu::Buffer,
    player_cubes: Vec<PlayerCube>,
    // Skybox
    skybox: Option<Skybox>,
    // Other
    msaa_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
    debug_overlay: DebugOverlay,
    menu_overlay: MenuOverlay,
    pub size: winit::dpi::PhysicalSize<u32>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        let instance = Self::create_instance();
        let surface = instance.create_surface(window)?;
        let adapter = Self::request_adapter(&instance, &surface).await?;
        let (device, queue) = Self::request_device(&adapter).await?;
        let config = Self::create_surface_config(&surface, &adapter, size);
        surface.configure(&device, &config);

        let camera_buffer = Self::create_camera_buffer(&device);
        let camera_bind_group_layout = Self::create_camera_bind_group_layout(&device);
        let camera_bind_group =
            Self::create_camera_bind_group(&device, &camera_bind_group_layout, &camera_buffer);

        // Basic shader for colored geometry
        let basic_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Basic Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/basic.wgsl").into()),
        });

        let basic_pipeline =
            Self::create_basic_pipeline(&device, &basic_shader, &camera_bind_group_layout, &config);
        let vertex_buffer = Self::create_vertex_buffer(&device);
        let index_buffer = Self::create_index_buffer(&device);

        // Model shader for textured geometry
        let model_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Model Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/model.wgsl").into()),
        });

        let texture_bind_group_layout = Texture::bind_group_layout(&device);

        let model_transform_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Model Transform Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let model_pipeline = Self::create_model_pipeline(
            &device,
            &model_shader,
            &camera_bind_group_layout,
            &texture_bind_group_layout,
            &model_transform_bind_group_layout,
            &config,
        );

        // Player cube shader and pipeline
        let player_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Player Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/player.wgsl").into()),
        });

        let player_cube_pipeline = Self::create_player_cube_pipeline(
            &device,
            &player_shader,
            &camera_bind_group_layout,
            &model_transform_bind_group_layout,
            &config,
        );

        let player_cube_vertex_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Player Cube Vertex Buffer"),
                contents: vertex::vertices_as_bytes(PLAYER_CUBE_VERTICES),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let player_cube_index_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Player Cube Index Buffer"),
                contents: indices_as_bytes(INDICES), // Same indices as legacy cube
                usage: wgpu::BufferUsages::INDEX,
            });

        let msaa_view = Self::create_msaa_view(&device, &config);
        let depth_view = Self::create_depth_view(&device, &config);

        // Load skybox
        let skybox = match Skybox::load(
            &device,
            &queue,
            &camera_bind_group_layout,
            config.format,
            "skybox/sky_24_cubemap_2k",
            MSAA_SAMPLE_COUNT,
        ) {
            Ok(s) => {
                log::info!("Skybox loaded successfully");
                Some(s)
            }
            Err(e) => {
                log::warn!("Failed to load skybox: {}", e);
                None
            }
        };

        let debug_overlay = DebugOverlay::new(
            &adapter,
            &device,
            &queue,
            config.format,
            size.width,
            size.height,
        );

        let menu_overlay = MenuOverlay::new(
            &device,
            &queue,
            config.format,
            size.width,
            size.height,
        );

        let mut renderer = Self {
            surface,
            device,
            queue,
            config,
            basic_pipeline,
            vertex_buffer,
            index_buffer,
            num_indices: INDICES.len() as u32,
            camera_buffer,
            camera_bind_group,
            camera_bind_group_layout,
            model_pipeline,
            texture_bind_group_layout,
            model_transform_bind_group_layout,
            models: Vec::new(),
            player_cube_pipeline,
            player_cube_vertex_buffer,
            player_cube_index_buffer,
            player_cubes: Vec::new(),
            skybox,
            msaa_view,
            depth_view,
            debug_overlay,
            menu_overlay,
            size,
        };

        // Load the barn lamp model
        if let Some(bytes) = Assets::load_bytes("models/AnisotropyBarnLamp.glb") {
            if let Ok(index) = renderer.load_model_from_bytes(&bytes, "AnisotropyBarnLamp") {
                // Scale up by 25 and move next to cube
                let transform = Mat4::from_translation(Vec3::new(-0.5, 0.0, 0.0))
                    * Mat4::from_scale(Vec3::splat(9.0))
                    * Mat4::from_axis_angle(Vec3::new(0.0, 1.0, 0.0), 270f32.to_radians());
                renderer.set_model_transform(index, transform);
            } else {
                log::error!("Failed to load model");
            }
        } else {
            log::warn!("Model models/AnisotropyBarnLamp.glb not found in assets");
        }

        Ok(renderer)
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
        self.msaa_view = Self::create_msaa_view(&self.device, &self.config);
        self.depth_view = Self::create_depth_view(&self.device, &self.config);
        self.debug_overlay
            .resize(&self.queue, new_size.width, new_size.height);
        self.menu_overlay
            .resize(&self.queue, new_size.width, new_size.height);
    }

    pub fn update_camera(&self, camera: &Camera) {
        let uniform = CameraUniform::from_camera(camera);
        self.queue
            .write_buffer(&self.camera_buffer, 0, uniform.as_bytes());
    }

    pub fn update_debug_overlay(&mut self, fps: f32, tick_rate: f32) {
        self.debug_overlay.update(fps, tick_rate);
    }

    pub fn menu_overlay(&mut self) -> &mut MenuOverlay {
        &mut self.menu_overlay
    }

    /// Load a GLB model from bytes and add it to the renderer.
    ///
    /// Returns the index of the loaded model.
    pub fn load_model_from_bytes(&mut self, bytes: &[u8], label: &str) -> Result<usize> {
        let model = Model::from_glb(
            &self.device,
            &self.queue,
            bytes,
            &self.texture_bind_group_layout,
            &self.model_transform_bind_group_layout,
            label,
        )?;
        let index = self.models.len();
        self.models.push(model);
        Ok(index)
    }

    /// Set the transform of a model.
    pub fn set_model_transform(&mut self, index: usize, transform: glam::Mat4) {
        if let Some(model) = self.models.get_mut(index) {
            model.set_transform(&self.queue, transform);
        }
    }

    /// Load a texture from bytes.
    #[allow(dead_code)]
    pub fn load_texture_from_bytes(&self, bytes: &[u8], label: &str) -> Result<Texture> {
        Texture::from_bytes(&self.device, &self.queue, bytes, label)
    }

    /// Get the texture bind group layout (for custom materials).
    #[allow(dead_code)]
    pub fn texture_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.texture_bind_group_layout
    }

    /// Get the camera bind group layout.
    #[allow(dead_code)]
    pub fn camera_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.camera_bind_group_layout
    }

    /// Get the number of loaded models.
    #[allow(dead_code)]
    pub fn model_count(&self) -> usize {
        self.models.len()
    }

    /// Clear all loaded models.
    #[allow(dead_code)]
    pub fn clear_models(&mut self) {
        self.models.clear()
    }

    /// Add a new player cube and return its index.
    pub fn add_player_cube(&mut self) -> Result<usize> {
        let transform_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Player Cube Transform Buffer"),
            contents: bytemuck::cast_slice(&Mat4::IDENTITY.to_cols_array()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let transform_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Player Cube Transform Bind Group"),
            layout: &self.model_transform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: transform_buffer.as_entire_binding(),
            }],
        });

        let player_cube = PlayerCube {
            transform_buffer,
            transform_bind_group,
            visible: false,
        };

        let index = self.player_cubes.len();
        self.player_cubes.push(player_cube);
        Ok(index)
    }

    /// Set the transform of a player cube.
    pub fn set_player_cube_transform(&mut self, index: usize, transform: Mat4) {
        if let Some(cube) = self.player_cubes.get(index) {
            self.queue.write_buffer(
                &cube.transform_buffer,
                0,
                bytemuck::cast_slice(&transform.to_cols_array()),
            );
        }
    }

    /// Set the visibility of a player cube.
    pub fn set_player_cube_visible(&mut self, index: usize, visible: bool) {
        if let Some(cube) = self.player_cubes.get_mut(index) {
            cube.visible = visible;
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());

        self.record_render_pass(&mut encoder, &view);

        // Prepare and render debug overlay
        let _ = self
            .debug_overlay
            .prepare(&self.device, &self.queue, self.config.width);
        
        // Prepare menu overlay
        let _ = self.menu_overlay.prepare(&self.device, &self.queue);
        
        self.record_overlay_pass(&mut encoder, &view);

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
        Ok(())
    }

    fn record_render_pass(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.msaa_view,
                resolve_target: Some(target),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        // Draw skybox first (it writes at max depth with depth test LessEqual)
        if let Some(ref skybox) = self.skybox {
            skybox.draw(&mut pass, &self.camera_bind_group);
        }

        // Draw basic colored geometry (legacy cube)
        pass.set_pipeline(&self.basic_pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..self.num_indices, 0, 0..1);

        // Draw textured models
        if !self.models.is_empty() {
            pass.set_pipeline(&self.model_pipeline);
            for model in &self.models {
                pass.draw_model(model, &self.camera_bind_group);
            }
        }

        // Draw player cubes
        let visible_cubes: Vec<_> = self.player_cubes.iter().filter(|c| c.visible).collect();
        if !visible_cubes.is_empty() {
            pass.set_pipeline(&self.player_cube_pipeline);
            pass.set_vertex_buffer(0, self.player_cube_vertex_buffer.slice(..));
            pass.set_index_buffer(self.player_cube_index_buffer.slice(..), wgpu::IndexFormat::Uint16);

            for cube in visible_cubes {
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_bind_group(1, &cube.transform_bind_group, &[]);
                pass.draw_indexed(0..self.num_indices, 0, 0..1);
            }
        }
    }

    fn record_overlay_pass(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Overlay Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        let _ = self.menu_overlay.render(&mut pass);
        let _ = self.debug_overlay.render(&mut pass);
    }

    fn create_instance() -> wgpu::Instance {
        wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        })
    }

    async fn request_adapter(
        instance: &wgpu::Instance,
        surface: &wgpu::Surface<'static>,
    ) -> Result<wgpu::Adapter> {
        Ok(instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(surface),
                force_fallback_adapter: false,
            })
            .await?)
    }

    async fn request_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue)> {
        Ok(adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::default(),
            })
            .await?)
    }

    fn create_surface_config(
        surface: &wgpu::Surface,
        adapter: &wgpu::Adapter,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> wgpu::SurfaceConfiguration {
        let caps = surface.get_capabilities(adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        }
    }

    fn create_camera_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera Buffer"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn create_camera_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    fn create_camera_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    }

    fn create_basic_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        camera_layout: &wgpu::BindGroupLayout,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::RenderPipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[camera_layout],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Basic Render Pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: MSAA_SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    }

    fn create_model_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        camera_layout: &wgpu::BindGroupLayout,
        texture_layout: &wgpu::BindGroupLayout,
        model_transform_layout: &wgpu::BindGroupLayout,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::RenderPipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Model Pipeline Layout"),
            bind_group_layouts: &[camera_layout, texture_layout, model_transform_layout],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Model Render Pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[ModelVertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: MSAA_SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    }

    fn create_player_cube_pipeline(
        device: &wgpu::Device,
        shader: &wgpu::ShaderModule,
        camera_layout: &wgpu::BindGroupLayout,
        transform_layout: &wgpu::BindGroupLayout,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::RenderPipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Player Cube Pipeline Layout"),
            bind_group_layouts: &[camera_layout, transform_layout],
            immediate_size: 0,
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Player Cube Render Pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Cw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: MSAA_SAMPLE_COUNT,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        })
    }

    fn create_vertex_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: vertex::vertices_as_bytes(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        })
    }

    fn create_index_buffer(device: &wgpu::Device) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Index Buffer"),
            contents: indices_as_bytes(INDICES),
            usage: wgpu::BufferUsages::INDEX,
        })
    }

    fn create_msaa_view(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::TextureView {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("MSAA Texture"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: MSAA_SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        texture.create_view(&Default::default())
    }

    fn create_depth_view(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::TextureView {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: MSAA_SAMPLE_COUNT,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        texture.create_view(&Default::default())
    }
}
