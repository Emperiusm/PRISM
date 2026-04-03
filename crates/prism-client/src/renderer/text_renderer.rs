// SPDX-License-Identifier: AGPL-3.0-or-later
//! glyphon-based GPU text rendering with glyph cache warming.

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer as GlyphonTextRenderer, Viewport, Weight,
};

use crate::ui::widgets::PaintContext;

/// Wraps glyphon's text rendering pipeline for use with wgpu.
///
/// Owns a `FontSystem`, glyph `SwashCache`, `TextAtlas`, and `GlyphonTextRenderer`.
/// Each frame, call [`prepare`] with the current `PaintContext` text runs,
/// then [`render`] inside a wgpu render pass.
pub struct TextPipeline {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: GlyphonTextRenderer,
    viewport: Viewport,
    /// Reusable buffer pool to avoid per-frame allocation.
    buffers: Vec<Buffer>,
}

impl TextPipeline {
    /// Create a new text rendering pipeline.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let mut font_system = FontSystem::new();

        // Load Material Symbols icon font — once, persistent in font database.
        let icon_font_data =
            include_bytes!("../../assets/fonts/MaterialSymbolsOutlined.ttf").to_vec();
        font_system.db_mut().load_font_data(icon_font_data);

        let swash_cache = SwashCache::new();
        let gpu_cache = Cache::new(device);
        let mut atlas = TextAtlas::new(device, queue, &gpu_cache, surface_format);
        let renderer =
            GlyphonTextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);
        let viewport = Viewport::new(device, &gpu_cache);

        Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            viewport,
            buffers: Vec::new(),
        }
    }

    /// Prepare text for rendering. Call once per frame before [`render`].
    ///
    /// Converts all `TextRun`s in `paint_ctx` into glyphon buffers, lays them
    /// out, and uploads glyph data to the atlas.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_width: u32,
        screen_height: u32,
        paint_ctx: &PaintContext,
    ) {
        self.viewport.update(
            queue,
            Resolution {
                width: screen_width,
                height: screen_height,
            },
        );

        let num_runs = paint_ctx.text_runs.len();

        // Grow buffer pool if needed
        while self.buffers.len() < num_runs {
            self.buffers
                .push(Buffer::new(&mut self.font_system, Metrics::new(14.0, 18.0)));
        }

        // Phase 1: update each buffer's content (requires &mut font_system)
        for (i, run) in paint_ctx.text_runs.iter().enumerate() {
            let buf = &mut self.buffers[i];
            let line_height = (run.font_size * 1.3).ceil();
            buf.set_metrics(
                &mut self.font_system,
                Metrics::new(run.font_size, line_height),
            );

            let family = if run.icon {
                Family::Name("Material Symbols Outlined")
            } else if run.monospace {
                Family::Monospace
            } else {
                Family::SansSerif
            };

            let weight = if run.bold {
                Weight::BOLD
            } else {
                Weight::NORMAL
            };

            buf.set_text(
                &mut self.font_system,
                &run.text,
                Attrs::new().family(family).weight(weight),
                Shaping::Advanced,
            );

            buf.set_size(&mut self.font_system, Some(2000.0), Some(line_height + 4.0));
            buf.shape_until_scroll(&mut self.font_system, false);
        }

        // Phase 2: build TextArea refs (borrows buffers immutably)
        let text_areas: Vec<TextArea> = paint_ctx
            .text_runs
            .iter()
            .enumerate()
            .map(|(i, run)| TextArea {
                buffer: &self.buffers[i],
                left: run.x,
                top: run.y,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: screen_width as i32,
                    bottom: screen_height as i32,
                },
                default_color: Color::rgba(
                    (run.color[0] * 255.0) as u8,
                    (run.color[1] * 255.0) as u8,
                    (run.color[2] * 255.0) as u8,
                    (run.color[3] * 255.0) as u8,
                ),
                custom_glyphs: &[],
            })
            .collect();

        // Phase 3: prepare the renderer (upload glyphs, update vertex buffer)
        if let Err(e) = self.renderer.prepare(
            device,
            queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            text_areas,
            &mut self.swash_cache,
        ) {
            tracing::warn!("glyphon prepare error: {e:?}");
        }
    }

    /// Render prepared text into the given render pass.
    pub fn render<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
    ) -> Result<(), glyphon::RenderError> {
        self.renderer.render(&self.atlas, &self.viewport, pass)
    }

    /// Trim unused atlas entries. Call periodically (e.g. every few seconds).
    pub fn trim(&mut self) {
        self.atlas.trim();
    }

    /// Borrow the font system (for text measurement).
    pub fn font_system_mut(&mut self) -> &mut FontSystem {
        &mut self.font_system
    }

    /// Measure the exact pixel width of shaped text. Use for critical layout
    /// (centering, breadcrumbs, header alignment). For non-critical layout,
    /// continue using `theme::text_width()` heuristic.
    pub fn text_width_exact(
        font_system: &mut FontSystem,
        text: &str,
        font_size: f32,
        bold: bool,
    ) -> f32 {
        let metrics = Metrics::new(font_size, font_size * 1.2);
        let mut buffer = Buffer::new(font_system, metrics);
        let weight = if bold { Weight::BOLD } else { Weight::NORMAL };
        let attrs = Attrs::new().family(Family::SansSerif).weight(weight);
        buffer.set_text(font_system, text, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(font_system, false);
        buffer
            .layout_runs()
            .map(|run| run.line_w)
            .next()
            .unwrap_or(0.0)
    }
}
