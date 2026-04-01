// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ring-buffered YUV plane upload with compute shader YUV→RGB conversion.

use wgpu::util::DeviceExt;

/// YUV parameters uniform — dimensions passed to the compute shader.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct YuvParams {
    width: u32,
    height: u32,
}

/// Double-buffered YUV texture set with GPU compute YUV→RGBA conversion.
///
/// The ring buffer (slot 0 / slot 1) allows the CPU to upload to one slot
/// while the GPU reads from the other, eliminating CPU/GPU stalls.
#[allow(dead_code)] // output_texture / params_buffer are owner-only (RAII lifetime guards)
pub struct StreamTexture {
    y_texture: [wgpu::Texture; 2],
    u_texture: [wgpu::Texture; 2],
    v_texture: [wgpu::Texture; 2],
    output_texture: wgpu::Texture,
    output_view: wgpu::TextureView,
    compute_pipeline: wgpu::ComputePipeline,
    bind_groups: [wgpu::BindGroup; 2],
    params_buffer: wgpu::Buffer,
    current_slot: usize,
    pub width: u32,
    pub height: u32,
    dirty: bool,
}

impl StreamTexture {
    /// Create a new `StreamTexture` sized `width × height` pixels.
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let half_w = width.div_ceil(2);
        let half_h = height.div_ceil(2);

        // ── Y plane textures (full resolution, R8Unorm) ───────────────────────
        let y_texture = std::array::from_fn(|i| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("y-plane-{i}")),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        });

        // ── U plane textures (half resolution, R8Unorm) ───────────────────────
        let u_texture = std::array::from_fn(|i| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("u-plane-{i}")),
                size: wgpu::Extent3d {
                    width: half_w,
                    height: half_h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        });

        // ── V plane textures (half resolution, R8Unorm) ───────────────────────
        let v_texture = std::array::from_fn(|i| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("v-plane-{i}")),
                size: wgpu::Extent3d {
                    width: half_w,
                    height: half_h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        });

        // ── RGBA output texture ───────────────────────────────────────────────
        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("yuv-output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // ── Params uniform buffer ─────────────────────────────────────────────
        let params = YuvParams { width, height };
        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("yuv-params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ── Bind group layout ─────────────────────────────────────────────────
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("yuv-compute-bgl"),
            entries: &[
                bgl_texture(0),
                bgl_texture(1),
                bgl_texture(2),
                // binding 3: storage texture write-only (RGBA output)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                // binding 4: uniform buffer (YuvParams)
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // ── Compute pipeline ──────────────────────────────────────────────────
        let shader_src = include_str!("shaders/yuv_to_rgb.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("yuv-to-rgb-cs"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("yuv-compute-layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("yuv-compute-pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("cs_main"),
            compilation_options: Default::default(),
            cache: None,
        });

        // ── Bind groups (one per ring slot) ───────────────────────────────────
        let bind_groups = std::array::from_fn(|slot| {
            let y_view = y_texture[slot].create_view(&wgpu::TextureViewDescriptor::default());
            let u_view = u_texture[slot].create_view(&wgpu::TextureViewDescriptor::default());
            let v_view = v_texture[slot].create_view(&wgpu::TextureViewDescriptor::default());

            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("yuv-bg-{slot}")),
                layout: &bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&y_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&u_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&v_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&output_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: params_buffer.as_entire_binding(),
                    },
                ],
            })
        });

        Self {
            y_texture,
            u_texture,
            v_texture,
            output_texture,
            output_view,
            compute_pipeline,
            bind_groups,
            params_buffer,
            current_slot: 0,
            width,
            height,
            dirty: false,
        }
    }

    /// Upload a YUV420 frame. Writes to the *opposite* ring slot, then swaps.
    pub fn upload_yuv(&mut self, queue: &wgpu::Queue, y_data: &[u8], u_data: &[u8], v_data: &[u8]) {
        let write_slot = 1 - self.current_slot;
        let half_w = self.width.div_ceil(2);
        let half_h = self.height.div_ceil(2);

        upload_plane(
            queue,
            &self.y_texture[write_slot],
            y_data,
            self.width,
            self.height,
        );
        upload_plane(queue, &self.u_texture[write_slot], u_data, half_w, half_h);
        upload_plane(queue, &self.v_texture[write_slot], v_data, half_w, half_h);

        self.current_slot = write_slot;
        self.dirty = true;
    }

    /// Dispatch the compute pass to convert the current slot's YUV planes to RGBA.
    /// No-ops if no new frame has been uploaded since the last convert.
    pub fn convert(&mut self, encoder: &mut wgpu::CommandEncoder) {
        if !self.dirty {
            return;
        }

        let wg_x = self.width.div_ceil(16);
        let wg_y = self.height.div_ceil(16);

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("yuv-to-rgb"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.compute_pipeline);
            pass.set_bind_group(0, &self.bind_groups[self.current_slot], &[]);
            pass.dispatch_workgroups(wg_x, wg_y, 1);
        }

        self.dirty = false;
    }

    /// A view into the RGBA output texture, suitable for use in a render bind group.
    pub fn output_view(&self) -> &wgpu::TextureView {
        &self.output_view
    }

    /// Whether a new frame is waiting to be converted.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Upload raw bytes into a single-channel R8 plane texture.
fn upload_plane(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    data: &[u8],
    width: u32,
    height: u32,
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

/// Build a compute-visible, non-filterable float texture2d bind group layout entry.
fn bgl_texture(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
