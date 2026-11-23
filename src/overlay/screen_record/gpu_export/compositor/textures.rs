use super::GpuCompositor;
use crate::overlay::screen_record::gpu_export::cursors::{
    CURSOR_ATLAS_COLS, CURSOR_TILE_SIZE, cursor_tile_mip_levels, dedupe_valid_slots,
    get_or_render_cursor_tile, is_uploadable_cursor_tile,
};

/// Texture upload, GPU-to-GPU copy, and readback methods for `GpuCompositor`.
impl GpuCompositor {
    pub fn upload_cursor_slot_rgba(&self, slot: u32, rgba: &[u8]) {
        if !is_uploadable_cursor_tile(slot, rgba.len()) {
            return;
        }
        let col = slot % CURSOR_ATLAS_COLS;
        let row = slot / CURSOR_ATLAS_COLS;
        self.write_cursor_tile_level(col, row, 0, rgba);
        for (level, pixels) in (1..).zip(cursor_tile_mip_levels(rgba)) {
            self.write_cursor_tile_level(col, row, level, &pixels);
        }
    }

    fn write_cursor_tile_level(&self, col: u32, row: u32, level: u32, rgba: &[u8]) {
        let side = CURSOR_TILE_SIZE >> level;
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.cursor_texture,
                mip_level: level,
                origin: wgpu::Origin3d {
                    x: col * side,
                    y: row * side,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(side * 4),
                rows_per_image: Some(side),
            },
            wgpu::Extent3d {
                width: side,
                height: side,
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

    pub fn upload_background(
        &mut self,
        rgba_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        super::super::limits::validate_rgba(
            &self.device.limits(),
            width,
            height,
            rgba_data,
            "Background",
        )?;
        let shared = &self.shared;
        let scopes = super::super::limits::ResourceErrorScopes::new(&self.device);

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
        scopes.finish("GPU background upload failed")?;
        self.background_width = width;
        self.background_height = height;
        Ok(())
    }

    /// Upload the sprite atlas RGBA pixels and rebuild the bind group.
    /// Call once before the pipeline starts.
    pub fn upload_atlas(
        &mut self,
        rgba_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        super::super::limits::validate_rgba(
            &self.device.limits(),
            width,
            height,
            rgba_data,
            "Overlay atlas",
        )?;
        let shared = &self.shared;
        let scopes = super::super::limits::ResourceErrorScopes::new(&self.device);
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
        scopes.finish("GPU overlay atlas upload failed")
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
        let shared = &self.shared;
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

    /// Upload one CPU-decoded BGRA webcam frame (fallback when the shared
    /// decode ring is unavailable).
    pub fn upload_webcam_frame(
        &mut self,
        bgra: &[u8],
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let expected = width as usize * height as usize * 4;
        if width == 0 || height == 0 || bgra.len() != expected {
            return Err(format!(
                "Webcam frame is {} bytes, expected {expected} for {width}x{height}",
                bgra.len()
            ));
        }
        self.webcam_overlay
            .ensure_size(&self.device, &self.shared, width, height);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: self.webcam_overlay.texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bgra,
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
        self.webcam_overlay.mark_has_frame();
        Ok(())
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

    fn copy_slot_into_vec(&self, slot: usize, out: &mut Vec<u8>) -> Result<(), String> {
        let buffer_slice = self.output_buffers[slot].slice(..);
        let data = buffer_slice
            .get_mapped_range()
            .map_err(|error| format!("GPU mapped range unavailable: {error}"))?;
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
        Ok(())
    }

    fn drain_next_readback(
        &mut self,
        out: &mut Vec<u8>,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<bool, String> {
        let Some(&slot) = self.pending_readbacks.front() else {
            return Ok(false);
        };
        let result = {
            let rx = self.readback_receivers[slot]
                .as_ref()
                .ok_or_else(|| "Missing readback receiver".to_string())?;
            super::super::wait::wait_for_callback(&self.device, rx, cancel, || self.check_health())
        };
        let status = match result {
            Ok(status) => status,
            Err(error) => {
                self.mark_unusable();
                return Err(error);
            }
        };
        if let Err(error) = status {
            self.mark_unusable();
            return Err(format!("GPU buffer map failed: {error}"));
        }

        self.readback_receivers[slot] = None;
        let _ = self.pending_readbacks.pop_front();
        self.copy_slot_into_vec(slot, out)?;
        Ok(true)
    }

    pub fn readback_output(
        &mut self,
        out: &mut Vec<u8>,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<(), String> {
        let _ = self.drain_next_readback(out, cancel)?;
        Ok(())
    }
}
