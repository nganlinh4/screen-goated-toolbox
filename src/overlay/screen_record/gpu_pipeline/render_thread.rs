use std::sync::atomic::Ordering;
use std::time::Instant;

use windows::Win32::Graphics::Direct3D12 as d3d12;

use super::ComScope;
use super::types::{DecodeOutput, RenderOutput, RenderThreadContext};

pub(super) fn run_render_thread(context: RenderThreadContext<'_>) -> Result<(), String> {
    let _com_scope = ComScope::initialize_mta()?;
    let RenderThreadContext {
        config,
        compositor,
        build_uniforms,
        cancel_flag,
        rx,
        mb_samples,
        tx,
        dec_gpu_recycle_tx,
        dec_cpu_recycle_tx,
        dec_wgpu_textures,
        dec_d3d12_fence,
        dec_keyed_mutexes,
        webcam_rx,
        webcam_gpu_recycle_tx,
        webcam_wgpu_textures,
        webcam_d3d12_fence,
        webcam_keyed_mutexes,
        webcam_render_width,
        webcam_render_height,
        gpu_textures,
        gpu_dx12_keyed_mutexes,
        gpu_slot_rx,
        cpu_recycle_rx,
    } = context;
    let use_gpu_encode = !gpu_textures.is_empty();
    let mut frames_rendered: u32 = 0;
    let mut t_upload = 0.0_f64;
    let mut t_render = 0.0_f64;
    let mut t_readback = 0.0_f64;
    let mut t_wait = 0.0_f64;

    // Pipelined readback state (CPU encode path only).
    let mut queued_readbacks: u32 = 0;

    // GPU decode ring recycle queue.
    // copy_frame_from_shared is async -- the ring slot cannot be returned to the decode
    // thread until after poll(Wait) confirms the DX12 read has completed.
    // Each entry is Some(ring_idx) for GPU-decode frames, None for CPU-decode frames.
    // GPU encode: one entry per frame, drained immediately after poll(Wait).
    // CPU encode: entries accumulate with pipelined readbacks and are drained together.
    let mut dec_ring_recycle_queue: std::collections::VecDeque<Option<usize>> =
        std::collections::VecDeque::new();
    let mut webcam_ring_recycle_queue: std::collections::VecDeque<Option<usize>> =
        std::collections::VecDeque::new();

    let mut render_slow_count: u32 = 0;
    loop {
        let tw0 = Instant::now();
        let msg = match rx.recv() {
            Ok(m) => m,
            Err(_) => break,
        };
        let wait_dur = tw0.elapsed().as_secs_f64();
        t_wait += wait_dur;

        if cancel_flag.load(Ordering::Relaxed) {
            match &msg {
                DecodeOutput::Gpu { ring_idx, .. } => {
                    let _ = dec_gpu_recycle_tx.send(*ring_idx);
                }
                DecodeOutput::GpuHold { .. } => {}
                DecodeOutput::Cpu { .. } => {}
                DecodeOutput::Inactive { .. } => {}
            }
            break;
        }

        let render_frame_t0 = Instant::now();
        let source_time = msg.source_time();
        let source_step = msg.source_step();
        let frame_idx = msg.frame_idx();

        // 1. Upload video frame to GPU (GPU copy or CPU upload depending on decode path).
        //
        // GPU path: Wait on cross-API shared fence to ensure D3D11's writes are visible
        // on DX12's GPU timeline, then copy from persistent shared texture.
        // Ring slot recycled after poll(Wait) confirms DX12 copy+render complete.
        let tu0 = Instant::now();
        match msg {
            DecodeOutput::Gpu {
                ring_idx,
                fence_value,
                ..
            } => {
                // Acquire keyed mutex on DX12 side (CPU ownership for shared texture).
                if !dec_keyed_mutexes.is_empty() {
                    unsafe {
                        dec_keyed_mutexes[ring_idx]
                            .AcquireSync(0, u32::MAX)
                            .map_err(|e| format!("AcquireSync dec render[{ring_idx}]: {e}"))?;
                    }
                }

                // Wait on DX12 command queue for D3D11's fence Signal.
                // This stalls DX12's GPU pipeline until D3D11 completes writing to the
                // shared texture. The Wait acts as an implicit acquire barrier, invalidating
                // DX12's L2 cache so subsequent reads get fresh VRAM data.
                if let Some(fence) = dec_d3d12_fence {
                    unsafe {
                        if let Some(hal_dev) = compositor.device().as_hal::<wgpu::hal::api::Dx12>()
                        {
                            let raw_queue: &d3d12::ID3D12CommandQueue =
                                &*(hal_dev.raw_queue() as *const _);
                            let _ = raw_queue.Wait(fence, fence_value);
                        }
                    }
                }

                compositor.copy_frame_from_shared(&dec_wgpu_textures[ring_idx]);

                dec_ring_recycle_queue.push_back(Some(ring_idx)); // recycled after poll(Wait)
            }
            DecodeOutput::GpuHold { .. } => {
                // Reuse the previously uploaded frame when the source PTS is held.
                // This avoids unnecessary shared-texture reads on hold frames.
                dec_ring_recycle_queue.push_back(None);
            }
            DecodeOutput::Cpu { bgra_video, .. } => {
                // CPU fallback: PCIe upload (no deferred recycle needed).
                compositor.upload_frame(&bgra_video);
                let _ = dec_cpu_recycle_tx.send(bgra_video);
                dec_ring_recycle_queue.push_back(None);
            }
            DecodeOutput::Inactive { .. } => {
                dec_ring_recycle_queue.push_back(None);
            }
        }
        t_upload += tu0.elapsed().as_secs_f64();

        if let Some(webcam_rx) = webcam_rx.as_ref() {
            let webcam_msg = webcam_rx
                .recv()
                .map_err(|_| "Webcam decode channel closed".to_string())?;
            if webcam_msg.frame_idx() != frame_idx {
                return Err(format!(
                    "Webcam frame index mismatch: expected {}, got {}",
                    frame_idx,
                    webcam_msg.frame_idx()
                ));
            }

            match webcam_msg {
                DecodeOutput::Gpu {
                    ring_idx,
                    fence_value,
                    ..
                } => {
                    if !webcam_keyed_mutexes.is_empty() {
                        unsafe {
                            webcam_keyed_mutexes[ring_idx]
                                .AcquireSync(0, u32::MAX)
                                .map_err(|e| {
                                    format!("AcquireSync webcam render[{ring_idx}]: {e}")
                                })?;
                        }
                    }
                    if let Some(fence) = webcam_d3d12_fence {
                        unsafe {
                            if let Some(hal_dev) =
                                compositor.device().as_hal::<wgpu::hal::api::Dx12>()
                            {
                                let raw_queue: &d3d12::ID3D12CommandQueue =
                                    &*(hal_dev.raw_queue() as *const _);
                                let _ = raw_queue.Wait(fence, fence_value);
                            }
                        }
                    }
                    compositor.copy_webcam_frame_from_shared(
                        &webcam_wgpu_textures[ring_idx],
                        webcam_render_width,
                        webcam_render_height,
                    );
                    webcam_ring_recycle_queue.push_back(Some(ring_idx));
                }
                DecodeOutput::GpuHold { .. } | DecodeOutput::Inactive { .. } => {
                    webcam_ring_recycle_queue.push_back(None);
                }
                DecodeOutput::Cpu { .. } => {
                    return Err("Unexpected CPU webcam decode output".to_string());
                }
            }
        }

        // 1b. Update animated cursor atlas tiles based on current source time.
        for slot in &config.animated_cursor_slots {
            if slot.frames.is_empty() || slot.loop_duration <= 0.0 {
                continue;
            }
            let n = slot.frames.len();
            let t = source_time % slot.loop_duration;
            let idx = ((t / slot.loop_duration) * n as f64).floor() as usize % n;
            compositor.upload_cursor_slot_rgba(slot.slot_id, &slot.frames[idx]);
        }

        // 2. Render to output texture.
        let tr0 = Instant::now();
        if mb_samples > 1 {
            let zoom_shutter = source_step * config.blur_zoom_shutter;
            let pan_shutter = source_step * config.blur_pan_shutter;
            let cursor_shutter = source_step * config.blur_cursor_shutter;

            // Collect all N (uniforms, weight) pairs then dispatch as a single
            // RenderPass -- 1 encoder, 1 submit, N draw calls with dynamic offsets.
            let mut passes = Vec::with_capacity(mb_samples as usize);
            for i in 0..mb_samples {
                let t = i as f64 / (mb_samples - 1).max(1) as f64;
                let pan_time = source_time - (pan_shutter * 0.5) + t * pan_shutter;
                let zoom_time = source_time - (zoom_shutter * 0.5) + t * zoom_shutter;
                let cur_time = source_time - (cursor_shutter * 0.5) + t * cursor_shutter;
                let uniforms = build_uniforms(source_time, pan_time, zoom_time, cur_time);
                passes.push((uniforms, 1.0 / (i as f64 + 1.0)));
            }
            compositor.render_accumulate_batched(&passes, None);
        } else {
            let uniforms = build_uniforms(source_time, source_time, source_time, source_time);
            compositor.render_to_output(&uniforms, true, None);
        }

        let webcam_frame = if webcam_rx.is_some() && !config.webcam_frames.is_empty() {
            let webcam_idx = (frame_idx as usize).min(config.webcam_frames.len() - 1);
            Some(&config.webcam_frames[webcam_idx])
        } else {
            None
        };
        let webcam_should_render = webcam_frame.is_some_and(|frame| {
            frame.visible && frame.opacity > 0.001 && frame.width > 0.0 && frame.height > 0.0
        });
        let overlay_quads = if !config.overlay_frames.is_empty() {
            let overlay_idx = (frame_idx as usize).min(config.overlay_frames.len() - 1);
            config.overlay_frames[overlay_idx].quads.as_slice()
        } else {
            &[]
        };

        if webcam_should_render || !overlay_quads.is_empty() {
            compositor.render_post_overlays(
                if webcam_should_render {
                    webcam_frame
                } else {
                    None
                },
                overlay_quads,
            );
        }

        t_render += tr0.elapsed().as_secs_f64();

        // 3. Output: GPU zero-copy or CPU readback.
        if use_gpu_encode {
            // GPU path: copy to shared VRAM texture, wait for GPU, send ring_idx.
            let ring_idx = gpu_slot_rx
                .recv()
                .map_err(|_| "GPU slot recycle channel closed")?;
            let trb0 = Instant::now();

            // Acquire DX12-side keyed mutex before writing to the encode ring slot.
            // This cache-invalidates any stale D3D11 data and satisfies the DXGI
            // keyed-mutex contract for cross-API (DX12<->D3D11) sharing.
            if !gpu_dx12_keyed_mutexes.is_empty() {
                unsafe {
                    gpu_dx12_keyed_mutexes[ring_idx]
                        .AcquireSync(0, u32::MAX)
                        .map_err(|e| format!("AcquireSync DX12 enc[{ring_idx}]: {e}"))?;
                }
            }

            compositor.copy_output_to_shared(&gpu_textures[ring_idx]);

            // Wait for all DX12 work to complete using on_submitted_work_done + poll.
            // This is strictly safer than poll(Wait) alone.
            let (tx_done, rx_done) = std::sync::mpsc::channel();
            compositor.queue().on_submitted_work_done(move || {
                let _ = tx_done.send(());
            });
            let _ = compositor
                .device()
                .poll(wgpu::PollType::wait_indefinitely());
            let _ = rx_done.recv();

            // Release DX12-side keyed mutex for this encode ring slot -- flushes DX12
            // caches so D3D11 (MF encoder) sees the freshly written frame data.
            if !gpu_dx12_keyed_mutexes.is_empty() {
                unsafe {
                    let _ = gpu_dx12_keyed_mutexes[ring_idx].ReleaseSync(0);
                }
            }

            // Recycle decode ring slot -- GPU work is done (poll+on_submitted_work_done).
            if let Some(Some(idx)) = dec_ring_recycle_queue.pop_front() {
                // Release decode keyed mutex so decode thread can re-acquire.
                if !dec_keyed_mutexes.is_empty() {
                    unsafe {
                        let _ = dec_keyed_mutexes[idx].ReleaseSync(0);
                    }
                }
                let _ = dec_gpu_recycle_tx.send(idx);
            }
            if let Some(Some(idx)) = webcam_ring_recycle_queue.pop_front() {
                if !webcam_keyed_mutexes.is_empty() {
                    unsafe {
                        let _ = webcam_keyed_mutexes[idx].ReleaseSync(0);
                    }
                }
                if let Some(tx) = webcam_gpu_recycle_tx.as_ref() {
                    let _ = tx.send(idx);
                }
            }
            t_readback += trb0.elapsed().as_secs_f64();
            if tx.send(RenderOutput::Gpu { ring_idx }).is_err() {
                break;
            }
            let render_frame_dur = render_frame_t0.elapsed().as_secs_f64();
            if render_frame_dur > 0.5 || wait_dur > 0.5 {
                render_slow_count += 1;
                eprintln!(
                    "[Render] SLOW f{frame_idx} src_t={source_time:.3} \
                     wait={wait_dur:.3}s render={render_frame_dur:.3}s"
                );
            }
            frames_rendered += 1;
        } else {
            // CPU path: pipelined readback (depth 2).
            compositor.enqueue_output_readback()?;
            queued_readbacks += 1;
            if queued_readbacks >= 2 {
                let trb0 = Instant::now();
                let mut out_buf = cpu_recycle_rx.try_recv().unwrap_or_default();
                compositor.readback_output(&mut out_buf)?;
                // readback_output internally does poll(Wait) -- DX12 copy is done.
                if let Some(Some(idx)) = dec_ring_recycle_queue.pop_front() {
                    if !dec_keyed_mutexes.is_empty() {
                        unsafe {
                            let _ = dec_keyed_mutexes[idx].ReleaseSync(0);
                        }
                    }
                    let _ = dec_gpu_recycle_tx.send(idx);
                }
                if let Some(Some(idx)) = webcam_ring_recycle_queue.pop_front() {
                    if !webcam_keyed_mutexes.is_empty() {
                        unsafe {
                            let _ = webcam_keyed_mutexes[idx].ReleaseSync(0);
                        }
                    }
                    if let Some(tx) = webcam_gpu_recycle_tx.as_ref() {
                        let _ = tx.send(idx);
                    }
                }
                t_readback += trb0.elapsed().as_secs_f64();
                queued_readbacks -= 1;
                if tx
                    .send(RenderOutput::Cpu {
                        rendered_bgra: out_buf,
                    })
                    .is_err()
                {
                    break;
                }
                frames_rendered += 1;
            }
        }
    }

    // CPU encode path: drain remaining GPU readbacks at end of video.
    while queued_readbacks > 0 {
        let trb0 = Instant::now();
        let mut out_buf = cpu_recycle_rx.try_recv().unwrap_or_default();
        compositor.readback_output(&mut out_buf)?;
        if let Some(Some(idx)) = dec_ring_recycle_queue.pop_front() {
            if !dec_keyed_mutexes.is_empty() {
                unsafe {
                    let _ = dec_keyed_mutexes[idx].ReleaseSync(0);
                }
            }
            let _ = dec_gpu_recycle_tx.send(idx);
        }
        if let Some(Some(idx)) = webcam_ring_recycle_queue.pop_front() {
            if !webcam_keyed_mutexes.is_empty() {
                unsafe {
                    let _ = webcam_keyed_mutexes[idx].ReleaseSync(0);
                }
            }
            if let Some(tx) = webcam_gpu_recycle_tx.as_ref() {
                let _ = tx.send(idx);
            }
        }
        t_readback += trb0.elapsed().as_secs_f64();
        queued_readbacks -= 1;
        let _ = tx.send(RenderOutput::Cpu {
            rendered_bgra: out_buf,
        });
        frames_rendered += 1;
    }

    if render_slow_count > 0 {
        eprintln!("[Render] {render_slow_count} slow frames (>0.5s each)");
    }
    let n = frames_rendered.max(1) as f64;
    let label = if use_gpu_encode { "copy" } else { "readback" };
    println!(
        "[Render] {} frames: upload {:.1} + render {:.1} + {} {:.1} + wait {:.1}ms avg",
        frames_rendered,
        t_upload / n * 1000.0,
        t_render / n * 1000.0,
        label,
        t_readback / n * 1000.0,
        t_wait / n * 1000.0,
    );

    Ok(())
}
