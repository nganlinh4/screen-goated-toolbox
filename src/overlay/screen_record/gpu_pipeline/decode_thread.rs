use std::sync::atomic::Ordering;
use std::time::Instant;

use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::Interface;

use super::super::d3d_interop::{D3D11GpuFence, VideoProcessor};
use super::super::mf_decode::{DxgiDeviceManager, MfDecoder};
use super::frame_timing::get_speed;
use super::types::{DecodeOutput, DecodeThreadContext};

pub(super) fn run_decode_thread(context: DecodeThreadContext<'_>) -> Result<(), String> {
    let DecodeThreadContext {
        label,
        cancel_flag,
        source_video_path,
        source_times,
        speed_points,
        trim_segments,
        framerate,
        crop_x,
        crop_y,
        source_rect_width,
        source_rect_height,
        output_width,
        output_height,
        active_mask,
        d3d11_device,
        d3d11_context,
        tx,
        recycle_rx,
        shared_textures,
        keyed_mutexes,
        d3d11_fence,
    } = context;
    let t_thread = Instant::now();

    let gpu_fence = D3D11GpuFence::new(d3d11_device, d3d11_context)?;

    // Cast to ID3D11DeviceContext4 for cross-API fence signaling.
    let d3d11_context4: Option<ID3D11DeviceContext4> =
        d3d11_fence.and_then(|_| d3d11_context.cast::<ID3D11DeviceContext4>().ok());

    let mut fence_value: u64 = 0;

    let t_dec_init = Instant::now();
    let device_manager = DxgiDeviceManager::new(d3d11_device)?;
    let decoder = MfDecoder::new(source_video_path, &device_manager, true)?;
    let source_w = decoder.width();
    let source_h = decoder.height();

    let initial_seek = if !trim_segments.is_empty() {
        trim_segments[0].start_time
    } else {
        source_times.first().copied().unwrap_or(0.0).max(0.0)
    };
    if initial_seek > 0.0 {
        decoder.seek_seconds(initial_seek)?;
    }
    let vp_out_w = output_width;
    let vp_out_h = output_height;
    let decode_vp = VideoProcessor::new(
        d3d11_device,
        d3d11_context,
        source_w,
        source_h,
        vp_out_w,
        vp_out_h,
    )?;
    if crop_x != 0 || crop_y != 0 || source_rect_width != source_w || source_rect_height != source_h
    {
        decode_vp.set_source_rect(crop_x, crop_y, source_rect_width, source_rect_height);
    }

    // Pool of VP output textures for B-frame PTS reorder.
    // Hardware decoders deliver B-frames in decode order (non-monotonic PTS).
    // We fill a window of REORDER_WINDOW frames, sort by PTS, then output in
    // display order -- fixing the "back and forth frames" issue on B-frame content.
    const REORDER_WINDOW: usize = 6;
    let pool_size = REORDER_WINDOW + 2; // +2 for cur + next held simultaneously
    let mut vp_pool: Vec<ID3D11Texture2D> = Vec::with_capacity(pool_size);
    let mut vp_resources: Vec<ID3D11Resource> = Vec::with_capacity(pool_size);
    let mut free_vp_slots: Vec<usize> = (0..pool_size).rev().collect();

    for _ in 0..pool_size {
        let tex = VideoProcessor::create_texture(
            d3d11_device,
            vp_out_w,
            vp_out_h,
            DXGI_FORMAT_B8G8R8A8_UNORM,
            D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE,
        )?;
        vp_resources.push(tex.cast().map_err(|e| format!("vp_pool->Resource: {e}"))?);
        vp_pool.push(tex);
    }

    // Pre-cast shared ring textures to ID3D11Resource (avoids per-frame cast).
    let shared_resources: Vec<ID3D11Resource> = shared_textures
        .iter()
        .map(|t| t.cast().map_err(|e| format!("shared_ring->Resource: {e}")))
        .collect::<Result<_, _>>()?;

    // Reorder queue: (vp_slot, pts). Sorted descending so pop() yields the lowest PTS.
    let mut reorder_queue: Vec<(usize, f64)> = Vec::with_capacity(REORDER_WINDOW);
    let mut eof_reached = false;
    let mut src_decoded: u32 = 0;

    // Holds DecodedFrames (IMFSamples) alive while VP Blts are pending on the GPU.
    // VP Blt reads from the decoder's NV12 texture asynchronously; if the IMFSample is
    // dropped too early, MF recycles the texture subresource and NVDEC (a separate HW
    // engine) can overwrite it before the VP Blt finishes reading -- causing frame corruption.
    // We collect frames here, then gpu_fence.signal_and_wait() after all VP Blts are queued,
    // ensuring they complete before the decoder can recycle any texture.
    let mut pending_samples: Vec<super::super::mf_decode::DecodedFrame> = Vec::new();

    // Diagnostic: log raw decoder PTS and output frame selection for first N frames.
    let decode_debug = false;

    // Fill the reorder queue from the decoder up to REORDER_WINDOW entries.
    // VP-converts each frame at read time (decode order), then sorts by PTS (display order).
    macro_rules! fill_reorder_queue {
        () => {
            if !eof_reached {
                while reorder_queue.len() < REORDER_WINDOW {
                    let slot = match free_vp_slots.pop() {
                        Some(s) => s,
                        None => break, // all slots busy -- queue is as full as possible
                    };
                    match decoder.read_frame()? {
                        Some(f) => {
                            decode_vp.convert(&f.texture, f.subresource_index, &vp_pool[slot])?;
                            let pts = f.pts_100ns as f64 / 10_000_000.0;
                            if decode_debug && src_decoded < 40 {
                                eprintln!(
                                    "[DecDbg] raw#{} pts={:.4} slot={}",
                                    src_decoded, pts, slot
                                );
                            }
                            reorder_queue.push((slot, pts));
                            pending_samples.push(f); // keep IMFSample alive until VP Blt done
                            src_decoded += 1;
                        }
                        None => {
                            eof_reached = true;
                            free_vp_slots.push(slot);
                            break;
                        }
                    }
                }
                // Ensure all VP Blts complete before releasing decoder textures.
                // NVDEC runs on a separate HW engine from the VP -- without this fence,
                // the decoder can overwrite source NV12 textures while VP is still reading.
                if !pending_samples.is_empty() {
                    gpu_fence.signal_and_wait();
                    pending_samples.clear();
                }
                // Sort descending; pop() then gives the frame with the lowest PTS.
                reorder_queue
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
        };
    }

    eprintln!(
        "[Decode:{label}] Init (MfDecoder + VP + pool): {:.3}s",
        t_dec_init.elapsed().as_secs_f64()
    );

    fill_reorder_queue!();
    let fa = reorder_queue.pop();
    fill_reorder_queue!();
    let fb = reorder_queue.pop();

    let mut cur_slot: Option<usize> = fa.map(|(s, _)| s);
    let mut _cur_pts: f64 = fa.map(|(_, p)| p).unwrap_or(0.0);
    let mut next_slot: Option<usize> = fb.map(|(s, _)| s);
    let mut next_pts: f64 = fb.map(|(_, p)| p).unwrap_or(f64::MAX);
    let mut have_next = next_slot.is_some();

    // Flush all reorder state, seek, and re-prime with fresh frames.
    // Must be defined AFTER the `let mut cur_slot/next_slot/cur_pts/next_pts/have_next`
    // declarations -- macro_rules! resolves bare identifiers at the definition site.
    macro_rules! flush_and_refill {
        () => {
            if let Some(s) = cur_slot.take() {
                free_vp_slots.push(s);
            }
            if let Some(s) = next_slot.take() {
                free_vp_slots.push(s);
            }
            for (s, _) in reorder_queue.drain(..) {
                free_vp_slots.push(s);
            }
            eof_reached = false;
            fill_reorder_queue!();
            let fa = reorder_queue.pop();
            fill_reorder_queue!();
            let fb = reorder_queue.pop();
            cur_slot = fa.map(|(s, _)| s);
            _cur_pts = fa.map(|(_, p)| p).unwrap_or(0.0);
            next_slot = fb.map(|(s, _)| s);
            next_pts = fb.map(|(_, p)| p).unwrap_or(f64::MAX);
            have_next = next_slot.is_some();
        };
    }

    if cur_slot.is_none() {
        return Ok(());
    }

    let mut current_segment_idx: usize = 0;
    let mut frames_held: u32 = 0;
    let mut slow_frame_count: u32 = 0;
    for (frame_idx, &source_time) in source_times.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }
        let frame_t0 = Instant::now();

        let next_source_time = source_times
            .get(frame_idx + 1)
            .copied()
            .unwrap_or(source_time);
        let speed = get_speed(source_time, speed_points).clamp(0.1, 16.0);
        let expected_step = speed / framerate.max(1) as f64;
        let mut source_step = next_source_time - source_time;
        if source_step <= 0.0 || source_step > expected_step * 1.05 {
            source_step = expected_step;
        }

        if active_mask.is_some_and(|mask| !mask[frame_idx]) {
            if tx
                .send(DecodeOutput::Inactive {
                    source_time,
                    source_step,
                    frame_idx: frame_idx as u32,
                })
                .is_err()
            {
                break;
            }
            continue;
        }

        let mut source_changed = false;

        // Seek on trim segment boundary change.
        if !trim_segments.is_empty() {
            let target_seg = trim_segments
                .iter()
                .position(|s| {
                    source_time >= s.start_time - 0.001 && source_time <= s.end_time + 0.001
                })
                .unwrap_or(current_segment_idx);

            if target_seg != current_segment_idx {
                decoder.seek_seconds(trim_segments[target_seg].start_time)?;
                current_segment_idx = target_seg;
                flush_and_refill!();
                source_changed = true;
            }
        }

        // Fast-forward seek if source_time is >1.5s ahead.
        if have_next && source_time - next_pts > 1.5 {
            decoder.seek_seconds(source_time)?;
            flush_and_refill!();
            source_changed = true;
        }

        let mut advanced = false;
        while have_next && next_pts <= source_time {
            if let Some(s) = cur_slot.take() {
                free_vp_slots.push(s);
            }
            cur_slot = next_slot.take();
            _cur_pts = next_pts;
            advanced = true;

            fill_reorder_queue!();
            if let Some((s, p)) = reorder_queue.pop() {
                next_slot = Some(s);
                next_pts = p;
            } else {
                have_next = false;
                next_slot = None;
                next_pts = f64::MAX;
            }
        }

        if !advanced && !source_changed && frame_idx > 0 {
            frames_held += 1;
            if tx
                .send(DecodeOutput::GpuHold {
                    source_time,
                    source_step,
                    frame_idx: frame_idx as u32,
                })
                .is_err()
            {
                break;
            }
            continue;
        }

        // Acquire a free ring slot (blocks if all slots are in use by render thread).
        let ring_idx = match recycle_rx.try_recv() {
            Ok(idx) => idx,
            Err(std::sync::mpsc::TryRecvError::Empty) => match recycle_rx.recv() {
                Ok(idx) => idx,
                Err(_) => break,
            },
            Err(_) => break,
        };

        let cur_vp_slot = match cur_slot {
            Some(s) => s,
            None => break,
        };

        if decode_debug && frame_idx < 40 {
            eprintln!(
                "[DecDbg] OUT f{} src_t={:.4} cur_pts={:.4} next_pts={:.4} slot={} adv={}",
                frame_idx, source_time, _cur_pts, next_pts, cur_vp_slot, advanced
            );
        }

        // Acquire keyed mutex for the shared decode ring slot (CPU-side ownership).
        if !keyed_mutexes.is_empty() {
            unsafe {
                keyed_mutexes[ring_idx]
                    .AcquireSync(0, u32::MAX)
                    .map_err(|e| format!("AcquireSync dec[{ring_idx}]: {e}"))?;
            }
        }

        // Copy VP output to shared decode ring slot.
        unsafe {
            d3d11_context.CopyResource(&shared_resources[ring_idx], &vp_resources[cur_vp_slot]);
        }

        // Signal cross-API fence AFTER the CopyResource.
        // This queues a GPU-timeline signal on D3D11's command queue. The render thread
        // calls ID3D12CommandQueue::Wait on this fence value, which stalls DX12's queue
        // until D3D11's GPU work completes -- providing both ordering AND cache coherence.
        fence_value += 1;
        if let (Some(ctx4), Some(fence)) = (&d3d11_context4, d3d11_fence) {
            unsafe {
                ctx4.Signal(fence, fence_value)
                    .map_err(|e| format!("D3D11 Signal fence[{ring_idx}]: {e}"))?;
            }
        }

        // GPU fence: ensure CopyResource + Signal are committed to GPU queue.
        gpu_fence.signal_and_wait();

        // Release keyed mutex -- D3D11 is done writing, render thread can acquire.
        if !keyed_mutexes.is_empty() {
            unsafe {
                let _ = keyed_mutexes[ring_idx].ReleaseSync(0);
            }
        }

        let frame_dur = frame_t0.elapsed().as_secs_f64();
        if frame_dur > 0.5 {
            slow_frame_count += 1;
            eprintln!(
                "[Decode:{label}] SLOW f{frame_idx}/{} src_t={source_time:.3} took {frame_dur:.3}s",
                source_times.len()
            );
        }

        if tx
            .send(DecodeOutput::Gpu {
                ring_idx,
                source_time,
                source_step,
                frame_idx: frame_idx as u32,
                fence_value,
            })
            .is_err()
        {
            break;
        }
    }

    if slow_frame_count > 0 {
        eprintln!("[Decode:{label}] {slow_frame_count} slow frames (>0.5s each)");
    }
    let elapsed = t_thread.elapsed().as_secs_f64();
    let fps = if elapsed > 0.001 {
        source_times.len() as f64 / elapsed
    } else {
        0.0
    };
    println!(
        "[Decode:{}] GPU: {} src -> {} out ({} held) in {:.1}s ({:.1} out_fps)",
        label,
        src_decoded,
        source_times.len(),
        frames_held,
        elapsed,
        fps
    );
    Ok(())
}
