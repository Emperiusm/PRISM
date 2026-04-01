// SPDX-License-Identifier: AGPL-3.0-or-later
//! Two-pass separable Gaussian blur at progressive resolutions.

use wgpu::util::DeviceExt;

/// Uniform buffer layout matched to the WGSL `BlurUniforms` struct.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurUniforms {
    direction: [f32; 2],
    _padding: [f32; 2],
}

/// Two-pass separable Gaussian blur operating at quarter resolution.
///
/// Pass 1 (horizontal) renders into `intermediate_texture`.
/// Pass 2 (vertical)   renders into `output_texture`.
///
/// The caller provides a `wgpu::BindGroup` for the source texture on every
/// [`BlurPipeline::run`] call; the pipeline owns all intermediate state.
#[allow(dead_code)]
pub struct BlurPipeline {
    h_pipeline: wgpu::RenderPipeline,
    v_pipeline: wgpu::RenderPipeline,
    intermediate_texture: wgpu::Texture,
    intermediate_view: wgpu::TextureView,
    output_texture: wgpu::Texture,
    output_view: wgpu::TextureView,
    h_bind_group: wgpu::BindGroup,
    v_bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    h_uniform_buffer: wgpu::Buffer,
    v_uniform_buffer: wgpu::Buffer,
    pub width: u32,
    pub height: u32,
}

impl BlurPipeline {
    /// Create a new `BlurPipeline` for a source image of `source_width × source_height`.
    ///
    /// Both passes render into quarter-resolution `Rgba8Unorm` textures.
    pub fn new(
        device: &wgpu::Device,
        source_width: u32,
        source_height: u32,
        _surface_format: wgpu::TextureFormat,
    ) -> Self {
        // ── Quarter resolution ────────────────────────────────────────────────
        let width = source_width.div_ceil(4);
        let height = source_height.div_ceil(4);

        // ── Intermediate and output textures ──────────────────────────────────
        let blur_format = wgpu::TextureFormat::Rgba8Unorm;
        let texture_usage =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;

        let intermediate_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blur-intermediate"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: blur_format,
            usage: texture_usage,
            view_formats: &[],
        });
        let intermediate_view =
            intermediate_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("blur-output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: blur_format,
            usage: texture_usage,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // ── Sampler (linear, clamp) ────────────────────────────────────────────
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blur-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // ── Uniform buffers ───────────────────────────────────────────────────
        let h_uniforms = BlurUniforms {
            direction: [1.0 / width as f32, 0.0],
            _padding: [0.0; 2],
        };
        let h_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blur-h-uniforms"),
            contents: bytemuck::bytes_of(&h_uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let v_uniforms = BlurUniforms {
            direction: [0.0, 1.0 / height as f32],
            _padding: [0.0; 2],
        };
        let v_uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blur-v-uniforms"),
            contents: bytemuck::bytes_of(&v_uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ── Bind group layout ─────────────────────────────────────────────────
        // binding 0: texture_2d<f32>
        // binding 1: sampler
        // binding 2: uniform buffer (BlurUniforms)
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur-bgl"),
            entries: &[
                // texture
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
                // sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // uniform buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // ── Shader ────────────────────────────────────────────────────────────
        let shader_src = include_str!("shaders/blur.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blur-shader"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        // ── Pipeline layout ───────────────────────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur-pipeline-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        // ── Helper to build a render pipeline (H and V are identical) ─────────
        let make_pipeline = |label: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
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
                        format: blur_format,
                        blend: None,
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
            })
        };

        let h_pipeline = make_pipeline("blur-h-pipeline");
        let v_pipeline = make_pipeline("blur-v-pipeline");

        // ── H bind group — placeholder: uses intermediate_view as source ──────
        // In practice, pass 1 receives an external bind group each frame;
        // this stored bind group is unused at runtime but satisfies the struct.
        let h_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur-h-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&intermediate_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: h_uniform_buffer.as_entire_binding(),
                },
            ],
        });

        // ── V bind group — reads from intermediate_view ───────────────────────
        let v_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur-v-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&intermediate_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: v_uniform_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            h_pipeline,
            v_pipeline,
            intermediate_texture,
            intermediate_view,
            output_texture,
            output_view,
            h_bind_group,
            v_bind_group,
            sampler,
            h_uniform_buffer,
            v_uniform_buffer,
            width,
            height,
        }
    }

    /// Execute both blur passes.
    ///
    /// `input_bind_group` must be a bind group compatible with the blur bind
    /// group layout (texture @ 0, sampler @ 1, uniform @ 2) where the uniform
    /// contains the horizontal direction — or simply a bind group created with
    /// [`BlurPipeline::make_input_bind_group`].
    ///
    /// Typically the caller builds an input bind group that wraps the downscaled
    /// stream texture and the horizontal uniform buffer.
    pub fn run(&self, encoder: &mut wgpu::CommandEncoder, input_bind_group: &wgpu::BindGroup) {
        // ── Pass 1: horizontal blur → intermediate ────────────────────────────
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blur-h-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.intermediate_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.h_pipeline);
            pass.set_bind_group(0, input_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        // ── Pass 2: vertical blur → output ─────────────────────────────────
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blur-v-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.v_pipeline);
            pass.set_bind_group(0, &self.v_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
    }

    /// A view into the blurred output texture, ready for compositing.
    pub fn output_view(&self) -> &wgpu::TextureView {
        &self.output_view
    }
}
