// SPDX-License-Identifier: AGPL-3.0-or-later
//! Renders PaintContext draw commands (glass quads, glow rects) to the screen.
//!
//! Text rendering via glyphon is deferred until glyphon is upgraded to match the
//! workspace wgpu version. For now, quads and glow rects are rendered as colored
//! rounded rectangles.

use crate::ui::widgets::PaintContext;
use wgpu::util::DeviceExt;

// ---------------------------------------------------------------------------
// Quad instance data
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadInstance {
    rect: [f32; 4],  // x, y, w, h in pixels
    color: [f32; 4], // rgba
    corner_radius: f32,
    _padding: [f32; 3], // align to 48 bytes
}

// ---------------------------------------------------------------------------
// Screen uniform
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenUniform {
    screen_size: [f32; 2],
}

// ---------------------------------------------------------------------------
// Inline WGSL shader for colored rounded-rect quads
// ---------------------------------------------------------------------------

const QUAD_SHADER: &str = r#"
struct ScreenUniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> screen: ScreenUniforms;

struct QuadInstance {
    rect: vec4<f32>,         // x, y, w, h in pixels
    color: vec4<f32>,        // rgba
    corner_radius: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(1) @binding(0) var<storage, read> instances: array<QuadInstance>;

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) local_pos: vec2<f32>,    // position within quad (0..w, 0..h)
    @location(1) quad_size: vec2<f32>,     // w, h
    @location(2) color: vec4<f32>,
    @location(3) corner_radius: f32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vid: u32,
    @builtin(instance_index) iid: u32,
) -> VertexOut {
    let inst = instances[iid];

    // 6 vertices per quad (2 triangles)
    // Triangle 1: 0,1,2  Triangle 2: 3,4,5
    // Corners: TL, TR, BL, BL, TR, BR
    var corner_x: array<f32, 6> = array<f32, 6>(0.0, 1.0, 0.0, 0.0, 1.0, 1.0);
    var corner_y: array<f32, 6> = array<f32, 6>(0.0, 0.0, 1.0, 1.0, 0.0, 1.0);

    let cx = corner_x[vid];
    let cy = corner_y[vid];

    let px = inst.rect.x + cx * inst.rect.z;
    let py = inst.rect.y + cy * inst.rect.w;

    // Convert pixel coords to NDC: x: [0, screen_w] -> [-1, 1], y: [0, screen_h] -> [1, -1]
    let ndc_x = (px / screen.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / screen.screen_size.y) * 2.0;

    var out: VertexOut;
    out.clip_pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.local_pos = vec2<f32>(cx * inst.rect.z, cy * inst.rect.w);
    out.quad_size = vec2<f32>(inst.rect.z, inst.rect.w);
    out.color = inst.color;
    out.corner_radius = inst.corner_radius;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let r = in.corner_radius;
    let half = in.quad_size * 0.5;
    let p = abs(in.local_pos - half);

    // SDF for rounded rectangle
    let q = p - half + vec2<f32>(r, r);
    let d = length(max(q, vec2<f32>(0.0))) - r;

    // Smooth edge (1px antialiasing)
    let alpha = 1.0 - smoothstep(-0.5, 0.5, d);

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
"#;

// ---------------------------------------------------------------------------
// UiRenderer
// ---------------------------------------------------------------------------

const MAX_QUADS: usize = 512;

/// Renders PaintContext draw commands to the screen using wgpu.
///
/// Currently renders glass quads and glow rects as colored rounded rectangles.
/// Text rendering will be added once glyphon is upgraded to match wgpu 24.
pub struct UiRenderer {
    quad_pipeline: wgpu::RenderPipeline,
    quad_instance_buffer: wgpu::Buffer,
    screen_uniform_buffer: wgpu::Buffer,
    screen_bind_group: wgpu::BindGroup,
    #[allow(dead_code)]
    instance_bind_group_layout: wgpu::BindGroupLayout,
    instance_bind_group: wgpu::BindGroup,
}

impl UiRenderer {
    /// Create a new `UiRenderer` for the given device and surface format.
    pub fn new(
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        // ── Screen uniform buffer ─────────────────────────────────────────
        let screen_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("ui-screen-uniforms"),
            contents: bytemuck::bytes_of(&ScreenUniform {
                screen_size: [1.0, 1.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let screen_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-screen-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let screen_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-screen-bg"),
            layout: &screen_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_uniform_buffer.as_entire_binding(),
            }],
        });

        // ── Instance storage buffer ───────────────────────────────────────
        let instance_buf_size = (MAX_QUADS * std::mem::size_of::<QuadInstance>()) as u64;
        let quad_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-quad-instances"),
            size: instance_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let instance_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-instance-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let instance_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-instance-bg"),
            layout: &instance_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: quad_instance_buffer.as_entire_binding(),
            }],
        });

        // ── Quad shader + pipeline ────────────────────────────────────────
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-quad-shader"),
            source: wgpu::ShaderSource::Wgsl(QUAD_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-quad-pipeline-layout"),
            bind_group_layouts: &[&screen_bgl, &instance_bgl],
            push_constant_ranges: &[],
        });

        let quad_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-quad-pipeline"),
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
            multiview: None,
            cache: None,
        });

        Self {
            quad_pipeline,
            quad_instance_buffer,
            screen_uniform_buffer,
            screen_bind_group,
            instance_bind_group_layout: instance_bgl,
            instance_bind_group,
        }
    }

    /// Render all draw commands from `paint_ctx` into the given render target.
    /// The target view should already have been cleared.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        screen_width: u32,
        screen_height: u32,
        paint_ctx: &PaintContext,
    ) {
        // Update screen uniforms
        queue.write_buffer(
            &self.screen_uniform_buffer,
            0,
            bytemuck::bytes_of(&ScreenUniform {
                screen_size: [screen_width as f32, screen_height as f32],
            }),
        );

        // ── Collect quad instances ────────────────────────────────────────
        let mut instances: Vec<QuadInstance> =
            Vec::with_capacity(paint_ctx.glass_quads.len() + paint_ctx.glow_rects.len());

        for gq in &paint_ctx.glass_quads {
            instances.push(QuadInstance {
                rect: [gq.rect.x, gq.rect.y, gq.rect.w, gq.rect.h],
                color: gq.tint,
                corner_radius: gq.corner_radius,
                _padding: [0.0; 3],
            });

            // Draw a 1px border if border_color has visible alpha
            if gq.border_color[3] > 0.01 {
                // Top edge
                instances.push(QuadInstance {
                    rect: [gq.rect.x, gq.rect.y, gq.rect.w, 1.0],
                    color: gq.border_color,
                    corner_radius: 0.0,
                    _padding: [0.0; 3],
                });
                // Bottom edge
                instances.push(QuadInstance {
                    rect: [gq.rect.x, gq.rect.y + gq.rect.h - 1.0, gq.rect.w, 1.0],
                    color: gq.border_color,
                    corner_radius: 0.0,
                    _padding: [0.0; 3],
                });
                // Left edge
                instances.push(QuadInstance {
                    rect: [gq.rect.x, gq.rect.y, 1.0, gq.rect.h],
                    color: gq.border_color,
                    corner_radius: 0.0,
                    _padding: [0.0; 3],
                });
                // Right edge
                instances.push(QuadInstance {
                    rect: [gq.rect.x + gq.rect.w - 1.0, gq.rect.y, 1.0, gq.rect.h],
                    color: gq.border_color,
                    corner_radius: 0.0,
                    _padding: [0.0; 3],
                });
            }
        }

        for gr in &paint_ctx.glow_rects {
            instances.push(QuadInstance {
                rect: [gr.rect.x, gr.rect.y, gr.rect.w, gr.rect.h],
                color: gr.color,
                corner_radius: 0.0,
                _padding: [0.0; 3],
            });
        }

        let num_quads = instances.len().min(MAX_QUADS);

        // Upload instances
        if num_quads > 0 {
            queue.write_buffer(
                &self.quad_instance_buffer,
                0,
                bytemuck::cast_slice(&instances[..num_quads]),
            );
        }

        // ── Render pass ───────────────────────────────────────────────────
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("UI Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // don't clear, render on top
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            // Draw quads
            if num_quads > 0 {
                pass.set_pipeline(&self.quad_pipeline);
                pass.set_bind_group(0, &self.screen_bind_group, &[]);
                pass.set_bind_group(1, &self.instance_bind_group, &[]);
                pass.draw(0..6, 0..num_quads as u32);
            }

            // TODO: text rendering — upgrade glyphon to 0.10+ (wgpu 24 compatible)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad_instance_is_48_bytes() {
        assert_eq!(std::mem::size_of::<QuadInstance>(), 48);
    }
}
