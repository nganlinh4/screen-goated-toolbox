use super::GpuCompositor;
use crate::overlay::screen_record::gpu_export::cursors::{
    dedupe_valid_slots, get_or_render_cursor_tile, CURSOR_ATLAS_COLS, CURSOR_TILE_SIZE,
};
use crate::overlay::screen_record::gpu_export::setup::shared_gpu_context;

/// Texture upload, GPU-to-GPU copy, and readback methods for `GpuCompositor`.
impl GpuCompositor {
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
    /// Uses a cache-flush barrier trick to ensure DX12 reads fresh VRAM data:
    /// 1. copy_buffer_to_texture (1 pixel) -> forces source into COPY_DST state
    /// 2. copy_texture_to_texture (full) -> transitions COPY_DST to COPY_SRC
    ///
    /// The COPY_DST→COPY_SRC barrier flushes L2 cache, guaranteeing DX12 reads
    /// the data that D3D11 just wrote (not stale cached data).
    /// Both commands in the same encoder/submit for correct ordering.
    pub fn copy_frame_from_shared(&self, source: &wgpu::Texture) {
        // Experimental raw-DX12 copy path. Keep this opt-in only until it is
        // proven stable across content and drivers.
        if std::env::var("SGT_EXPERIMENTAL_RAW_DX12_COPY").is_ok()
            && let Some(raw_dx12_copy) = &self.dx12_shared_copy
            && unsafe { raw_dx12_copy.copy_shared_to_video(source, &self.video_texture) }.is_ok()
        {
            return;
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Copy Decode to Video"),
            });

        // Step 1: 1-pixel buffer→texture copy to force source into COPY_DST state.
        // This is a no-op data-wise (writes 1 pixel at origin with zeroed data)
        // but forces DX12 to transition the resource state to COPY_DEST.
        // bytes_per_row=None is valid for a single-row copy (wgpu skips alignment check).
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &self.cache_flush_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: None,
                    rows_per_image: None,
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );

        // Step 2: Full copy from source to video_texture.
        // This transitions source from COPY_DST→COPY_SRC, which includes an
        // L2 cache flush barrier — ensuring DX12 reads fresh data from VRAM.
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

    pub fn copy_webcam_frame_from_shared(
        &mut self,
        source: &wgpu::Texture,
        width: u32,
        height: u32,
    ) {
        let shared = match shared_gpu_context() {
            Ok(shared) => shared,
            Err(_) => return,
        };
        self.webcam_overlay
            .ensure_size(&self.device, shared, width, height);

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Copy Shared Webcam to Overlay"),
            });

        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &self.cache_flush_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: None,
                    rows_per_image: None,
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );

        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: self.webcam_overlay.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));
        self.webcam_overlay.mark_has_frame();
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

        // Force target through COPY_DST→COPY_SRC once per write. This flushes DX12
        // caches/state for shared encode textures before D3D11 acquires the slot.
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &self.output_state_reset_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );

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
            wgpu::PollType::wait_indefinitely()
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
                        return Err("GPU readback channel disconnected".to_string());
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
}
