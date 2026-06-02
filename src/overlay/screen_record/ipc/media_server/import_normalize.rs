use std::path::Path;
use std::time::Instant;

use windows::Win32::Graphics::Direct3D11::*;
use windows::core::Interface;

mod audio_decode;

use crate::overlay::screen_record::d3d_interop::{D3D11GpuFence, create_d3d11_device};
use crate::overlay::screen_record::mf_audio::{AudioConfig, AudioStream};
use crate::overlay::screen_record::mf_decode::{self, DecodedFrame, DxgiDeviceManager, MfDecoder};
use crate::overlay::screen_record::mf_encode::{
    EncoderConfig, MfEncoder, VideoCodec, VideoInputSurfaceFormat,
};
use crate::overlay::screen_record::native_export::config::compute_default_video_bitrate_kbps;

use self::audio_decode::{ImportAudioDecoder, open_import_audio_decoder, probe_has_audio_track};
use super::NORMALIZED_IMPORT_AUDIO_BITRATE_KBPS;

const IMPORT_REORDER_WINDOW: usize = 6;

pub(super) fn probe_media_has_audio(path: &Path) -> Result<bool, String> {
    audio_decode::probe_media_has_audio(path)
}

pub(super) fn probe_audio_duration_seconds(path: &Path) -> Result<f64, String> {
    audio_decode::probe_audio_duration_seconds(path)
}

pub(super) fn normalize_imported_video_mf(
    input_path: &Path,
    output_path: &Path,
    trace_id: &str,
) -> Result<bool, String> {
    let started_at = Instant::now();
    let input_path_str = input_path.to_string_lossy().to_string();
    let output_path_str = output_path.to_string_lossy().to_string();

    mf_decode::mf_startup()?;
    let metadata = mf_decode::probe_video_metadata(&input_path_str)?;
    if metadata.width == 0 || metadata.height == 0 {
        return Err("Media Foundation probe returned zero-sized video".to_string());
    }
    if metadata.width % 2 != 0 || metadata.height % 2 != 0 {
        return Err(format!(
            "Media Foundation import normalize requires even dimensions, got {}x{}",
            metadata.width, metadata.height
        ));
    }

    let fps_num = metadata.fps_num.max(1);
    let fps_den = metadata.fps_den.max(1);
    let nominal_fps = ((fps_num as f64 / fps_den as f64).round() as u32).max(1);
    let bitrate_kbps =
        compute_default_video_bitrate_kbps(metadata.width, metadata.height, nominal_fps);
    let nominal_frame_duration_100ns = ((10_000_000i64 * fps_den as i64) / fps_num as i64).max(1);

    crate::log_info!(
        "[VideoImport:{}][Normalize][MF] start input=\"{}\" output=\"{}\" {}x{} fps={}/{} bitrate_kbps={}",
        trace_id,
        input_path.display(),
        output_path.display(),
        metadata.width,
        metadata.height,
        fps_num,
        fps_den,
        bitrate_kbps
    );

    let (d3d11_device, d3d11_context) = create_d3d11_device()?;
    {
        let multithread: ID3D11Multithread = d3d11_device
            .cast()
            .map_err(|e| format!("QI ID3D11Multithread: {e}"))?;
        unsafe {
            let _ = multithread.SetMultithreadProtected(true);
        }
    }
    let device_manager = DxgiDeviceManager::new(&d3d11_device)?;
    let copy_fence = D3D11GpuFence::new(&d3d11_device, &d3d11_context)?;
    let video_decoder = MfDecoder::new(&input_path_str, &device_manager, true)?;

    let has_audio = probe_has_audio_track(&input_path_str)?;
    let mut audio_decoder = if has_audio {
        Some(open_import_audio_decoder(&input_path_str, trace_id)?)
    } else {
        None
    };
    let audio_config = audio_decoder.as_ref().map(|decoder| AudioConfig {
        sample_rate: decoder.sample_rate(),
        channels: decoder.channels(),
        bitrate_kbps: NORMALIZED_IMPORT_AUDIO_BITRATE_KBPS,
    });

    let encoder_config = EncoderConfig {
        codec: VideoCodec::H264,
        input_surface_format: VideoInputSurfaceFormat::Nv12,
        width: metadata.width,
        height: metadata.height,
        fps_num,
        fps_den,
        bitrate_kbps,
    };
    let (encoder, audio_stream) = MfEncoder::new(
        &output_path_str,
        encoder_config,
        &device_manager,
        audio_config.as_ref(),
    )?;

    let mut reorder_queue = Vec::with_capacity(IMPORT_REORDER_WINDOW);
    let mut video_eof = false;
    let mut pending_frame =
        next_video_frame_in_presentation_order(&video_decoder, &mut reorder_queue, &mut video_eof)?
            .ok_or_else(|| "No video frames decoded from imported file".to_string())?;

    let mut pending_audio_sample = if let Some(decoder) = audio_decoder.as_mut() {
        decoder.read_samples()?
    } else {
        None
    };

    let base_pts_100ns = pending_audio_sample
        .as_ref()
        .map(|(_, ts)| *ts)
        .map(|audio_ts| audio_ts.min(pending_frame.pts_100ns))
        .unwrap_or(pending_frame.pts_100ns);

    let mut audio_chunks_written = 0u64;
    let mut last_audio_end_100ns = 0i64;
    let mut last_video_end_100ns = 0i64;
    let mut video_frames_encoded = 0u64;
    {
        let mut video_encode = VideoEncodeContext {
            encoder: &encoder,
            device: &d3d11_device,
            context: &d3d11_context,
            copy_fence: &copy_fence,
            base_pts_100ns,
            last_video_end_100ns: &mut last_video_end_100ns,
        };

        loop {
            let next_frame = next_video_frame_in_presentation_order(
                &video_decoder,
                &mut reorder_queue,
                &mut video_eof,
            )?;
            let Some(next_frame) = next_frame else {
                break;
            };

            let frame_duration_100ns =
                (next_frame.pts_100ns - pending_frame.pts_100ns).max(nominal_frame_duration_100ns);
            if let (Some(decoder), Some(stream)) = (audio_decoder.as_mut(), audio_stream.as_ref()) {
                let current_video_timestamp_100ns =
                    (pending_frame.pts_100ns - base_pts_100ns).max(0);
                drain_audio_until_target(
                    AudioDrainContext {
                        stream,
                        encoder: &encoder,
                        decoder,
                        pending_audio_sample: &mut pending_audio_sample,
                        base_pts_100ns,
                        last_audio_end_100ns: &mut last_audio_end_100ns,
                        audio_chunks_written: &mut audio_chunks_written,
                    },
                    current_video_timestamp_100ns,
                )?;
            }
            encode_video_frame(&mut video_encode, &pending_frame, frame_duration_100ns)?;
            video_frames_encoded += 1;
            pending_frame = next_frame;
        }

        encode_video_frame(
            &mut video_encode,
            &pending_frame,
            nominal_frame_duration_100ns,
        )?;
        video_frames_encoded += 1;
    }

    if let (Some(decoder), Some(stream)) = (audio_decoder.as_mut(), audio_stream.as_ref()) {
        let flush_target_100ns = last_video_end_100ns.max(0);
        drain_audio_until_target(
            AudioDrainContext {
                stream,
                encoder: &encoder,
                decoder,
                pending_audio_sample: &mut pending_audio_sample,
                base_pts_100ns,
                last_audio_end_100ns: &mut last_audio_end_100ns,
                audio_chunks_written: &mut audio_chunks_written,
            },
            flush_target_100ns,
        )?;

        while let Some((pcm, timestamp_100ns)) = pending_audio_sample.take() {
            if write_audio_chunk(
                stream,
                &encoder,
                decoder,
                &pcm,
                timestamp_100ns,
                base_pts_100ns,
                &mut last_audio_end_100ns,
            )? {
                audio_chunks_written += 1;
            }
            pending_audio_sample = decoder.read_samples()?;
        }
    }
    encoder.finalize()?;

    crate::log_info!(
        "[VideoImport:{}][Normalize][MF] complete {:.3}s has_audio={} video_frames={} audio_chunks={}",
        trace_id,
        started_at.elapsed().as_secs_f64(),
        has_audio,
        video_frames_encoded,
        audio_chunks_written
    );

    Ok(has_audio)
}

fn next_video_frame_in_presentation_order(
    decoder: &MfDecoder,
    reorder_queue: &mut Vec<DecodedFrame>,
    eof_reached: &mut bool,
) -> Result<Option<DecodedFrame>, String> {
    while reorder_queue.len() < IMPORT_REORDER_WINDOW && !*eof_reached {
        match decoder.read_frame()? {
            Some(frame) => reorder_queue.push(frame),
            None => *eof_reached = true,
        }
    }
    reorder_queue.sort_by(|left, right| right.pts_100ns.cmp(&left.pts_100ns));
    Ok(reorder_queue.pop())
}

struct VideoEncodeContext<'a> {
    encoder: &'a MfEncoder,
    device: &'a ID3D11Device,
    context: &'a ID3D11DeviceContext,
    copy_fence: &'a D3D11GpuFence,
    base_pts_100ns: i64,
    last_video_end_100ns: &'a mut i64,
}

fn encode_video_frame(
    context: &mut VideoEncodeContext<'_>,
    frame: &DecodedFrame,
    duration_100ns: i64,
) -> Result<(), String> {
    let texture =
        copy_frame_to_private_texture(context.device, context.context, context.copy_fence, frame)?;
    let mut timestamp_100ns = (frame.pts_100ns - context.base_pts_100ns).max(0);
    if timestamp_100ns < *context.last_video_end_100ns {
        timestamp_100ns = *context.last_video_end_100ns;
    }
    let duration_100ns = duration_100ns.max(1);
    context
        .encoder
        .write_frame_gpu(&texture, timestamp_100ns, duration_100ns)?;
    *context.last_video_end_100ns = timestamp_100ns + duration_100ns;
    Ok(())
}

fn copy_frame_to_private_texture(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    copy_fence: &D3D11GpuFence,
    frame: &DecodedFrame,
) -> Result<ID3D11Texture2D, String> {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe {
        frame.texture.GetDesc(&mut desc);
    }
    desc.Usage = D3D11_USAGE_DEFAULT;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = 0;
    desc.MiscFlags = 0;

    let mut texture: Option<ID3D11Texture2D> = None;
    unsafe {
        device
            .CreateTexture2D(&desc, None, Some(&mut texture))
            .map_err(|e| format!("CreateTexture2D import-private: {e}"))?;
    }
    let texture = texture.ok_or("CreateTexture2D import-private returned null")?;

    let source_resource: ID3D11Resource = frame
        .texture
        .cast()
        .map_err(|e| format!("frame texture -> ID3D11Resource: {e}"))?;
    let dest_resource: ID3D11Resource = texture
        .cast()
        .map_err(|e| format!("private texture -> ID3D11Resource: {e}"))?;

    unsafe {
        context.CopyResource(&dest_resource, &source_resource);
    }
    copy_fence.signal_and_wait();

    Ok(texture)
}

fn write_audio_chunk(
    stream: &AudioStream,
    encoder: &MfEncoder,
    decoder: &ImportAudioDecoder,
    pcm_data: &[u8],
    timestamp_100ns: i64,
    base_pts_100ns: i64,
    last_audio_end_100ns: &mut i64,
) -> Result<bool, String> {
    let channels = decoder.channels() as usize;
    if channels == 0 || pcm_data.is_empty() {
        return Ok(false);
    }

    let samples_per_channel = pcm_data.len() / (channels * 4);
    if samples_per_channel == 0 {
        return Ok(false);
    }

    let duration_100ns =
        ((samples_per_channel as i64) * 10_000_000 / decoder.sample_rate() as i64).max(1);
    let mut relative_timestamp_100ns = (timestamp_100ns - base_pts_100ns).max(0);
    if relative_timestamp_100ns < *last_audio_end_100ns {
        relative_timestamp_100ns = *last_audio_end_100ns;
    }

    stream.write_samples_direct(
        encoder.writer(),
        pcm_data,
        relative_timestamp_100ns,
        duration_100ns,
    )?;
    *last_audio_end_100ns = relative_timestamp_100ns + duration_100ns;
    Ok(true)
}

struct AudioDrainContext<'a> {
    stream: &'a AudioStream,
    encoder: &'a MfEncoder,
    decoder: &'a mut ImportAudioDecoder,
    pending_audio_sample: &'a mut Option<(Vec<u8>, i64)>,
    base_pts_100ns: i64,
    last_audio_end_100ns: &'a mut i64,
    audio_chunks_written: &'a mut u64,
}

fn drain_audio_until_target(
    context: AudioDrainContext<'_>,
    target_timestamp_100ns: i64,
) -> Result<(), String> {
    let AudioDrainContext {
        stream,
        encoder,
        decoder,
        pending_audio_sample,
        base_pts_100ns,
        last_audio_end_100ns,
        audio_chunks_written,
    } = context;
    loop {
        let should_write = pending_audio_sample
            .as_ref()
            .map(|(_, timestamp_100ns)| {
                (*timestamp_100ns - base_pts_100ns).max(0) <= target_timestamp_100ns
            })
            .unwrap_or(false);
        if !should_write {
            break;
        }

        let (pcm, timestamp_100ns) = pending_audio_sample
            .take()
            .ok_or_else(|| "Missing pending audio sample".to_string())?;
        if write_audio_chunk(
            stream,
            encoder,
            decoder,
            &pcm,
            timestamp_100ns,
            base_pts_100ns,
            last_audio_end_100ns,
        )? {
            *audio_chunks_written += 1;
        }
        *pending_audio_sample = decoder.read_samples()?;
    }

    Ok(())
}
