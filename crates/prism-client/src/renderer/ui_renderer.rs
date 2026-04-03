// SPDX-License-Identifier: AGPL-3.0-or-later
//! Renders PaintContext draw commands with real blurred glass compositing.

use crate::renderer::text_renderer::TextPipeline;
use crate::ui::widgets::{GlowLayer, PaintContext};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct GlowInstance {
    rect: [f32; 4],
    color: [f32; 4],
    corner_radius: f32,
    spread: f32,
    intensity: f32,
    _padding: f32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct GlassInstance {
    rect: [f32; 4],
    blur_rect: [f32; 4],
    tint: [f32; 4],
    border_color: [f32; 4],
    corner_radius: f32,
    noise_intensity: f32,
    backdrop_blur: f32,
    saturation: f32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenUniform {
    screen_size: [f32; 2],
}

const GLOW_SHADER: &str = r#"
struct ScreenUniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> screen: ScreenUniforms;

struct GlowInstance {
    rect: vec4<f32>,
    color: vec4<f32>,
    corner_radius: f32,
    spread: f32,
    intensity: f32,
    _pad0: f32,
}

@group(1) @binding(0) var<storage, read> instances: array<GlowInstance>;

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) quad_size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) corner_radius: f32,
    @location(4) spread: f32,
    @location(5) intensity: f32,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vid: u32,
    @builtin(instance_index) iid: u32,
) -> VertexOut {
    let inst = instances[iid];

    var corner_x: array<f32, 6> = array<f32, 6>(0.0, 1.0, 0.0, 0.0, 1.0, 1.0);
    var corner_y: array<f32, 6> = array<f32, 6>(0.0, 0.0, 1.0, 1.0, 0.0, 1.0);

    let cx = corner_x[vid];
    let cy = corner_y[vid];

    let px = inst.rect.x + cx * inst.rect.z;
    let py = inst.rect.y + cy * inst.rect.w;

    let ndc_x = (px / screen.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / screen.screen_size.y) * 2.0;

    var out: VertexOut;
    out.clip_pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.local_pos = vec2<f32>(cx * inst.rect.z, cy * inst.rect.w);
    out.quad_size = vec2<f32>(inst.rect.z, inst.rect.w);
    out.color = inst.color;
    out.corner_radius = inst.corner_radius;
    out.spread = inst.spread;
    out.intensity = inst.intensity;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let r = in.corner_radius;
    let half = in.quad_size * 0.5;
    let p = in.local_pos - half;
    let q = abs(p) - half + vec2<f32>(r, r);
    let d = length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
    let softness = max(in.spread, 0.001);
    let alpha = exp(-max(d, 0.0) / softness) * in.color.a * in.intensity;
    return vec4<f32>(in.color.rgb, alpha);
}
"#;

const GLASS_SHADER: &str = r#"
struct ScreenUniforms {
    screen_size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> screen: ScreenUniforms;

struct GlassInstance {
    rect: vec4<f32>,
    blur_rect: vec4<f32>,
    tint: vec4<f32>,
    border_color: vec4<f32>,
    corner_radius: f32,
    noise_intensity: f32,
    backdrop_blur: f32,
    saturation: f32,
}

@group(1) @binding(0) var<storage, read> instances: array<GlassInstance>;
@group(2) @binding(0) var blur_tex: texture_2d<f32>;
@group(2) @binding(1) var blur_samp: sampler;

struct VertexOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) local_pos: vec2<f32>,
    @location(1) quad_size: vec2<f32>,
    @location(2) blur_uv: vec2<f32>,
    @location(3) tint: vec4<f32>,
    @location(4) border_color: vec4<f32>,
    @location(5) corner_radius: f32,
    @location(6) noise_intensity: f32,
    @location(7) backdrop_blur: f32,
    @location(8) saturation: f32,
}

fn hash12(p: vec2<f32>) -> f32 {
    let h = dot(p, vec2<f32>(127.1, 311.7));
    return fract(sin(h) * 43758.5453123);
}

@vertex
fn vs_main(
    @builtin(vertex_index) vid: u32,
    @builtin(instance_index) iid: u32,
) -> VertexOut {
    let inst = instances[iid];

    var corner_x: array<f32, 6> = array<f32, 6>(0.0, 1.0, 0.0, 0.0, 1.0, 1.0);
    var corner_y: array<f32, 6> = array<f32, 6>(0.0, 0.0, 1.0, 1.0, 0.0, 1.0);

    let cx = corner_x[vid];
    let cy = corner_y[vid];

    let px = inst.rect.x + cx * inst.rect.z;
    let py = inst.rect.y + cy * inst.rect.w;
    let ndc_x = (px / screen.screen_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / screen.screen_size.y) * 2.0;

    let blur_px = inst.blur_rect.x + cx * inst.blur_rect.z;
    let blur_py = inst.blur_rect.y + cy * inst.blur_rect.w;

    var out: VertexOut;
    out.clip_pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.local_pos = vec2<f32>(cx * inst.rect.z, cy * inst.rect.w);
    out.quad_size = vec2<f32>(inst.rect.z, inst.rect.w);
    out.blur_uv = vec2<f32>(blur_px / screen.screen_size.x, blur_py / screen.screen_size.y);
    out.tint = inst.tint;
    out.border_color = inst.border_color;
    out.corner_radius = inst.corner_radius;
    out.noise_intensity = inst.noise_intensity;
    out.backdrop_blur = inst.backdrop_blur;
    out.saturation = inst.saturation;
    return out;
}

fn saturate_color(color: vec3<f32>, amount: f32) -> vec3<f32> {
    let luma = dot(color, vec3<f32>(0.2126, 0.7152, 0.0722));
    return clamp(mix(vec3<f32>(luma), color, amount), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn sample_backdrop(uv: vec2<f32>, blur_strength: f32) -> vec3<f32> {
    let scaled_blur = blur_strength * 0.14;
    if (scaled_blur <= 0.5) {
        return textureSample(blur_tex, blur_samp, uv).rgb;
    }

    let offset = vec2<f32>(
        scaled_blur / screen.screen_size.x,
        scaled_blur / screen.screen_size.y
    );
    let center = textureSample(blur_tex, blur_samp, uv).rgb * 4.0;
    let horizontal = (
        textureSample(blur_tex, blur_samp, uv + vec2<f32>( offset.x, 0.0)).rgb +
        textureSample(blur_tex, blur_samp, uv + vec2<f32>(-offset.x, 0.0)).rgb
    ) * 2.0;
    let vertical = (
        textureSample(blur_tex, blur_samp, uv + vec2<f32>(0.0,  offset.y)).rgb +
        textureSample(blur_tex, blur_samp, uv + vec2<f32>(0.0, -offset.y)).rgb
    ) * 2.0;
    let diagonal =
        textureSample(blur_tex, blur_samp, uv + offset).rgb +
        textureSample(blur_tex, blur_samp, uv + vec2<f32>( offset.x, -offset.y)).rgb +
        textureSample(blur_tex, blur_samp, uv + vec2<f32>(-offset.x,  offset.y)).rgb +
        textureSample(blur_tex, blur_samp, uv - offset).rgb;
    return (center + horizontal + vertical + diagonal) / 16.0;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let r = in.corner_radius;
    let half = in.quad_size * 0.5;
    let p = abs(in.local_pos - half);
    let q = p - half + vec2<f32>(r, r);
    let d = length(max(q, vec2<f32>(0.0))) - r;

    let edge_alpha = 1.0 - smoothstep(-0.5, 0.5, d);
    if (edge_alpha <= 0.001) {
        discard;
    }

    var color = sample_backdrop(in.blur_uv, in.backdrop_blur);
    color = saturate_color(color, in.saturation);
    color = mix(color, in.tint.rgb, in.tint.a);

    if (in.noise_intensity > 0.001) {
        let noise = hash12(floor(in.local_pos * 0.5));
        color += (noise - 0.5) * in.noise_intensity;
    }

    let border_band = (1.0 - smoothstep(0.0, 1.5, abs(d + 0.75))) * in.border_color.a;
    let top_glint = mix(1.45, 1.0, in.local_pos.y / max(in.quad_size.y, 1.0));
    color = mix(color, in.border_color.rgb * top_glint, border_band);

    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), edge_alpha);
}
"#;

const MAX_GLOW_RECTS: usize = 512;
const MAX_GLASS_QUADS: usize = 512;

/// Renders glass quads, glow rects, and text on top of a blurred backdrop.
pub struct UiRenderer {
    glow_pipeline: wgpu::RenderPipeline,
    glass_pipeline: wgpu::RenderPipeline,
    glow_instance_buffer: wgpu::Buffer,
    glass_instance_buffer: wgpu::Buffer,
    screen_uniform_buffer: wgpu::Buffer,
    screen_bind_group: wgpu::BindGroup,
    glow_instance_bind_group: wgpu::BindGroup,
    glass_instance_bind_group: wgpu::BindGroup,
    backdrop_bind_group_layout: wgpu::BindGroupLayout,
    backdrop_sampler: wgpu::Sampler,
    text_pipeline: TextPipeline,
}

impl UiRenderer {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
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

        let glow_buf_size = (MAX_GLOW_RECTS * std::mem::size_of::<GlowInstance>()) as u64;
        let glow_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-glow-instances"),
            size: glow_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let glass_buf_size = (MAX_GLASS_QUADS * std::mem::size_of::<GlassInstance>()) as u64;
        let glass_instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ui-glass-instances"),
            size: glass_buf_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let glow_instance_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui-glow-instance-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let glass_instance_bgl =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ui-glass-instance-bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let glow_instance_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-glow-instance-bg"),
            layout: &glow_instance_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: glow_instance_buffer.as_entire_binding(),
            }],
        });

        let glass_instance_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-glass-instance-bg"),
            layout: &glass_instance_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: glass_instance_buffer.as_entire_binding(),
            }],
        });

        let backdrop_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("ui-backdrop-bgl"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
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
            });

        let backdrop_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ui-backdrop-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let glow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-glow-shader"),
            source: wgpu::ShaderSource::Wgsl(GLOW_SHADER.into()),
        });
        let glass_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ui-glass-shader"),
            source: wgpu::ShaderSource::Wgsl(GLASS_SHADER.into()),
        });

        let glow_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-glow-pipeline-layout"),
            bind_group_layouts: &[&screen_bgl, &glow_instance_bgl],
            push_constant_ranges: &[],
        });
        let glass_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ui-glass-pipeline-layout"),
            bind_group_layouts: &[
                &screen_bgl,
                &glass_instance_bgl,
                &backdrop_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let glow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-glow-pipeline"),
            layout: Some(&glow_layout),
            vertex: wgpu::VertexState {
                module: &glow_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &glow_shader,
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

        let glass_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ui-glass-pipeline"),
            layout: Some(&glass_layout),
            vertex: wgpu::VertexState {
                module: &glass_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &glass_shader,
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

        let text_pipeline = TextPipeline::new(device, queue, surface_format);

        Self {
            glow_pipeline,
            glass_pipeline,
            glow_instance_buffer,
            glass_instance_buffer,
            screen_uniform_buffer,
            screen_bind_group,
            glow_instance_bind_group,
            glass_instance_bind_group,
            backdrop_bind_group_layout,
            backdrop_sampler,
            text_pipeline,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        backdrop_view: &wgpu::TextureView,
        screen_width: u32,
        screen_height: u32,
        paint_ctx: &PaintContext,
    ) {
        queue.write_buffer(
            &self.screen_uniform_buffer,
            0,
            bytemuck::bytes_of(&ScreenUniform {
                screen_size: [screen_width as f32, screen_height as f32],
            }),
        );

        let glass_instances: Vec<GlassInstance> = paint_ctx
            .glass_quads
            .iter()
            .take(MAX_GLASS_QUADS)
            .map(|gq| GlassInstance {
                rect: [gq.rect.x, gq.rect.y, gq.rect.w, gq.rect.h],
                blur_rect: [
                    gq.blur_rect.x,
                    gq.blur_rect.y,
                    gq.blur_rect.w,
                    gq.blur_rect.h,
                ],
                tint: gq.tint,
                border_color: gq.border_color,
                corner_radius: gq.corner_radius,
                noise_intensity: gq.noise_intensity,
                backdrop_blur: gq.backdrop_blur,
                saturation: gq.saturation,
            })
            .collect();

        let under_glow_instances: Vec<GlowInstance> = paint_ctx
            .glow_rects
            .iter()
            .filter(|gr| gr.layer == GlowLayer::Underlay)
            .take(MAX_GLOW_RECTS)
            .map(|gr| GlowInstance {
                rect: [gr.rect.x, gr.rect.y, gr.rect.w, gr.rect.h],
                color: gr.color,
                corner_radius: gr.corner_radius,
                spread: gr.spread,
                intensity: gr.intensity,
                _padding: 0.0,
            })
            .collect();

        let remaining_capacity = MAX_GLOW_RECTS.saturating_sub(under_glow_instances.len());
        let over_glow_instances: Vec<GlowInstance> = paint_ctx
            .glow_rects
            .iter()
            .filter(|gr| gr.layer == GlowLayer::Overlay)
            .take(remaining_capacity)
            .map(|gr| GlowInstance {
                rect: [gr.rect.x, gr.rect.y, gr.rect.w, gr.rect.h],
                color: gr.color,
                corner_radius: gr.corner_radius,
                spread: gr.spread,
                intensity: gr.intensity,
                _padding: 0.0,
            })
            .collect();
        let glow_instances: Vec<GlowInstance> = under_glow_instances
            .iter()
            .chain(over_glow_instances.iter())
            .copied()
            .collect();

        if !glass_instances.is_empty() {
            queue.write_buffer(
                &self.glass_instance_buffer,
                0,
                bytemuck::cast_slice(&glass_instances),
            );
        }
        if !glow_instances.is_empty() {
            queue.write_buffer(
                &self.glow_instance_buffer,
                0,
                bytemuck::cast_slice(&glow_instances),
            );
        }

        self.text_pipeline
            .prepare(device, queue, screen_width, screen_height, paint_ctx);

        let backdrop_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui-backdrop-bg"),
            layout: &self.backdrop_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(backdrop_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.backdrop_sampler),
                },
            ],
        });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("UI Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });

            if !under_glow_instances.is_empty() {
                pass.set_pipeline(&self.glow_pipeline);
                pass.set_bind_group(0, &self.screen_bind_group, &[]);
                pass.set_bind_group(1, &self.glow_instance_bind_group, &[]);
                pass.draw(0..6, 0..under_glow_instances.len() as u32);
            }

            if !glass_instances.is_empty() {
                pass.set_pipeline(&self.glass_pipeline);
                pass.set_bind_group(0, &self.screen_bind_group, &[]);
                pass.set_bind_group(1, &self.glass_instance_bind_group, &[]);
                pass.set_bind_group(2, &backdrop_bind_group, &[]);
                pass.draw(0..6, 0..glass_instances.len() as u32);
            }

            if !over_glow_instances.is_empty() {
                let base_instance = under_glow_instances.len() as u32;
                let end_instance = base_instance + over_glow_instances.len() as u32;
                pass.set_pipeline(&self.glow_pipeline);
                pass.set_bind_group(0, &self.screen_bind_group, &[]);
                pass.set_bind_group(1, &self.glow_instance_bind_group, &[]);
                pass.draw(0..6, base_instance..end_instance);
            }

            if !paint_ctx.text_runs.is_empty()
                && let Err(e) = self.text_pipeline.render(&mut pass)
            {
                tracing::warn!("glyphon render error: {e:?}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glow_instance_is_48_bytes() {
        assert_eq!(std::mem::size_of::<GlowInstance>(), 48);
    }

    #[test]
    fn glass_instance_is_80_bytes() {
        assert_eq!(std::mem::size_of::<GlassInstance>(), 80);
    }
}
