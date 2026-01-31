use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadVertex {
    position: [f32; 2],
}

impl QuadVertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        }
    }
}

const QUAD_VERTICES: &[QuadVertex] = &[
    QuadVertex {
        position: [-1.0, -1.0],
    },
    QuadVertex {
        position: [1.0, -1.0],
    },
    QuadVertex {
        position: [1.0, 1.0],
    },
    QuadVertex {
        position: [-1.0, 1.0],
    },
];

const QUAD_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuOption {
    Resume,
    Disconnect,
    Quit,
}

impl MenuOption {
    pub fn all() -> &'static [MenuOption] {
        &[MenuOption::Resume, MenuOption::Disconnect, MenuOption::Quit]
    }

    pub fn label(&self) -> &'static str {
        match self {
            MenuOption::Resume => "Resume",
            MenuOption::Disconnect => "Disconnect",
            MenuOption::Quit => "Quit",
        }
    }
}

pub struct MenuOverlay {
    visible: bool,
    selected: usize,
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    viewport: Viewport,
    buffer: Buffer,
    quad_pipeline: wgpu::RenderPipeline,
    quad_vertex_buffer: wgpu::Buffer,
    quad_index_buffer: wgpu::Buffer,
    width: u32,
    height: u32,
}

impl MenuOverlay {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let mut font_system = FontSystem::new();
        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let text_renderer = TextRenderer::new(&mut atlas, device, Default::default(), None);
        let mut viewport = Viewport::new(device, &cache);
        viewport.update(queue, Resolution { width, height });

        let mut buffer = Buffer::new(&mut font_system, Metrics::new(24.0, 32.0));
        buffer.set_size(&mut font_system, Some(400.0), None);

        let quad_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Menu Quad Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/menu_quad.wgsl").into()),
        });

        let quad_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Menu Quad Pipeline Layout"),
            bind_group_layouts: &[],
            immediate_size: 0,
        });

        let quad_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Menu Quad Pipeline"),
            layout: Some(&quad_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &quad_shader,
                entry_point: Some("vs_main"),
                buffers: &[QuadVertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &quad_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let quad_vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Menu Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let quad_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Menu Quad Index Buffer"),
            contents: bytemuck::cast_slice(QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            visible: false,
            selected: 0,
            font_system,
            swash_cache,
            atlas,
            text_renderer,
            viewport,
            buffer,
            quad_pipeline,
            quad_vertex_buffer,
            quad_index_buffer,
            width,
            height,
        }
    }

    pub fn resize(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.viewport.update(queue, Resolution { width, height });
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.selected = 0;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    #[allow(dead_code)]
    pub fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }

    pub fn selected_option(&self) -> MenuOption {
        MenuOption::all()[self.selected]
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max = MenuOption::all().len() - 1;
        if self.selected < max {
            self.selected += 1;
        }
    }

    fn update_text(&mut self) {
        let mut text = String::from("  PAUSED\n\n");

        for (i, option) in MenuOption::all().iter().enumerate() {
            let marker = if i == self.selected { "> " } else { "  " };
            text.push_str(&format!("{}{}\n", marker, option.label()));
        }

        text.push_str("\nUp/Down: Select\nEnter: Confirm\nEsc: Resume");

        self.buffer.set_text(
            &mut self.font_system,
            &text,
            &Attrs::new().family(Family::Monospace),
            Shaping::Basic,
            None,
        );

        self.buffer.shape_until_scroll(&mut self.font_system, false);
    }

    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<(), glyphon::PrepareError> {
        if !self.visible {
            return Ok(());
        }

        self.update_text();

        let buffer_width = 300.0;
        let left = ((self.width as f32) - buffer_width) / 2.0;
        let top = (self.height as f32) / 2.0 - 100.0;

        let text_areas = [TextArea {
            buffer: &self.buffer,
            left,
            top,
            scale: 1.0,
            bounds: TextBounds {
                left: left as i32,
                top: top as i32,
                right: (left + buffer_width) as i32,
                bottom: (top + 300.0) as i32,
            },
            default_color: Color::rgb(255, 255, 255),
            custom_glyphs: &[],
        }];

        self.text_renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            text_areas,
            &mut self.swash_cache,
        )
    }

    pub fn render<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
    ) -> Result<(), glyphon::RenderError> {
        if !self.visible {
            return Ok(());
        }

        pass.set_pipeline(&self.quad_pipeline);
        pass.set_vertex_buffer(0, self.quad_vertex_buffer.slice(..));
        pass.set_index_buffer(self.quad_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..6, 0, 0..1);

        self.text_renderer.render(&self.atlas, &self.viewport, pass)
    }
}
