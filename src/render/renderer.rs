use glam::Mat4;

use super::camera::{Camera, CameraUniform};
use super::constants::{CLEAR_COLOR, MSAA_SAMPLE_COUNT};
use super::geometry::StaticMesh;
use super::gpu;
use super::overlays::{DebugOverlay, MenuOverlay};
use super::pipelines;
use super::player_cube::{PlayerCube, PlayerCubeResources};
use super::resources::{DrawModel, Model, Skybox, Texture};
use super::scene;
use super::targets::RenderTargets;

use std::sync::Arc;

use anyhow::Result;
use winit::window::Window;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderError {
    Timeout,
    Occluded,
    Outdated,
    Lost,
    Validation,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    // Camera
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera_bind_group_layout: wgpu::BindGroupLayout,
    // Model pipeline (for textured models)
    model_pipeline: wgpu::RenderPipeline,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    model_transform_bind_group_layout: wgpu::BindGroupLayout,
    // Models
    models: Vec<Model>,
    // Player cubes
    player_cube_resources: PlayerCubeResources,
    player_cubes: Vec<PlayerCube>,
    // Static geometry (ground, platforms)
    static_meshes: Vec<StaticMesh>,
    // Skybox
    skybox: Option<Skybox>,
    // Other
    targets: RenderTargets,
    debug_overlay: DebugOverlay,
    menu_overlay: MenuOverlay,
    pub size: winit::dpi::PhysicalSize<u32>,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        let instance = gpu::create_instance();
        let surface = instance.create_surface(window)?;
        let adapter = gpu::request_adapter(&instance, &surface).await?;
        let (device, queue) = gpu::request_device(&adapter).await?;
        let config = gpu::create_surface_config(&surface, &adapter, size);
        surface.configure(&device, &config);

        let camera_buffer = pipelines::create_camera_buffer(&device);
        let camera_bind_group_layout = pipelines::create_camera_bind_group_layout(&device);
        let camera_bind_group =
            pipelines::create_camera_bind_group(&device, &camera_bind_group_layout, &camera_buffer);

        // Model shader for textured geometry
        let model_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Model Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/model.wgsl").into()),
        });

        let texture_bind_group_layout = Texture::bind_group_layout(&device);

        let model_transform_bind_group_layout =
            pipelines::create_model_transform_bind_group_layout(&device);

        let model_pipeline = pipelines::create_model_pipeline(
            &device,
            &model_shader,
            &camera_bind_group_layout,
            &texture_bind_group_layout,
            &model_transform_bind_group_layout,
            &config,
        );

        let player_cube_resources = PlayerCubeResources::new(
            &device,
            &camera_bind_group_layout,
            &model_transform_bind_group_layout,
            &config,
        );

        let targets = RenderTargets::new(&device, &config);

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

        let menu_overlay =
            MenuOverlay::new(&device, &queue, config.format, size.width, size.height);

        // Create testing ground static geometry
        let static_meshes =
            scene::create_testing_ground_meshes(&device, &model_transform_bind_group_layout);

        let renderer = Self {
            surface,
            device,
            queue,
            config,
            camera_buffer,
            camera_bind_group,
            camera_bind_group_layout,
            model_pipeline,
            texture_bind_group_layout,
            model_transform_bind_group_layout,
            models: Vec::new(),
            player_cube_resources,
            player_cubes: Vec::new(),
            static_meshes,
            skybox,
            targets,
            debug_overlay,
            menu_overlay,
            size,
        };

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
        self.targets = RenderTargets::new(&self.device, &self.config);
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
        let player_cube = PlayerCube::new(&self.device, &self.model_transform_bind_group_layout);
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

    pub fn render(&mut self) -> std::result::Result<(), RenderError> {
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output)
            | wgpu::CurrentSurfaceTexture::Suboptimal(output) => output,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(RenderError::Timeout),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(RenderError::Occluded),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(RenderError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(RenderError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::Validation),
        };
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
                view: &self.targets.msaa_view,
                resolve_target: Some(target),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(CLEAR_COLOR),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.targets.depth_view,
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

        // Draw static geometry (ground, platforms)
        if !self.static_meshes.is_empty() {
            pass.set_pipeline(&self.player_cube_resources.pipeline);
            for mesh in &self.static_meshes {
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_bind_group(1, &mesh.transform_bind_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..mesh.num_indices, 0, 0..1);
            }
        }

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
            pass.set_pipeline(&self.player_cube_resources.pipeline);
            pass.set_vertex_buffer(0, self.player_cube_resources.vertex_buffer.slice(..));
            pass.set_index_buffer(
                self.player_cube_resources.index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );

            for cube in visible_cubes {
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_bind_group(1, &cube.transform_bind_group, &[]);
                pass.draw_indexed(0..self.player_cube_resources.num_indices, 0, 0..1);
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
}
