use super::CompositorUniforms;
use super::GpuCompositor;
use crate::overlay::screen_record::gpu_export::setup::OverlayVertex;
use crate::overlay::screen_record::native_export::config::OverlayQuad;

/// Core rendering methods for `GpuCompositor`.
impl GpuCompositor {
    pub fn render_to_output(
        &self,
        uniforms: &CompositorUniforms,
        clear: bool,
        video_bg: Option<&wgpu::BindGroup>,
    ) {
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
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[0]);
            pass.set_bind_group(1, video_bg.unwrap_or(&self.video_bind_group), &[]);
            pass.set_bind_group(2, &self.cursor_bind_group, &[]);
            pass.set_bind_group(3, &self.background_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..6, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Run all motion blur sub-frames in a single RenderPass with one queue.submit().
    ///
    /// Each pass updates the uniform buffer offset (dynamic offset) and blend constant
    /// between draw calls — no encoder recreation overhead per pass. This replaces
    /// N separate encoder+submit cycles with 1, cutting ~0.2ms × N overhead from
    /// every motion-blur frame.
    pub fn render_accumulate_batched(
        &self,
        passes: &[(CompositorUniforms, f64)],
        video_bg: Option<&wgpu::BindGroup>,
    ) {
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
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: load_op,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.accumulate_pipeline);
            pass.set_bind_group(1, video_bg.unwrap_or(&self.video_bind_group), &[]);
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

    fn build_overlay_vertices(&self, quads: &[OverlayQuad]) -> Vec<OverlayVertex> {
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
        vertices
    }

    pub fn render_post_overlays(
        &self,
        webcam_frame: Option<
            &crate::overlay::screen_record::native_export::config::BakedWebcamFrame,
        >,
        quads: &[OverlayQuad],
    ) {
        let shared = &self.shared;
        let webcam_ready = webcam_frame.is_some_and(|frame| {
            self.webcam_overlay
                .prepare(&self.queue, self.width, self.height, frame)
        });

        let overlay_vertices = if quads.is_empty() {
            Vec::new()
        } else {
            self.build_overlay_vertices(quads)
        };
        if !webcam_ready && overlay_vertices.is_empty() {
            return;
        }

        let view = self
            .output_texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        if webcam_ready {
            let mut encoder = self.post_overlay_encoder();
            let mut pass = begin_load_pass(&mut encoder, &view, "Webcam Overlay Pass");
            self.webcam_overlay
                .render_pass(&mut pass, shared, &self.vertex_buffer);
            drop(pass);
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // A frame with more text quads than the vertex buffer holds is drawn in
        // buffer-sized batches (whole quads each) instead of being dropped.
        // Each batch is its own submission because queued buffer writes land
        // before the next submit.
        let quads_per_batch = self.overlay_vertex_buffer.size() as usize
            / (std::mem::size_of::<OverlayVertex>() * VERTICES_PER_QUAD);
        if quads_per_batch == 0 {
            return;
        }
        for batch in overlay_vertices.chunks(quads_per_batch * VERTICES_PER_QUAD) {
            self.queue
                .write_buffer(&self.overlay_vertex_buffer, 0, bytemuck::cast_slice(batch));
            let mut encoder = self.post_overlay_encoder();
            let mut pass = begin_load_pass(&mut encoder, &view, "Atlas Overlay Pass");
            pass.set_pipeline(&shared.overlay_pipeline);
            pass.set_bind_group(0, &self.atlas_bind_group, &[]);
            pass.set_vertex_buffer(0, self.overlay_vertex_buffer.slice(..));
            pass.draw(0..batch.len() as u32, 0..1);
            drop(pass);
            self.queue.submit(std::iter::once(encoder.finish()));
        }
    }

    fn post_overlay_encoder(&self) -> wgpu::CommandEncoder {
        self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Post Overlays"),
            })
    }
}

const VERTICES_PER_QUAD: usize = 6;

fn begin_load_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    view: &'a wgpu::TextureView,
    label: &'static str,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    })
}
