mod dx12_copy;
mod rendering;
mod textures;

use bytemuck::{Pod, Zeroable};
use std::collections::VecDeque;
use std::sync::Arc;
use windows::Win32::Graphics::Direct3D12 as d3d12;

use super::cursors::{CURSOR_ATLAS_COLS, CURSOR_ATLAS_ROWS, CURSOR_TILE_SIZE};
use super::setup::{OUTPUT_TEXTURE_FORMAT, shared_gpu_context};
use super::webcam::WebcamOverlayState;
use dx12_copy::{Dx12SharedCopyContext, READBACK_RING_SIZE};

#[repr(C, align(16))]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct CompositorUniforms {
    pub video_offset: [f32; 2],      // 0-8
    pub video_scale: [f32; 2],       // 8-16
    pub output_size: [f32; 2],       // 16-24
    pub video_size: [f32; 2],        // 24-32
    pub border_radius: f32,          // 32-36
    pub shadow_offset: f32,          // 36-40
    pub shadow_blur: f32,            // 40-44
    pub shadow_opacity: f32,         // 44-48
    pub gradient_color1: [f32; 4],   // 48-64
    pub gradient_color2: [f32; 4],   // 64-80
    pub gradient_color3: [f32; 4],   // 80-96
    pub gradient_color4: [f32; 4],   // 96-112
    pub gradient_color5: [f32; 4],   // 112-128
    pub time: f32,                   // 128-132
    pub render_mode: f32,            // 132-136: 0=all, 1=scene-only, 2=cursor-only
    pub cursor_pos: [f32; 2],        // 136-144
    pub cursor_scale: f32,           // 144-148
    pub cursor_opacity: f32, // 148-152 - cursor visibility (0.0 = hidden, 1.0 = fully visible)
    pub cursor_type_id: f32, // 152-156
    pub cursor_rotation: f32, // 156-160 (radians, tip anchored)
    pub cursor_shadow: f32,  // 160-164 (0-1)
    pub use_background_texture: f32, // 164-168 (0.0=gradient, 1.0=custom texture)
    pub bg_zoom: f32,        // 168-172
    pub bg_anchor_x: f32,    // 172-176
    pub bg_anchor_y: f32,    // 176-180
    pub bg_style: f32, // 180-184 (background family: 0=linear,1=diagonal-glow,2=edge-ribbons,3=stacked-radial,4=prism-fold,5=topographic-flow,6=windowlight-caustics,7=matte-collage,8=orbital-arcs,9=melted-glass)
    pub bg_tex_w: f32, // 184-188 (native texture width for cover UV)
    pub bg_tex_h: f32, // 188-192 (native texture height for cover UV)
    pub bg_params1: [f32; 4], // 192-208
    pub bg_params2: [f32; 4], // 208-224
    pub bg_params3: [f32; 4], // 224-240
    pub bg_params4: [f32; 4], // 240-256
    pub bg_params5: [f32; 4], // 256-272
    pub bg_params6: [f32; 4], // 272-288
}

pub struct GpuCompositor {
    pub(super) device: Arc<wgpu::Device>,
    pub(super) queue: Arc<wgpu::Queue>,
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) accumulate_pipeline: wgpu::RenderPipeline,
    pub(super) vertex_buffer: wgpu::Buffer,
    pub(super) uniform_buffer: wgpu::Buffer,
    pub(super) uniform_bind_group: wgpu::BindGroup,
    pub(super) uniform_alignment: u32,
    pub(super) video_texture: wgpu::Texture,
    pub(super) video_bind_group: wgpu::BindGroup,
    pub(super) cursor_texture: wgpu::Texture,
    pub(super) cursor_bind_group: wgpu::BindGroup,
    pub(super) background_texture: wgpu::Texture,
    pub(super) background_bind_group: wgpu::BindGroup,
    pub(super) background_sampler: wgpu::Sampler,
    pub(super) output_texture: wgpu::Texture,
    pub(super) output_buffers: Vec<wgpu::Buffer>,
    pub(super) readback_receivers:
        Vec<Option<std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>>,
    pub(super) pending_readbacks: VecDeque<usize>,
    pub(super) next_readback_slot: usize,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) background_width: u32,
    pub(super) background_height: u32,
    pub(super) padded_bytes_per_row: u32,
    pub(super) video_width: u32,
    pub(super) video_height: u32,
    // Tiny buffer for cache-flush trick: a 1-pixel copy_buffer_to_texture forces
    // the shared decode texture into COPY_DST state, then copy_texture_to_texture
    // transitions it to COPY_SRC — the COPY_DST→COPY_SRC barrier flushes L2 cache.
    pub(super) cache_flush_buffer: wgpu::Buffer,
    // 1x1 throwaway target used to force shared encode-ring slots through a
    // COPY_DST→COPY_SRC transition after each write (DX12 cache/state flush).
    pub(super) output_state_reset_texture: wgpu::Texture,
    // Raw DX12 copy path for shared decode textures. This bypasses wgpu's texture
    // state tracker and enforces explicit resource transitions around the copy.
    pub(super) dx12_shared_copy: Option<Dx12SharedCopyContext>,
    // Sprite atlas overlay pipeline
    pub(super) atlas_texture: wgpu::Texture,
    pub(super) atlas_bind_group: wgpu::BindGroup,
    pub(super) atlas_sampler: wgpu::Sampler,
    pub(super) overlay_vertex_buffer: wgpu::Buffer,
    pub(super) webcam_overlay: WebcamOverlayState,
}

impl GpuCompositor {
    pub fn new(
        output_width: u32,
        output_height: u32,
        video_width: u32,
        video_height: u32,
        background_width: u32,
        background_height: u32,
    ) -> Result<Self, String> {
        let shared = shared_gpu_context()?;
        let device = Arc::clone(&shared.device);
        let queue = Arc::clone(&shared.queue);
        let pipeline = shared.pipeline.clone();
        let accumulate_pipeline = shared.accumulate_pipeline.clone();
        let vertex_buffer = shared.vertex_buffer.clone();

        // Dynamic offsets must be aligned to the device minimum, and each slot
        // also has to be large enough to fit the full uniform struct.
        let min_uniform_alignment = device.limits().min_uniform_buffer_offset_alignment as usize;
        let uniform_size = std::mem::size_of::<CompositorUniforms>();
        let uniform_alignment =
            uniform_size.div_ceil(min_uniform_alignment) * min_uniform_alignment;
        // Allocate 16 slots (safely covers max 8 blur samples with headroom).
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: (uniform_alignment * 16) as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // BGRA storage matches D3D11 VP output directly (no CPU channel swap).
        // The shader samples through a Bgra8UnormSrgb view for automatic
        // sRGB→linear conversion; wgpu maps .r/.g/.b/.a correctly regardless
        // of the underlying byte order.
        let video_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Video Texture"),
            size: wgpu::Extent3d {
                width: video_width,
                height: video_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[wgpu::TextureFormat::Bgra8UnormSrgb],
        });
        let video_view = video_texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(wgpu::TextureFormat::Bgra8UnormSrgb),
            ..Default::default()
        });

        // Cursor Texture Atlas: (CURSOR_TILE_SIZE*CURSOR_ATLAS_COLS) x (CURSOR_TILE_SIZE*CURSOR_ATLAS_ROWS)
        // 2D atlas keeps 512px cursor tiles while staying under GPU texture limits.
        let cursor_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Cursor Atlas Texture"),
            size: wgpu::Extent3d {
                width: CURSOR_TILE_SIZE * CURSOR_ATLAS_COLS,
                height: CURSOR_TILE_SIZE * CURSOR_ATLAS_ROWS,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let cursor_view = cursor_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let background_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Background Texture"),
            size: wgpu::Extent3d {
                width: background_width.max(1),
                height: background_height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let background_view =
            background_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let video_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        // Cursor sampler: Linear for smooth antialiased edges. The 8x rasterized
        // cursors have AA from tiny_skia; Nearest would destroy sub-pixel smoothing.
        // Atlas tile bleeding is not an issue — cursors are centered in 512x512 tiles
        // with >60px transparent padding on each side.
        let cursor_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        // Background sampler uses linear filtering for smooth scaling.
        let background_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Output Texture"),
            size: wgpu::Extent3d {
                width: output_width,
                height: output_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OUTPUT_TEXTURE_FORMAT,
            // TEXTURE_BINDING needed so D3D12 doesn't set DENY_SHADER_RESOURCE —
            // D3D11On12 wrapping requires it for VideoProcessor BGRA→NV12 input.
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let unpadded_bytes_per_row = output_width * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let output_buffer_size = (padded_bytes_per_row * output_height) as u64;
        let mut output_buffers = Vec::with_capacity(READBACK_RING_SIZE);
        for idx in 0..READBACK_RING_SIZE {
            output_buffers.push(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(&format!("Output Buffer {}", idx)),
                size: output_buffer_size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }));
        }
        let readback_receivers = (0..READBACK_RING_SIZE).map(|_| None).collect();

        let uniform_layout = shared.uniform_layout.clone();
        let texture_layout = shared.texture_layout.clone();
        let background_overlay_layout = shared.background_overlay_layout.clone();

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform BG"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &uniform_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(std::mem::size_of::<CompositorUniforms>() as u64),
                }),
            }],
        });

        let video_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Video BG"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&video_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&video_sampler),
                },
            ],
        });

        let cursor_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Cursor BG"),
            layout: &texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&cursor_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&cursor_sampler),
                },
            ],
        });

        let background_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Background BG"),
            layout: &background_overlay_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&background_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&background_sampler),
                },
            ],
        });

        // Sprite atlas: starts as a 1×1 transparent placeholder; replaced by upload_atlas().
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Atlas Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Atlas BG"),
            layout: &shared.atlas_texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &atlas_texture.create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });
        // 4-byte buffer for cache-flush trick on shared decode textures.
        // copy_buffer_to_texture writes 1 pixel (4 bytes BGRA) to force COPY_DST state.
        let cache_flush_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Cache Flush Buffer"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let output_state_reset_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Output State Reset Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            usage: wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let dx12_shared_copy = unsafe {
            device
                .as_hal::<wgpu::hal::api::Dx12>()
                .and_then(|hal_device| {
                    let raw_device: &d3d12::ID3D12Device = &*(hal_device.raw_device() as *const _);
                    let raw_queue: &d3d12::ID3D12CommandQueue =
                        &*(hal_device.raw_queue() as *const _);
                    Dx12SharedCopyContext::new(raw_device, raw_queue).ok()
                })
        };

        // 1MB vertex buffer — enough for ~8000 quads (6 verts × 24 bytes each).
        let overlay_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Overlay VB"),
            size: 1024 * 1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let webcam_overlay = WebcamOverlayState::new(&device, shared);

        Ok(Self {
            device,
            queue,
            pipeline,
            accumulate_pipeline,
            vertex_buffer,
            uniform_buffer,
            uniform_bind_group,
            uniform_alignment: uniform_alignment as u32,
            video_texture,
            video_bind_group,
            cursor_texture,
            cursor_bind_group,
            background_texture,
            background_bind_group,
            background_sampler,
            output_texture,
            output_buffers,
            readback_receivers,
            pending_readbacks: VecDeque::with_capacity(READBACK_RING_SIZE),
            next_readback_slot: 0,
            width: output_width,
            height: output_height,
            background_width: background_width.max(1),
            background_height: background_height.max(1),
            padded_bytes_per_row,
            video_width,
            video_height,
            cache_flush_buffer,
            output_state_reset_texture,
            dx12_shared_copy,
            atlas_texture,
            atlas_bind_group,
            atlas_sampler,
            overlay_vertex_buffer,
            webcam_overlay,
        })
    }

    /// Get a reference to the wgpu device (needed by pipeline for HAL interop).
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Get a reference to the wgpu queue.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }
}
