use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::time::Instant;

use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::Interface;

use super::super::d3d_interop::{D3D11Readback, VideoProcessor, create_d3d11_device};
use super::super::mf_decode::{DxgiDeviceManager, MfDecoder};
use super::frame_timing::get_speed;
use super::types::{DecodeOutput, PipelineConfig};

/// CPU fallback decode thread: D3D11 VP + CPU readback (legacy path).
///
/// `cur_bgra` and `next_bgra` are PERMANENT buffers owned by this thread (never sent across
/// threads). Per output frame we copy `cur_bgra` into a recycled `send_buf` and send that.
/// This correctly handles the "hold" case (same source frame reused for multiple output frames)
/// which occurs whenever output fps > source fps.
pub(super) fn run_decode_thread_cpu(
    config: &PipelineConfig,
    cancel_flag: &std::sync::atomic::AtomicBool,
    source_times: &[f64],
    tx: mpsc::SyncSender<DecodeOutput>,
    recycle_rx: mpsc::Receiver<Vec<u8>>,
) -> Result<(), String> {
    let t_thread = Instant::now();

    // D3D11 device (CPU decode thread creates its own)
    let (d3d11_device, d3d11_context) = create_d3d11_device()?;
    {
        let mt: ID3D11Multithread = d3d11_device
            .cast()
            .map_err(|e| format!("QI ID3D11Multithread: {e}"))?;
        unsafe {
            let _ = mt.SetMultithreadProtected(true);
        }
    }

    let device_manager = DxgiDeviceManager::new(&d3d11_device)?;
    let decoder = MfDecoder::new(&config.source_video_path, &device_manager, true)?;
    let source_w = decoder.width();
    let source_h = decoder.height();

    let initial_seek = if !config.trim_segments.is_empty() {
        config.trim_segments[0].start_time
    } else {
        config.trim_start
    };
    if initial_seek > 0.0 {
        decoder.seek_seconds(initial_seek)?;
    }

    let vp_out_w = config.video_width;
    let vp_out_h = config.video_height;
    let decode_vp = VideoProcessor::new(
        &d3d11_device,
        &d3d11_context,
        source_w,
        source_h,
        vp_out_w,
        vp_out_h,
    )?;
    decode_vp.set_source_rect(config.crop_x, config.crop_y, vp_out_w, vp_out_h);

    let vp_output = VideoProcessor::create_texture(
        &d3d11_device,
        vp_out_w,
        vp_out_h,
        DXGI_FORMAT_B8G8R8A8_UNORM,
        D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE,
    )?;
    let readback = D3D11Readback::new(
        &d3d11_device,
        &d3d11_context,
        vp_out_w,
        vp_out_h,
        DXGI_FORMAT_B8G8R8A8_UNORM,
    )?;

    let frame_size = (vp_out_w * vp_out_h * 4) as usize;

    // Decode one source frame into a buffer; returns PTS in seconds.
    let decode_one = |buf: &mut Vec<u8>| -> Result<Option<f64>, String> {
        let decoded = match decoder.read_frame()? {
            Some(f) => f,
            None => return Ok(None),
        };
        decode_vp.convert(&decoded.texture, decoded.subresource_index, &vp_output)?;
        readback.readback(&vp_output, buf)?;
        Ok(Some(decoded.pts_100ns as f64 / 10_000_000.0))
    };

    // B-frame reorder buffer: same window approach as the GPU path.
    // decode_one (VP + readback) already runs at fill time; queue holds decoded BGRA.
    const REORDER_WINDOW_CPU: usize = 6;
    let mut free_bufs: Vec<Vec<u8>> = Vec::new();
    let mut reorder_queue_cpu: Vec<(Vec<u8>, f64)> = Vec::with_capacity(REORDER_WINDOW_CPU);
    let mut eof_reached_cpu = false;
    let mut src_decoded: u32 = 0;

    macro_rules! get_buf {
        () => {
            free_bufs.pop().unwrap_or_else(|| {
                let mut b = Vec::with_capacity(frame_size);
                b.resize(frame_size, 0);
                b
            })
        };
    }

    macro_rules! fill_reorder_cpu {
        () => {
            if !eof_reached_cpu {
                while reorder_queue_cpu.len() < REORDER_WINDOW_CPU {
                    let mut buf = get_buf!();
                    match decode_one(&mut buf)? {
                        Some(pts) => {
                            reorder_queue_cpu.push((buf, pts));
                            src_decoded += 1;
                        }
                        None => {
                            eof_reached_cpu = true;
                            free_bufs.push(buf);
                            break;
                        }
                    }
                }
                reorder_queue_cpu
                    .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
        };
    }

    fill_reorder_cpu!();
    let fa = reorder_queue_cpu.pop();
    fill_reorder_cpu!();
    let fb = reorder_queue_cpu.pop();

    let (mut cur_bgra, mut cur_pts): (Option<Vec<u8>>, f64) =
        fa.map(|(b, p)| (Some(b), p)).unwrap_or((None, 0.0));
    let (mut next_bgra, mut next_pts): (Option<Vec<u8>>, f64) =
        fb.map(|(b, p)| (Some(b), p)).unwrap_or((None, f64::MAX));
    let mut have_next = next_bgra.is_some();

    // Must be defined AFTER the variable declarations above -- see comment on flush_and_refill!.
    macro_rules! flush_and_refill_cpu {
        () => {
            if let Some(b) = cur_bgra.take() {
                free_bufs.push(b);
            }
            if let Some(b) = next_bgra.take() {
                free_bufs.push(b);
            }
            for (b, _) in reorder_queue_cpu.drain(..) {
                free_bufs.push(b);
            }
            eof_reached_cpu = false;
            fill_reorder_cpu!();
            let fa = reorder_queue_cpu.pop();
            fill_reorder_cpu!();
            let fb = reorder_queue_cpu.pop();
            (cur_bgra, cur_pts) = fa.map(|(b, p)| (Some(b), p)).unwrap_or((None, 0.0));
            (next_bgra, next_pts) = fb.map(|(b, p)| (Some(b), p)).unwrap_or((None, f64::MAX));
            have_next = next_bgra.is_some();
        };
    }

    if cur_bgra.is_none() {
        return Ok(());
    }

    let mut current_segment_idx: usize = 0;
    let mut frames_held: u32 = 0;

    for (frame_idx, &source_time) in source_times.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let next_source_time = source_times
            .get(frame_idx + 1)
            .copied()
            .unwrap_or(source_time);
        let speed = get_speed(source_time, &config.speed_points).clamp(0.1, 16.0);
        let expected_step = speed / config.framerate as f64;
        let mut source_step = next_source_time - source_time;
        if source_step <= 0.0 || source_step > expected_step * 1.05 {
            source_step = expected_step;
        }

        // Seek on trim segment boundary change.
        if !config.trim_segments.is_empty() {
            let target_seg = config
                .trim_segments
                .iter()
                .position(|s| {
                    source_time >= s.start_time - 0.001 && source_time <= s.end_time + 0.001
                })
                .unwrap_or(current_segment_idx);

            if target_seg != current_segment_idx {
                decoder.seek_seconds(config.trim_segments[target_seg].start_time)?;
                current_segment_idx = target_seg;
                flush_and_refill_cpu!();
            }
        }

        // Fast-forward seek if source_time is >1.5s ahead.
        if have_next && source_time - next_pts > 1.5 {
            decoder.seek_seconds(source_time)?;
            flush_and_refill_cpu!();
        }

        let mut advanced = false;
        while have_next && next_pts <= source_time {
            if let Some(b) = cur_bgra.take() {
                free_bufs.push(b);
            }
            cur_bgra = next_bgra.take();
            cur_pts = next_pts;
            advanced = true;

            fill_reorder_cpu!();
            if let Some((b, p)) = reorder_queue_cpu.pop() {
                next_bgra = Some(b);
                next_pts = p;
            } else {
                have_next = false;
                next_bgra = None;
                next_pts = f64::MAX;
            }
        }

        if !advanced && frame_idx > 0 {
            frames_held += 1;
        }
        let _ = cur_pts;

        if let Some(ref bgra) = cur_bgra {
            let mut send_buf = recycle_rx.try_recv().unwrap_or_else(|_| get_buf!());
            send_buf.resize(frame_size, 0);
            send_buf.copy_from_slice(bgra);

            if tx
                .send(DecodeOutput::Cpu {
                    bgra_video: send_buf,
                    source_time,
                    source_step,
                    frame_idx: frame_idx as u32,
                })
                .is_err()
            {
                break;
            }
        } else {
            break;
        }
    }

    let elapsed = t_thread.elapsed().as_secs_f64();
    println!(
        "[Decode] CPU: {} src -> {} out ({} held) in {:.1}s",
        src_decoded,
        source_times.len(),
        frames_held,
        elapsed
    );
    Ok(())
}
