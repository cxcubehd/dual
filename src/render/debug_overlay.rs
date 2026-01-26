use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport,
};

pub struct DebugOverlay {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    viewport: Viewport,
    buffer: Buffer,
}

impl DebugOverlay {
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

        let mut buffer = Buffer::new(&mut font_system, Metrics::new(16.0, 20.0));
        buffer.set_size(&mut font_system, Some(300.0), Some(100.0));

        Self {
            font_system,
            swash_cache,
            atlas,
            text_renderer,
            viewport,
            buffer,
        }
    }

    pub fn resize(&mut self, queue: &wgpu::Queue, width: u32, height: u32) {
        self.viewport.update(queue, Resolution { width, height });
    }

    pub fn update(&mut self, fps: f32, tick_rate: f32) {
        let text = format!("FPS: {:.1}\nTick: {:.1}/s", fps, tick_rate);

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
        screen_width: u32,
    ) -> Result<(), glyphon::PrepareError> {
        let buffer_width = self.buffer.size().0.unwrap_or(300.0) as i32;
        let padding = 10;
        let left = (screen_width as i32) - buffer_width - padding;

        let text_areas = [TextArea {
            buffer: &self.buffer,
            left: left as f32,
            top: padding as f32,
            scale: 1.0,
            bounds: TextBounds {
                left,
                top: padding,
                right: screen_width as i32 - padding,
                bottom: 100,
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
        self.text_renderer.render(&self.atlas, &self.viewport, pass)
    }
}
