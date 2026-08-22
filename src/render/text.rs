//! Text rendering using glyphon/cosmic-text

use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer as GlyphonTextRenderer, Viewport,
};
use wgpu::{Device, MultisampleState, Queue, TextureFormat};

use super::renderer::{RenderConfig, RendererError};

/// Text rendering system for spreadsheet cells
pub struct TextRenderer {
    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    renderer: GlyphonTextRenderer,
    viewport: Viewport,
    // Reusable buffers for text layout
    text_buffers: Vec<Buffer>,
}

impl TextRenderer {
    /// Create a new text renderer
    pub fn new(
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        _config: &RenderConfig,
    ) -> Result<Self, RendererError> {
        let mut font_system = FontSystem::new();

        // Load system fonts
        font_system.db_mut().load_system_fonts();

        let swash_cache = SwashCache::new();
        let cache = Cache::new(device);
        let mut atlas = TextAtlas::new(device, queue, &cache, format);
        let renderer =
            GlyphonTextRenderer::new(&mut atlas, device, MultisampleState::default(), None);

        let viewport = Viewport::new(device, &cache);

        Ok(Self {
            font_system,
            swash_cache,
            atlas,
            renderer,
            viewport,
            text_buffers: Vec::new(),
        })
    }

    /// Prepare text for rendering in a cell
    pub fn prepare_cell_text(
        &mut self,
        text: &str,
        _x: f32,
        _y: f32,
        width: f32,
        height: f32,
        font_size: f32,
        color: Color,
    ) -> usize {
        let metrics = Metrics::new(font_size, font_size * 1.2);

        let mut buffer = Buffer::new(&mut self.font_system, metrics);

        buffer.set_size(&mut self.font_system, Some(width), Some(height));

        let attrs = Attrs::new().family(Family::SansSerif).color(color);

        buffer.set_text(&mut self.font_system, text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.font_system, false);

        let buffer_idx = self.text_buffers.len();
        self.text_buffers.push(buffer);

        buffer_idx
    }

    /// Clear all prepared text buffers
    pub fn clear_buffers(&mut self) {
        self.text_buffers.clear();
    }

    /// Update viewport for current frame
    pub fn update_viewport(&mut self, _device: &Device, queue: &Queue, width: u32, height: u32) {
        self.viewport.update(queue, Resolution { width, height });
    }

    /// Prepare all text areas for rendering
    pub fn prepare(
        &mut self,
        device: &Device,
        queue: &Queue,
        text_areas: &[CellTextArea],
    ) -> Result<(), RendererError> {
        let areas: Vec<TextArea> = text_areas
            .iter()
            .filter_map(|area| {
                self.text_buffers
                    .get(area.buffer_index)
                    .map(|buffer| TextArea {
                        buffer,
                        left: area.x,
                        top: area.y,
                        scale: 1.0,
                        bounds: TextBounds {
                            left: area.x as i32,
                            top: area.y as i32,
                            right: (area.x + area.width) as i32,
                            bottom: (area.y + area.height) as i32,
                        },
                        default_color: area.color,
                        custom_glyphs: &[],
                    })
            })
            .collect();

        self.renderer
            .prepare(
                device,
                queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                areas,
                &mut self.swash_cache,
            )
            .map_err(|e| RendererError::ShaderError(format!("Text prepare error: {:?}", e)))?;

        Ok(())
    }

    /// Render text to a render pass
    pub fn render<'a>(
        &'a self,
        render_pass: &mut wgpu::RenderPass<'a>,
    ) -> Result<(), RendererError> {
        self.renderer
            .render(&self.atlas, &self.viewport, render_pass)
            .map_err(|e| RendererError::ShaderError(format!("Text render error: {:?}", e)))?;
        Ok(())
    }

    /// Trim the atlas to free unused space
    pub fn trim(&mut self) {
        self.atlas.trim();
    }
}

/// A text area to render in a cell
#[derive(Debug, Clone)]
pub struct CellTextArea {
    /// Index into the text buffer array
    pub buffer_index: usize,
    /// X position in pixels
    pub x: f32,
    /// Y position in pixels
    pub y: f32,
    /// Width in pixels
    pub width: f32,
    /// Height in pixels
    pub height: f32,
    /// Text color
    pub color: Color,
}

impl CellTextArea {
    pub fn new(buffer_index: usize, x: f32, y: f32, width: f32, height: f32, color: Color) -> Self {
        Self {
            buffer_index,
            x,
            y,
            width,
            height,
            color,
        }
    }
}
