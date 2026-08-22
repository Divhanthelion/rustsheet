//! Main GPU renderer coordinating wgpu context and rendering pipeline

use std::sync::Arc;
use wgpu::{Adapter, Device, Instance, Queue, Surface, SurfaceConfiguration, TextureFormat};

/// Configuration for the renderer
#[derive(Debug, Clone)]
pub struct RenderConfig {
    /// Width of the render target in pixels
    pub width: u32,
    /// Height of the render target in pixels
    pub height: u32,
    /// Cell default width in pixels
    pub default_cell_width: f32,
    /// Cell default height in pixels
    pub default_cell_height: f32,
    /// Grid line width in pixels
    pub grid_line_width: f32,
    /// Background color (RGBA)
    pub background_color: [f32; 4],
    /// Grid line color (RGBA)
    pub grid_line_color: [f32; 4],
    /// Font size in points
    pub font_size: f32,
    /// Enable VSync
    pub vsync: bool,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            default_cell_width: 100.0,
            default_cell_height: 24.0,
            grid_line_width: 1.0,
            background_color: [1.0, 1.0, 1.0, 1.0], // White
            grid_line_color: [0.8, 0.8, 0.8, 1.0],  // Light gray
            font_size: 12.0,
            vsync: true,
        }
    }
}

/// The main GPU renderer
pub struct GpuRenderer {
    #[allow(dead_code)]
    instance: Instance,
    #[allow(dead_code)]
    adapter: Arc<Adapter>,
    device: Arc<Device>,
    queue: Arc<Queue>,
    surface: Option<Surface<'static>>,
    surface_config: Option<SurfaceConfiguration>,
    config: RenderConfig,
    // Viewport state
    scroll_x: f32,
    scroll_y: f32,
    scale: f32,
}

impl GpuRenderer {
    /// Create a new renderer without a surface (for headless rendering)
    pub async fn new_headless(config: RenderConfig) -> Result<Self, RendererError> {
        let instance = Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_e| RendererError::NoAdapter)?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("rustsheet_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::default(),
            })
            .await
            .map_err(|e| RendererError::DeviceError(e.to_string()))?;

        Ok(Self {
            instance,
            adapter: Arc::new(adapter),
            device: Arc::new(device),
            queue: Arc::new(queue),
            surface: None,
            surface_config: None,
            config,
            scroll_x: 0.0,
            scroll_y: 0.0,
            scale: 1.0,
        })
    }

    /// Create a renderer with a window surface
    ///
    /// # Safety
    /// The window must remain valid for the lifetime of the surface.
    pub async fn new_with_surface<W>(window: W, config: RenderConfig) -> Result<Self, RendererError>
    where
        W: raw_window_handle::HasWindowHandle
            + raw_window_handle::HasDisplayHandle
            + Send
            + Sync
            + 'static,
    {
        let instance = Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(window)
            .map_err(|e| RendererError::SurfaceError(e.to_string()))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_e| RendererError::NoAdapter)?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("rustsheet_device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::default(),
            })
            .await
            .map_err(|e| RendererError::DeviceError(e.to_string()))?;

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: config.width,
            height: config.height,
            present_mode: if config.vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            },
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        Ok(Self {
            instance,
            adapter: Arc::new(adapter),
            device: Arc::new(device),
            queue: Arc::new(queue),
            surface: Some(surface),
            surface_config: Some(surface_config),
            config,
            scroll_x: 0.0,
            scroll_y: 0.0,
            scale: 1.0,
        })
    }

    /// Resize the render target
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;

        if let (Some(surface), Some(config)) = (&self.surface, &mut self.surface_config) {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
        }
    }

    /// Set scroll position
    pub fn set_scroll(&mut self, x: f32, y: f32) {
        self.scroll_x = x;
        self.scroll_y = y;
    }

    /// Get current scroll position
    pub fn scroll(&self) -> (f32, f32) {
        (self.scroll_x, self.scroll_y)
    }

    /// Set zoom scale
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.clamp(0.1, 5.0);
    }

    /// Get current scale
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Get the device
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Get the queue
    pub fn queue(&self) -> &Queue {
        &self.queue
    }

    /// Get the preferred texture format
    pub fn texture_format(&self) -> TextureFormat {
        self.surface_config
            .as_ref()
            .map(|c| c.format)
            .unwrap_or(TextureFormat::Bgra8UnormSrgb)
    }

    /// Get the config
    pub fn config(&self) -> &RenderConfig {
        &self.config
    }

    /// Get viewport dimensions
    pub fn viewport_size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Calculate which cells are visible in the current viewport
    pub fn visible_cell_range(&self) -> VisibleRange {
        let cell_w = self.config.default_cell_width * self.scale;
        let cell_h = self.config.default_cell_height * self.scale;

        let start_col = (self.scroll_x / cell_w).floor() as u32;
        let start_row = (self.scroll_y / cell_h).floor() as u32;

        let visible_cols = ((self.config.width as f32) / cell_w).ceil() as u32 + 1;
        let visible_rows = ((self.config.height as f32) / cell_h).ceil() as u32 + 1;

        VisibleRange {
            start_col,
            start_row,
            end_col: start_col + visible_cols,
            end_row: start_row + visible_rows,
        }
    }

    /// Begin a new frame for rendering
    pub fn begin_frame(&self) -> Result<RenderFrame<'_>, RendererError> {
        let surface = self.surface.as_ref().ok_or(RendererError::NoSurface)?;

        let output = surface
            .get_current_texture()
            .map_err(|e| RendererError::SurfaceError(e.to_string()))?;

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("render_encoder"),
            });

        Ok(RenderFrame {
            output,
            view,
            encoder,
            config: &self.config,
        })
    }

    /// Submit the frame for presentation
    pub fn end_frame(&self, frame: RenderFrame) {
        self.queue.submit(std::iter::once(frame.encoder.finish()));
        frame.output.present();
    }
}

/// A render frame in progress
pub struct RenderFrame<'a> {
    output: wgpu::SurfaceTexture,
    view: wgpu::TextureView,
    encoder: wgpu::CommandEncoder,
    config: &'a RenderConfig,
}

impl<'a> RenderFrame<'a> {
    /// Get the texture view to render to
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Get the command encoder
    pub fn encoder_mut(&mut self) -> &mut wgpu::CommandEncoder {
        &mut self.encoder
    }

    /// Get the render config
    pub fn config(&self) -> &RenderConfig {
        self.config
    }

    /// Clear the frame with the background color
    pub fn clear(&mut self) {
        let _render_pass = self.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: self.config.background_color[0] as f64,
                        g: self.config.background_color[1] as f64,
                        b: self.config.background_color[2] as f64,
                        a: self.config.background_color[3] as f64,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
}

/// Visible cell range in the viewport
#[derive(Debug, Clone, Copy)]
pub struct VisibleRange {
    pub start_col: u32,
    pub start_row: u32,
    pub end_col: u32,
    pub end_row: u32,
}

/// Renderer errors
#[derive(Debug, thiserror::Error)]
pub enum RendererError {
    #[error("No suitable GPU adapter found")]
    NoAdapter,
    #[error("Failed to create device: {0}")]
    DeviceError(String),
    #[error("Surface error: {0}")]
    SurfaceError(String),
    #[error("No surface configured for rendering")]
    NoSurface,
    #[error("Shader compilation error: {0}")]
    ShaderError(String),
}
