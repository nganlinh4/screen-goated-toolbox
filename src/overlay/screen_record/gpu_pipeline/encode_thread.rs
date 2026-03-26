use std::sync::atomic::Ordering;
use std::time::Instant;

use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::core::Interface;

use super::super::d3d_interop::D3D11GpuFence;
use super::super::mf_audio::{AudioConfig, MfAudioDecoder};
use super::super::mf_encode::{EncoderConfig, MfEncoder};
use super::audio::{apply_audio_volume_envelope, resample_pcm_bytes};
use super::frame_timing::get_speed;
use super::types::{EncodeThreadContext, RenderOutput, ZeroCopyExportResult};

pub(super) fn run_encode_thread(
    context: EncodeThreadContext<'_>,
) -> Result<ZeroCopyExportResult, String> {
    let EncodeThreadContext {
        config,
        enc_device_manager,
        progress,
        cancel_flag,
        rx,
        total_frames,
        start,
        gpu_buffers,
        gpu_recycle_tx,
        cpu_recycle_tx,
    } = context;
    let t_enc_init = Instant::now();
    let mut audio_decoder = None;
    let mut audio_config = None;

    if let Some(path) = &config.audio_path
        && !path.is_empty()
    {
        match MfAudioDecoder::new(path) {
            Ok(dec) => {
                audio_config = Some(AudioConfig {
                    sample_rate: dec.sample_rate(),
                    channels: dec.channels(),
                    bitrate_kbps: 192,
                });
                audio_decoder = Some(dec);
            }
            Err(e) => eprintln!("[Audio] Failed to open native audio decoder: {e}"),
        }
    }

    let encoder_config = EncoderConfig {
        codec: config.codec,
        width: config.output_width,
        height: config.output_height,
        fps_num: config.framerate,
        fps_den: 1,
        bitrate_kbps: config.bitrate_kbps,
    };
    let (encoder, opt_audio_stream) = MfEncoder::new(
        &config.output_path,
        encoder_config,
        enc_device_manager,
        audio_config.as_ref(),
    )?;
    let frame_duration_100ns = encoder.frame_duration_100ns();

    let mut audio_output_100ns = 0i64;
    let mut audio_segment_idx = 0usize;
    let mut audio_eof = false;
    let mut total_audio_samples_written: u64 = 0;

    if let Some(dec) = &audio_decoder
        && !config.audio_is_preprocessed
    {
        let start_time = if config.trim_segments.is_empty() {
            config.trim_start
        } else {
            config.trim_segments[0].start_time
        };
        if start_time > 0.0 {
            let _ = dec.seek((start_time * 10_000_000.0) as i64);
        }
    }

    eprintln!(
        "[Encode] Init (audio decoder + MF encoder): {:.3}s",
        t_enc_init.elapsed().as_secs_f64()
    );

    let mut frames_encoded: u32 = 0;
    let mut t_encode = 0.0_f64;
    let mut t_wait = 0.0_f64;

    // D3D11-side keyed mutexes for the encode ring -- AcquireSync before reading each
    // shared slot and ReleaseSync only after the slot has been copied to a private
    // encode texture owned by this thread.
    let gpu_enc_keyed_mutexes: Vec<IDXGIKeyedMutex> = gpu_buffers
        .iter()
        .map(|b| b.texture.cast::<IDXGIKeyedMutex>())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("QI IDXGIKeyedMutex (encode ring): {e}"))?;

    // Pre-cast shared encode textures once for fast CopyResource.
    let gpu_buffer_resources: Vec<ID3D11Resource> = gpu_buffers
        .iter()
        .map(|b| b.texture.cast::<ID3D11Resource>())
        .collect::<Result<_, _>>()
        .map_err(|e| format!("QI ID3D11Resource (encode ring): {e}"))?;

    // Encode-side private texture copy context:
    //   shared slot (render-owned) -> private texture (encode-owned) -> MF WriteSample.
    //
    // This decouples shared-slot reuse timing from asynchronous MF consumption and
    // removes ring-periodic frame interleaving when the writer keeps GPU samples alive.
    let (enc_copy_device, enc_copy_context, enc_copy_fence, enc_copy_desc) =
        if let Some(first_buf) = gpu_buffers.first() {
            let device = unsafe {
                first_buf
                    .texture
                    .GetDevice()
                    .map_err(|e| format!("GetDevice (encode shared texture): {e}"))?
            };

            let context = unsafe {
                device
                    .GetImmediateContext()
                    .map_err(|e| format!("GetImmediateContext (encode): {e}"))?
            };

            let fence = D3D11GpuFence::new(&device, &context)?;

            let mut desc = D3D11_TEXTURE2D_DESC::default();
            unsafe {
                first_buf.texture.GetDesc(&mut desc);
            }
            desc.MiscFlags = 0;

            (Some(device), Some(context), Some(fence), Some(desc))
        } else {
            (None, None, None, None)
        };

    let mut encode_slow_count: u32 = 0;
    loop {
        let tw0 = Instant::now();
        let msg = match rx.recv() {
            Ok(m) => m,
            Err(_) => break,
        };
        let enc_wait_dur = tw0.elapsed().as_secs_f64();
        t_wait += enc_wait_dur;

        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let enc_frame_t0 = Instant::now();
        let timestamp_100ns = frames_encoded as i64 * frame_duration_100ns;

        // Audio interleaving.
        if let (Some(dec), Some(stream)) = (&audio_decoder, &opt_audio_stream) {
            while !audio_eof && audio_output_100ns <= timestamp_100ns {
                if config.audio_is_preprocessed {
                    match dec.read_samples() {
                        Ok(Some((pcm, _ts_100ns))) => {
                            let channels = dec.channels() as usize;
                            if channels == 0 || pcm.is_empty() {
                                continue;
                            }
                            let samples_per_channel = pcm.len() / (channels * 4);
                            if samples_per_channel == 0 {
                                continue;
                            }
                            let next_total =
                                total_audio_samples_written + samples_per_channel as u64;
                            let next_100ns = (next_total * 10_000_000) / dec.sample_rate() as u64;
                            let dur_100ns = next_100ns as i64 - audio_output_100ns;
                            if dur_100ns <= 0 {
                                continue;
                            }
                            if let Err(e) = stream.write_samples_direct(
                                encoder.writer(),
                                &pcm,
                                audio_output_100ns,
                                dur_100ns,
                            ) {
                                eprintln!("[Audio] Native mixed audio write error: {}", e);
                                audio_eof = true;
                            } else {
                                total_audio_samples_written = next_total;
                                audio_output_100ns = next_100ns as i64;
                            }
                            continue;
                        }
                        Ok(None) => {
                            audio_eof = true;
                            continue;
                        }
                        Err(e) => {
                            eprintln!("[Audio] Native mixed audio decode error: {}", e);
                            audio_eof = true;
                            continue;
                        }
                    }
                }

                let current_seg = if config.trim_segments.is_empty() {
                    Some((config.trim_start, config.trim_start + config.duration))
                } else {
                    config
                        .trim_segments
                        .get(audio_segment_idx)
                        .map(|s| (s.start_time, s.end_time))
                };
                let Some((seg_start, seg_end)) = current_seg else {
                    audio_eof = true;
                    break;
                };

                match dec.read_samples() {
                    Ok(Some((pcm, ts_100ns))) => {
                        let chunk_time = ts_100ns as f64 / 10_000_000.0;
                        if chunk_time > seg_end {
                            audio_segment_idx += 1;
                            if config.trim_segments.is_empty() {
                                audio_eof = true;
                            } else if let Some(next_seg) =
                                config.trim_segments.get(audio_segment_idx)
                            {
                                let _ = dec.seek((next_seg.start_time * 10_000_000.0) as i64);
                            }
                            continue;
                        }
                        if chunk_time < seg_start {
                            continue;
                        }
                        if chunk_time <= seg_end {
                            let channels = dec.channels() as usize;
                            let speed =
                                get_speed(chunk_time, &config.speed_points).clamp(0.1, 16.0);
                            let input_frames = if channels == 0 {
                                0
                            } else {
                                pcm.len() / (channels * 4)
                            };
                            let source_duration_sec = if input_frames == 0 {
                                0.0
                            } else {
                                input_frames as f64 / dec.sample_rate() as f64
                            };
                            let mut resampled = resample_pcm_bytes(&pcm, speed, channels);
                            apply_audio_volume_envelope(
                                &mut resampled,
                                chunk_time,
                                source_duration_sec,
                                channels,
                                &config.audio_volume_points,
                            );
                            if channels == 0 || resampled.is_empty() {
                                continue;
                            }
                            let samples_per_channel = resampled.len() / (channels * 4);
                            if samples_per_channel == 0 {
                                continue;
                            }
                            let next_total =
                                total_audio_samples_written + samples_per_channel as u64;
                            let next_100ns = (next_total * 10_000_000) / dec.sample_rate() as u64;
                            let dur_100ns = next_100ns as i64 - audio_output_100ns;
                            if dur_100ns <= 0 {
                                continue;
                            }
                            if let Err(e) = stream.write_samples_direct(
                                encoder.writer(),
                                &resampled,
                                audio_output_100ns,
                                dur_100ns,
                            ) {
                                eprintln!("[Audio] Native audio write error: {}", e);
                                audio_eof = true;
                            } else {
                                total_audio_samples_written = next_total;
                                audio_output_100ns = next_100ns as i64;
                            }
                        }
                    }
                    Ok(None) => audio_eof = true,
                    Err(e) => {
                        eprintln!("[Audio] Native audio decode error: {}", e);
                        audio_eof = true;
                    }
                }
            }
        }

        // Video encode: GPU or CPU path based on message variant.
        let te0 = Instant::now();
        match msg {
            RenderOutput::Gpu { ring_idx } => {
                // Acquire D3D11-side keyed mutex -- cache-invalidates the DX12-written
                // data so the MF encoder reads the correct frame, not stale L2 data.
                if !gpu_enc_keyed_mutexes.is_empty() {
                    unsafe {
                        gpu_enc_keyed_mutexes[ring_idx]
                            .AcquireSync(0, u32::MAX)
                            .map_err(|e| format!("AcquireSync D3D11 enc[{ring_idx}]: {e}"))?;
                    }
                }

                let private_texture =
                    if let (Some(device), Some(context), Some(fence), Some(desc)) = (
                        &enc_copy_device,
                        &enc_copy_context,
                        &enc_copy_fence,
                        &enc_copy_desc,
                    ) {
                        let mut copied_opt: Option<ID3D11Texture2D> = None;
                        unsafe {
                            device
                                .CreateTexture2D(desc, None, Some(&mut copied_opt))
                                .map_err(|e| format!("CreateTexture2D encode-private: {e}"))?;
                        }
                        let copied =
                            copied_opt.ok_or("CreateTexture2D encode-private returned null")?;

                        let copied_res: ID3D11Resource = copied
                            .cast()
                            .map_err(|e| format!("QI encode-private ID3D11Resource: {e}"))?;
                        unsafe {
                            context.CopyResource(&copied_res, &gpu_buffer_resources[ring_idx]);
                        }
                        // Ensure the shared->private copy is complete before releasing the
                        // shared slot back to the render thread.
                        fence.signal_and_wait();
                        copied
                    } else {
                        gpu_buffers[ring_idx].texture.clone()
                    };

                // Shared slot is no longer needed once the private copy is complete.
                if !gpu_enc_keyed_mutexes.is_empty() {
                    unsafe {
                        let _ = gpu_enc_keyed_mutexes[ring_idx].ReleaseSync(0);
                    }
                }
                let _ = gpu_recycle_tx.send(ring_idx);

                encoder.write_frame_gpu(&private_texture, timestamp_100ns, frame_duration_100ns)?;
            }
            RenderOutput::Cpu { rendered_bgra } => {
                encoder.write_frame_cpu(&rendered_bgra, timestamp_100ns, frame_duration_100ns)?;
                let _ = cpu_recycle_tx.send(rendered_bgra);
            }
        }
        t_encode += te0.elapsed().as_secs_f64();

        let enc_frame_dur = enc_frame_t0.elapsed().as_secs_f64();
        if enc_frame_dur > 0.5 || enc_wait_dur > 0.5 {
            encode_slow_count += 1;
            eprintln!(
                "[Encode] SLOW f{}/{total_frames} wait={enc_wait_dur:.3}s encode={enc_frame_dur:.3}s",
                frames_encoded + 1
            );
        }

        frames_encoded += 1;

        if let Some(ref cb) = progress
            && (frames_encoded.is_multiple_of(15) || frames_encoded == total_frames)
        {
            let elapsed = start.elapsed().as_secs_f64();
            let pct = (frames_encoded as f64 / total_frames as f64 * 100.0).min(100.0);
            let eta = if frames_encoded > 0 {
                (elapsed / frames_encoded as f64) * (total_frames - frames_encoded) as f64
            } else {
                0.0
            };
            cb(pct, eta);
        }
    }

    encoder.finalize()?;

    let elapsed = start.elapsed().as_secs_f64();
    let fps = frames_encoded as f64 / elapsed;
    let n = frames_encoded.max(1) as f64;

    if encode_slow_count > 0 {
        eprintln!("[Encode] {encode_slow_count} slow frames (>0.5s each)");
    }
    println!(
        "[Encode] {} frames: encode {:.1} + wait {:.1}ms avg",
        frames_encoded,
        t_encode / n * 1000.0,
        t_wait / n * 1000.0,
    );
    println!(
        "[Pipeline] Done: {} frames in {:.2}s ({:.1} fps)",
        frames_encoded, elapsed, fps
    );

    Ok(ZeroCopyExportResult {
        frames_encoded,
        elapsed_secs: elapsed,
        fps,
    })
}
