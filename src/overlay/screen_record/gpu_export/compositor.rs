use bytemuck::{Pod, Zeroable};
use std::collections::VecDeque;
use std::sync::Arc;

use super::cursors::{
    dedupe_valid_slots, get_or_render_cursor_tile, CURSOR_ATLAS_COLS, CURSOR_ATLAS_ROWS,
    CURSOR_TILE_SIZE,
};
use super::setup::{shared_gpu_context, OverlayVertex, OUTPUT_TEXTURE_FORMAT};
use crate::overlay::screen_record::native_export::config::OverlayQuad;

const READBACK_RING_SIZE: usize = 5;

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
    pub time: f32,                   // 80-84
    pub render_mode: f32,            // 84-88: 0=all, 1=scene-only, 2=cursor-only
    pub cursor_pos: [f32; 2],        // 88-96
    pub cursor_scale: f32,           // 96-100
    pub cursor_opacity: f32, // 100-104 - cursor visibility (0.0 = hidden, 1.0 = fully visible)
    pub cursor_type_id: f32, // 104-108
    pub cursor_rotation: f32, // 108-112 (radians, tip anchored)
    pub cursor_shadow: f32,  // 112-116 (0-1)
    pub use_background_texture: f32, // 116-120 (0.0=gradient, 1.0=custom texture)
    pub bg_zoom: f32,        // 120-124
    pub bg_anchor_x: f32,    // 124-128
    pub bg_anchor_y: f32,    // 128-132
    pub bg_style: f32,       // 132-136 (gradient variant: 0=none,1=g4,2=g5,3=g6,4=g7)
    pub bg_tex_w: f32,       // 136-140 (native texture width for cover UV)
    pub bg_tex_h: f32,       // 140-144 (native texture height for cover UV)
}

pub struct GpuCompositor {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    pipeline: wgpu::RenderPipeline,
    accumulate_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    uniform_alignment: u32,
    video_texture: wgpu::Texture,
    video_bind_group: wgpu::BindGroup,
    cursor_texture: wgpu::Texture,
    cursor_bind_group: wgpu::BindGroup,
    background_texture: wgpu::Texture,
    background_bind_group: wgpu::BindGroup,
    background_sampler: wgpu::Sampler,
    output_texture: wgpu::Texture,
    output_buffers: Vec<wgpu::Buffer>,
    readback_receivers: Vec<Option<std::sync::mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>>,
    pending_readbacks: VecDeque<usize>,
    next_readback_slot: usize,
    width: u32,
    height: u32,
    background_width: u32,
    background_height: u32,
    padded_bytes_per_row: u32,
    video_width: u32,
    video_height: u32,
    // Sprite atlas overlay pipeline
    atlas_texture: wgpu::Texture,
    atlas_bind_group: wgpu::BindGroup,
    atlas_sampler: wgpu::Sampler,
    overlay_vertex_buffer: wgpu::Buffer,
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

        // Align to device requirement for dynamic uniform buffer offsets.
        // Allocate 16 slots (safely covers max 8 blur samples with headroom).
        let uniform_alignment = device.limits().min_uniform_buffer_offset_alignment;
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: (uniform_alignment as usize * 16) as u64,
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
            mipmap_filter: wgpu::FilterMode::Linear,
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
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // Background sampler uses linear filtering for smooth scaling.
        let background_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
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
                    size: wgpu::BufferSize::new(
                        std::mem::size_of::<CompositorUniforms>() as u64,
                    ),
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
            mipmap_filter: wgpu::FilterMode::Linear,
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
        // 1MB vertex buffer — enough for ~8000 quads (6 verts × 24 bytes each).
        let overlay_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Overlay VB"),
            size: 1024 * 1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Ok(Self {
            device,
            queue,
            pipeline,
            accumulate_pipeline,
            vertex_buffer,
            uniform_buffer,
            uniform_bind_group,
            uniform_alignment,
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
            atlas_texture,
            atlas_bind_group,
            atlas_sampler,
            overlay_vertex_buffer,
        })
    }

    pub fn upload_cursor_slot_rgba(&self, slot: u32, rgba: &[u8]) {
        let col = slot % CURSOR_ATLAS_COLS;
        let row = slot / CURSOR_ATLAS_COLS;

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.cursor_texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: col * CURSOR_TILE_SIZE,
                    y: row * CURSOR_TILE_SIZE,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(CURSOR_TILE_SIZE * 4),
                rows_per_image: Some(CURSOR_TILE_SIZE),
            },
            wgpu::Extent3d {
                width: CURSOR_TILE_SIZE,
                height: CURSOR_TILE_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn init_cursor_texture_fast(&self, slots: &[u32]) -> bool {
        for slot in dedupe_valid_slots(slots) {
            if let Some(tile) = get_or_render_cursor_tile(slot) {
                self.upload_cursor_slot_rgba(slot, tile.as_slice());
            }
        }
        false
    }

    pub fn upload_frame(&self, rgba_data: &[u8]) {
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.video_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.video_width * 4),
                rows_per_image: Some(self.video_height),
            },
            wgpu::Extent3d {
                width: self.video_width,
                height: self.video_height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn upload_background(&mut self, rgba_data: &[u8], width: u32, height: u32) {
        if width == 0 || height == 0 || rgba_data.is_empty() {
            return;
        }
        let shared = match shared_gpu_context() {
            Ok(s) => s,
            Err(_) => return,
        };

        // Recreate texture at native image dimensions (no CPU pre-scaling needed).
        self.background_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Background Texture Loaded"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.background_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.background_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Background BG"),
            layout: &shared.background_overlay_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &self
                            .background_texture
                            .create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.background_sampler),
                },
            ],
        });
        self.background_width = width;
        self.background_height = height;
    }

    pub fn render_to_output(&self, uniforms: &CompositorUniforms, clear: bool) {
        let uniform_data = bytemuck::bytes_of(uniforms);
        self.queue
            .write_buffer(&self.uniform_buffer, 0, uniform_data);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        {
            let load_op = if clear {
                wgpu::LoadOp::Clear(wgpu::Color::BLACK)
            } else {
                wgpu::LoadOp::Load
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self
                        .output_texture
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[0]);
            pass.set_bind_group(1, &self.video_bind_group, &[]);
            pass.set_bind_group(2, &self.cursor_bind_group, &[]);
            pass.set_bind_group(3, &self.background_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    fn copy_output_to_readback_slot(&self, slot: usize) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.output_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &self.output_buffers[slot],
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(self.padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    pub fn enqueue_output_readback(&mut self) -> Result<(), String> {
        if self.pending_readbacks.len() >= self.output_buffers.len() {
            return Err("Readback ring overflow: pending frames were not drained".to_string());
        }

        let slot = self.next_readback_slot;
        self.next_readback_slot = (self.next_readback_slot + 1) % self.output_buffers.len();
        if self
            .pending_readbacks
            .iter()
            .any(|pending| *pending == slot)
        {
            return Err("Readback slot reuse before previous map completed".to_string());
        }

        self.copy_output_to_readback_slot(slot);
        let buffer_slice = self.output_buffers[slot].slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.readback_receivers[slot] = Some(rx);
        self.pending_readbacks.push_back(slot);
        Ok(())
    }

    fn copy_slot_into_vec(&self, slot: usize, out: &mut Vec<u8>) {
        let buffer_slice = self.output_buffers[slot].slice(..);
        let data = buffer_slice.get_mapped_range();
        let unpadded = self.width * 4;
        if self.padded_bytes_per_row == unpadded {
            out.clear();
            out.extend_from_slice(&data);
        } else {
            out.clear();
            out.reserve((unpadded * self.height) as usize);
            for row in data.chunks(self.padded_bytes_per_row as usize) {
                out.extend_from_slice(&row[..unpadded as usize]);
            }
        }
        drop(data);
        self.output_buffers[slot].unmap();
    }

    fn drain_next_readback(&mut self, out: &mut Vec<u8>, blocking: bool) -> Result<bool, String> {
        let _ = self.device.poll(if blocking {
            wgpu::PollType::Wait
        } else {
            wgpu::PollType::Poll
        });

        let Some(&slot) = self.pending_readbacks.front() else {
            return Ok(false);
        };

        let map_status = {
            let rx = self.readback_receivers[slot]
                .as_ref()
                .ok_or_else(|| "Missing readback receiver".to_string())?;
            if blocking {
                match rx.recv() {
                    Ok(result) => Some(result),
                    Err(err) => return Err(format!("GPU readback channel failed: {}", err)),
                }
            } else {
                match rx.try_recv() {
                    Ok(result) => Some(result),
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        return Err("GPU readback channel disconnected".to_string())
                    }
                }
            }
        };

        let Some(status) = map_status else {
            return Ok(false);
        };
        status.map_err(|e| format!("GPU buffer map failed: {}", e))?;

        self.readback_receivers[slot] = None;
        let _ = self.pending_readbacks.pop_front();
        self.copy_slot_into_vec(slot, out);
        Ok(true)
    }

    pub fn readback_output(&mut self, out: &mut Vec<u8>) -> Result<(), String> {
        let _ = self.drain_next_readback(out, true)?;
        Ok(())
    }

    pub fn render_frame_enqueue_readback(
        &mut self,
        uniforms: &CompositorUniforms,
    ) -> Result<(), String> {
        self.render_to_output(uniforms, true);
        self.enqueue_output_readback()
    }

    pub fn render_frame_into(
        &mut self,
        uniforms: &CompositorUniforms,
        out: &mut Vec<u8>,
    ) -> Result<(), String> {
        self.render_frame_enqueue_readback(uniforms)?;
        self.readback_output(out)
    }

    pub fn render_frame(&mut self, uniforms: &CompositorUniforms) -> Vec<u8> {
        let mut out = Vec::with_capacity((self.width * self.height * 4) as usize);
        let _ = self.render_frame_into(uniforms, &mut out);
        out
    }

    /// Run all motion blur sub-frames in a single RenderPass with one queue.submit().
    ///
    /// Each pass updates the uniform buffer offset (dynamic offset) and blend constant
    /// between draw calls — no encoder recreation overhead per pass. This replaces
    /// N separate encoder+submit cycles with 1, cutting ~0.2ms × N overhead from
    /// every motion-blur frame.
    pub fn render_accumulate_batched(&self, passes: &[(CompositorUniforms, f64)]) {
        if passes.is_empty() {
            return;
        }
        let n = passes.len().min(16);
        let alignment = self.uniform_alignment as usize;

        // Write all N uniform structs into the aligned buffer slots upfront.
        let mut staging = vec![0u8; n * alignment];
        for (i, (uniforms, _)) in passes[..n].iter().enumerate() {
            let data = bytemuck::bytes_of(uniforms);
            staging[i * alignment..i * alignment + data.len()].copy_from_slice(data);
        }
        self.queue.write_buffer(&self.uniform_buffer, 0, &staging);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Create the view once — reused across all N passes.
        let view = self
            .output_texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // N separate RenderPasses inside the same CommandEncoder.
        //
        // A single RenderPass with N draw calls and a changing blend_constant triggers a
        // DX12 driver bug: the ROP tile cache doesn't flush between draws when only the
        // blend constant changes, so draw i+1's blend DST reads the cleared value instead
        // of draw i's committed output → "back-and-forth frame" corruption.
        //
        // Ending each RenderPass forces a DX12 resource barrier / ROP flush before the
        // next LoadOp::Load, guaranteeing correct sequential accumulation.
        // CPU overhead is negligible (begin/end_render_pass is near-zero); the key saving
        // (single CommandEncoder + single queue.submit) is fully preserved.
        for (i, (_, weight)) in passes[..n].iter().enumerate() {
            let load_op = if i == 0 {
                wgpu::LoadOp::Clear(wgpu::Color::BLACK)
            } else {
                wgpu::LoadOp::Load
            };

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            pass.set_pipeline(&self.accumulate_pipeline);
            pass.set_bind_group(1, &self.video_bind_group, &[]);
            pass.set_bind_group(2, &self.cursor_bind_group, &[]);
            pass.set_bind_group(3, &self.background_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_blend_constant(wgpu::Color {
                r: *weight,
                g: *weight,
                b: *weight,
                a: *weight,
            });
            pass.set_bind_group(0, &self.uniform_bind_group, &[(i * alignment) as u32]);
            pass.draw(0..6, 0..1);
            // pass drops here → EndRenderPass → DX12 ROP flush → next LoadOp::Load sees committed result
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Upload the sprite atlas RGBA pixels and rebuild the bind group.
    /// Call once before the pipeline starts.
    pub fn upload_atlas(&mut self, rgba_data: &[u8], width: u32, height: u32) {
        if width == 0 || height == 0 || rgba_data.is_empty() {
            return;
        }
        let shared = match shared_gpu_context() {
            Ok(s) => s,
            Err(_) => return,
        };
        self.atlas_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Atlas Texture Loaded"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.atlas_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Atlas BG"),
            layout: &shared.atlas_texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &self
                            .atlas_texture
                            .create_view(&wgpu::TextureViewDescriptor::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.atlas_sampler),
                },
            ],
        });
    }

    /// Copy a shared decode texture into the video input texture (GPU-to-GPU).
    ///
    /// Replaces the CPU `upload_frame()` path: instead of PCIe download + upload,
    /// this is an internal GPU memory copy (~0.1ms at 4K vs ~6ms for CPU round-trip).
    pub fn copy_frame_from_shared(&self, source: &wgpu::Texture) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Copy Decode to Video"),
            });

        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &self.video_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: self.video_width,
                height: self.video_height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Copy the output texture to a shared wgpu texture (GPU-to-GPU, no PCIe bus).
    ///
    /// Used by the zero-copy pipeline: after rendering, the output is copied to a
    /// shared texture that the D3D11 encode device can read directly via DXGI interop.
    pub fn copy_output_to_shared(&self, target: &wgpu::Texture) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Copy to Shared"),
            });

        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.output_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Get a reference to the wgpu device (needed by pipeline for HAL interop).
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Get a reference to the wgpu queue.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Draw atlas quads directly onto the output texture (called after the main render pass).
    /// Uses premultiplied-alpha blending so transparent quads compose correctly.
    pub fn render_overlays(&self, quads: &[OverlayQuad]) {
        if quads.is_empty() {
            return;
        }
        let shared = match shared_gpu_context() {
            Ok(s) => s,
            Err(_) => return,
        };

        let out_w = self.width as f32;
        let out_h = self.height as f32;
        let mut vertices: Vec<OverlayVertex> = Vec::with_capacity(quads.len() * 6);

        for q in quads {
            let x1 = (q.x / out_w) * 2.0 - 1.0;
            let y1 = 1.0 - (q.y / out_h) * 2.0;
            let x2 = ((q.x + q.w) / out_w) * 2.0 - 1.0;
            let y2 = 1.0 - ((q.y + q.h) / out_h) * 2.0;
            let u1 = q.u;
            let v1 = q.v;
            let u2 = q.u + q.uw;
            let v2 = q.v + q.vh;
            let a = q.alpha;
            // Two triangles (CCW)
            vertices.push(OverlayVertex {
                pos: [x1, y1],
                uv: [u1, v1],
                alpha: a,
                _pad: 0.0,
            });
            vertices.push(OverlayVertex {
                pos: [x2, y1],
                uv: [u2, v1],
                alpha: a,
                _pad: 0.0,
            });
            vertices.push(OverlayVertex {
                pos: [x1, y2],
                uv: [u1, v2],
                alpha: a,
                _pad: 0.0,
            });
            vertices.push(OverlayVertex {
                pos: [x2, y1],
                uv: [u2, v1],
                alpha: a,
                _pad: 0.0,
            });
            vertices.push(OverlayVertex {
                pos: [x2, y2],
                uv: [u2, v2],
                alpha: a,
                _pad: 0.0,
            });
            vertices.push(OverlayVertex {
                pos: [x1, y2],
                uv: [u1, v2],
                alpha: a,
                _pad: 0.0,
            });
        }

        let byte_len = (vertices.len() * std::mem::size_of::<OverlayVertex>()) as u64;
        if byte_len > self.overlay_vertex_buffer.size() {
            return; // Buffer too small — skip rather than panic
        }
        self.queue.write_buffer(
            &self.overlay_vertex_buffer,
            0,
            bytemuck::cast_slice(&vertices),
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let view = self
                .output_texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // composite onto existing output
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&shared.overlay_pipeline);
            pass.set_bind_group(0, &self.atlas_bind_group, &[]);
            pass.set_vertex_buffer(0, self.overlay_vertex_buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }
}
