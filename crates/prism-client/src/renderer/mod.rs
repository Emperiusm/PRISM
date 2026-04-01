// SPDX-License-Identifier: AGPL-3.0-or-later
//! wgpu-based renderer for PRISM client — stream texture, blur, glass panels, text.

pub mod animation;
pub mod blur_pipeline;
pub mod glass_panel;
pub mod shader_cache;
pub mod stream_texture;
pub mod text_renderer;

use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

/// Screen-size uniform passed to shaders so they can convert pixel coords to NDC.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ScreenUniforms {
    screen_size: [f32; 2],
}

/// Core wgpu renderer — owns the GPU device, surface, and stream render pipeline.
#[allow(dead_code)]
pub struct PrismRenderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub stream_pipeline: wgpu::RenderPipeline,
    pub stream_bind_group_layout: wgpu::BindGroupLayout,
    pub screen_uniform_buffer: wgpu::Buffer,
    pub screen_bind_group: wgpu::BindGroup,
    window: Arc<Window>,
}

impl PrismRenderer {
    /// Initialise wgpu, create the surface, and build the stream render pipeline.
    pub async fn new(window: Arc<Window>) -> Result<Self, Box<dyn std::error::Error>> {
        let size = window.inner_size();

        // ── Instance & surface ────────────────────────────────────────────────
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        // Safety: the surface must not outlive the window. We hold Arc<Window> for
        // the lifetime of PrismRenderer, so the window lives at least as long as
        // the surface.
        let surface = instance.create_surface(Arc::clone(&window))?;

        // ── Adapter ───────────────────────────────────────────────────────────
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or("no suitable wgpu adapter found")?;

        // ── Device & queue ────────────────────────────────────────────────────
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("prism-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await?;

        // ── Surface configuration ─────────────────────────────────────────────
        let caps = surface.get_capabilities(&adapter);
        let surface_format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        // ── Screen uniform buffer ─────────────────────────────────────────────
        let screen_uniforms = ScreenUniforms {
            screen_size: [size.width as f32, size.height as f32],
        };
        let screen_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("screen-uniforms"),
            contents: bytemuck::bytes_of(&screen_uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ── Screen bind group layout (group 0) ────────────────────────────────
        let screen_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("screen-bgl"),
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
            label: Some("screen-bg"),
            layout: &screen_bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: screen_uniform_buffer.as_entire_binding(),
            }],
        });

        // ── Stream bind group layout (group 1) ────────────────────────────────
        let stream_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stream-bgl"),
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

        // ── Stream render pipeline ─────────────────────────────────────────────
        // Vertex shader: fullscreen triangle (no vertex buffer required).
        // Fragment shader: passthrough texture sample from stream_tex.
        let stream_vs_src = r#"
struct ScreenUniforms { screen_size: vec2<f32> }
@group(0) @binding(0) var<uniform> screen: ScreenUniforms;

struct FragIn {
    @builtin(position)            clip_pos:    vec4<f32>,
    @location(0)                  uv:          vec2<f32>,
    @location(1)                  screen_pos:  vec2<f32>,
    @location(2) @interpolate(flat) instance_id: u32,
}

@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> FragIn {
    // Fullscreen triangle covering the entire viewport.
    let uv = vec2<f32>(
        f32((vid << 1u) & 2u),
        f32( vid        & 2u),
    );
    let clip = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    var out: FragIn;
    out.clip_pos    = vec4<f32>(clip, 0.0, 1.0);
    out.uv          = uv;
    out.screen_pos  = uv * screen.screen_size;
    out.instance_id = 0u;
    return out;
}
"#;

        let stream_fs_src = r#"
@group(1) @binding(0) var stream_tex:     texture_2d<f32>;
@group(1) @binding(1) var stream_sampler: sampler;

struct FragIn {
    @builtin(position)              clip_pos:    vec4<f32>,
    @location(0)                    uv:          vec2<f32>,
    @location(1)                    screen_pos:  vec2<f32>,
    @location(2) @interpolate(flat) instance_id: u32,
}

@fragment
fn fs_stream(in: FragIn) -> @location(0) vec4<f32> {
    return textureSample(stream_tex, stream_sampler, in.uv);
}
"#;

        // Combine into a single WGSL module so wgpu sees both entry points.
        let stream_shader_src = format!("{}\n{}", stream_vs_src, stream_fs_src);

        let stream_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stream-shader"),
            source: wgpu::ShaderSource::Wgsl(stream_shader_src.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("stream-pipeline-layout"),
            bind_group_layouts: &[&screen_bgl, &stream_bgl],
            push_constant_ranges: &[],
        });

        let stream_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stream-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &stream_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &stream_shader,
                entry_point: Some("fs_stream"),
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

        Ok(Self {
            device,
            queue,
            surface,
            surface_config,
            stream_pipeline,
            stream_bind_group_layout: stream_bgl,
            screen_uniform_buffer,
            screen_bind_group,
            window,
        })
    }

    /// Resize the surface. No-ops if either dimension is zero.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);
        let uniforms = ScreenUniforms {
            screen_size: [width as f32, height as f32],
        };
        self.queue.write_buffer(
            &self.screen_uniform_buffer,
            0,
            bytemuck::bytes_of(&uniforms),
        );
    }

    /// Current surface width in pixels.
    pub fn width(&self) -> u32 {
        self.surface_config.width
    }

    /// Current surface height in pixels.
    pub fn height(&self) -> u32 {
        self.surface_config.height
    }

    /// The texture format of the configured surface.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_config.format
    }
}
