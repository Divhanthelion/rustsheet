//! Grid rendering - draws grid lines and cell backgrounds

use std::mem;
use wgpu::util::DeviceExt;
use wgpu::{
    BindGroup, Buffer, Device, Queue, RenderPipeline, TextureFormat,
};

use super::renderer::{RenderConfig, RendererError, VisibleRange};

/// Vertex for grid rendering
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GridVertex {
    position: [f32; 2],
    color: [f32; 4],
}

/// Uniform buffer for view transformation
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct ViewUniforms {
    /// View-projection matrix (orthographic)
    view_proj: [[f32; 4]; 4],
    /// Scroll offset
    scroll: [f32; 2],
    /// Scale factor
    scale: f32,
    /// Padding
    _padding: f32,
}

/// Grid line and background renderer
pub struct GridRenderer {
    pipeline: RenderPipeline,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    uniform_buffer: Buffer,
    bind_group: BindGroup,
    num_indices: u32,
    #[allow(dead_code)]
    max_vertices: usize,
}

impl GridRenderer {
    /// Create a new grid renderer
    pub fn new(
        device: &Device,
        format: TextureFormat,
        config: &RenderConfig,
    ) -> Result<Self, RendererError> {
        // Create shader module
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grid_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/grid.wgsl").into()),
        });

        // Create uniform buffer
        let uniforms = ViewUniforms {
            view_proj: Self::create_ortho_matrix(config.width as f32, config.height as f32),
            scroll: [0.0, 0.0],
            scale: 1.0,
            _padding: 0.0,
        };

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("grid_uniforms"),
            contents: bytemuck::cast_slice(&[uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Create bind group layout
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grid_bind_group_layout"),
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

        // Create bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grid_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Create pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grid_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            immediate_size: 0,
        });

        // Create render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grid_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: mem::size_of::<GridVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            offset: 0,
                            shader_location: 0,
                            format: wgpu::VertexFormat::Float32x2,
                        },
                        wgpu::VertexAttribute {
                            offset: mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                            shader_location: 1,
                            format: wgpu::VertexFormat::Float32x4,
                        },
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        // Initial empty buffers
        let max_vertices = 10000;
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid_vertex_buffer"),
            size: (max_vertices * mem::size_of::<GridVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("grid_index_buffer"),
            size: (max_vertices * 6 / 4 * mem::size_of::<u32>()) as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            pipeline,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            bind_group,
            num_indices: 0,
            max_vertices,
        })
    }

    /// Create orthographic projection matrix
    fn create_ortho_matrix(width: f32, height: f32) -> [[f32; 4]; 4] {
        let left = 0.0;
        let right = width;
        let bottom = height;
        let top = 0.0;
        let near = -1.0;
        let far = 1.0;

        [
            [2.0 / (right - left), 0.0, 0.0, 0.0],
            [0.0, 2.0 / (top - bottom), 0.0, 0.0],
            [0.0, 0.0, -2.0 / (far - near), 0.0],
            [
                -(right + left) / (right - left),
                -(top + bottom) / (top - bottom),
                -(far + near) / (far - near),
                1.0,
            ],
        ]
    }

    /// Update uniforms for current frame
    pub fn update_uniforms(
        &self,
        queue: &Queue,
        width: u32,
        height: u32,
        scroll_x: f32,
        scroll_y: f32,
        scale: f32,
    ) {
        let uniforms = ViewUniforms {
            view_proj: Self::create_ortho_matrix(width as f32, height as f32),
            scroll: [scroll_x, scroll_y],
            scale,
            _padding: 0.0,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Build grid line geometry for visible range
    pub fn build_grid_lines(
        &mut self,
        queue: &Queue,
        config: &RenderConfig,
        visible: VisibleRange,
        col_widths: &[f32],
        row_heights: &[f32],
        scroll_x: f32,
        scroll_y: f32,
        scale: f32,
    ) {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let line_color = config.grid_line_color;
        let line_width = config.grid_line_width;

        // Calculate column positions
        let mut col_positions: Vec<f32> = Vec::new();
        let mut x = -scroll_x;
        for col in 0..=visible.end_col {
            if col >= visible.start_col {
                col_positions.push(x);
            }
            let width = col_widths.get(col as usize).copied().unwrap_or(config.default_cell_width);
            x += width * scale;
        }
        col_positions.push(x); // End position

        // Calculate row positions
        let mut row_positions: Vec<f32> = Vec::new();
        let mut y = -scroll_y;
        for row in 0..=visible.end_row {
            if row >= visible.start_row {
                row_positions.push(y);
            }
            let height = row_heights.get(row as usize).copied().unwrap_or(config.default_cell_height);
            y += height * scale;
        }
        row_positions.push(y); // End position

        let viewport_height = config.height as f32;
        let viewport_width = config.width as f32;

        // Vertical lines
        for &col_x in &col_positions {
            if col_x >= 0.0 && col_x <= viewport_width {
                let idx = vertices.len() as u32;
                vertices.push(GridVertex {
                    position: [col_x, 0.0],
                    color: line_color,
                });
                vertices.push(GridVertex {
                    position: [col_x + line_width, 0.0],
                    color: line_color,
                });
                vertices.push(GridVertex {
                    position: [col_x + line_width, viewport_height],
                    color: line_color,
                });
                vertices.push(GridVertex {
                    position: [col_x, viewport_height],
                    color: line_color,
                });
                indices.extend_from_slice(&[idx, idx + 1, idx + 2, idx, idx + 2, idx + 3]);
            }
        }

        // Horizontal lines
        for &row_y in &row_positions {
            if row_y >= 0.0 && row_y <= viewport_height {
                let idx = vertices.len() as u32;
                vertices.push(GridVertex {
                    position: [0.0, row_y],
                    color: line_color,
                });
                vertices.push(GridVertex {
                    position: [viewport_width, row_y],
                    color: line_color,
                });
                vertices.push(GridVertex {
                    position: [viewport_width, row_y + line_width],
                    color: line_color,
                });
                vertices.push(GridVertex {
                    position: [0.0, row_y + line_width],
                    color: line_color,
                });
                indices.extend_from_slice(&[idx, idx + 1, idx + 2, idx, idx + 2, idx + 3]);
            }
        }

        // Upload to GPU
        if !vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
            queue.write_buffer(&self.index_buffer, 0, bytemuck::cast_slice(&indices));
        }
        self.num_indices = indices.len() as u32;
    }

    /// Add a cell background rectangle (internal helper, exposed for custom rendering)
    #[allow(private_interfaces)]
    pub fn add_cell_background(
        vertices: &mut Vec<GridVertex>,
        indices: &mut Vec<u32>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
    ) {
        let idx = vertices.len() as u32;
        vertices.push(GridVertex { position: [x, y], color });
        vertices.push(GridVertex { position: [x + width, y], color });
        vertices.push(GridVertex { position: [x + width, y + height], color });
        vertices.push(GridVertex { position: [x, y + height], color });
        indices.extend_from_slice(&[idx, idx + 1, idx + 2, idx, idx + 2, idx + 3]);
    }

    /// Render the grid
    pub fn render<'a>(&'a self, render_pass: &mut wgpu::RenderPass<'a>) {
        if self.num_indices == 0 {
            return;
        }
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        render_pass.draw_indexed(0..self.num_indices, 0, 0..1);
    }
}
