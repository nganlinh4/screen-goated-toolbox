use std::path::Path;
use std::time::Instant;

use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
use windows::core::Interface;

use super::config::compute_default_video_bitrate_kbps;
use crate::overlay::screen_record::d3d_interop::{D3D11GpuFence, create_d3d11_device};
use crate::overlay::screen_record::mf_audio::{AudioConfig, AudioStream, MfAudioDecoder};
use crate::overlay::screen_record::mf_decode::{self, DecodedFrame, DxgiDeviceManager, MfDecoder};
use crate::overlay::screen_record::mf_encode::{
    EncoderConfig, MfEncoder, VideoCodec, VideoInputSurfaceFormat,
};

const AUDIO_SAMPLE_RATE: u32 = 48_000;
const AUDIO_CHANNELS: u32 = 2;
const AUDIO_BITRATE_KBPS: u32 = 192;
const REORDER_WINDOW: usize = 6;
const SILENCE_CHUNK_FRAMES: u64 = 4_800;

pub(crate) struct StitchClip<'a> {
    pub path: &'a Path,
    pub trim_start_sec: f64,
    pub duration_sec: f64,
}

pub(crate) struct StitchConfig {
    pub width: u32,
    pub height: u32,
    pub framerate: u32,
    pub bitrate_kbps: u32,
}

pub(crate) fn stitch_clips_to_mp4(
    clips: &[StitchClip<'_>],
    output_path: &Path,
    config: &StitchConfig,
) -> Result<(), String> {
    if clips.is_empty() {
        return Err("No clips to stitch".to_string());
    }
    if config.width == 0 || config.height == 0 || config.framerate == 0 {
        return Err("Native stitcher received invalid output geometry".to_string());
    }

    let started_at = Instant::now();
    mf_decode::mf_startup()?;

    let (d3d11_device, d3d11_context) = create_d3d11_device()?;
    {
        let multithread: ID3D11Multithread = d3d11_device
            .cast()
            .map_err(|e| format!("QI ID3D11Multithread stitch: {e}"))?;
        unsafe {
            let _ = multithread.SetMultithreadProtected(true);
        }
    }
    let device_manager = DxgiDeviceManager::new(&d3d11_device)?;
    let copy_fence = D3D11GpuFence::new(&d3d11_device, &d3d11_context)?;

    let audio_config = AudioConfig {
        sample_rate: AUDIO_SAMPLE_RATE,
        channels: AUDIO_CHANNELS,
        bitrate_kbps: AUDIO_BITRATE_KBPS,
    };
    let encoder_config = EncoderConfig {
        codec: VideoCodec::H264,
        input_surface_format: VideoInputSurfaceFormat::Nv12,
        width: config.width,
        height: config.height,
        fps_num: config.framerate,
        fps_den: 1,
        bitrate_kbps: if config.bitrate_kbps > 0 {
            config.bitrate_kbps
        } else {
            compute_default_video_bitrate_kbps(config.width, config.height, config.framerate)
        },
    };
    let output_path_str = output_path.to_string_lossy().to_string();
    let (encoder, audio_stream) = MfEncoder::new(
        &output_path_str,
        encoder_config,
        &device_manager,
        Some(&audio_config),
    )?;
    let audio_stream = audio_stream.ok_or("Native stitcher failed to create audio stream")?;
    let frame_duration_100ns = encoder.frame_duration_100ns().max(1);

    let mut frames_written = 0u64;
    let mut audio_cursor_100ns = 0i64;
    let mut timeline_cursor_100ns = 0i64;

    for (clip_index, clip) in clips.iter().enumerate() {
        if !clip.path.exists() {
            return Err(format!("Clip source missing: {}", clip.path.display()));
        }
        let target_frames = (clip.duration_sec.max(0.0) * config.framerate as f64).round() as u64;
        if target_frames == 0 {
            continue;
        }

        let clip_path = clip.path.to_string_lossy().to_string();
        let video_decoder = MfDecoder::new(&clip_path, &device_manager, true)?;
        if clip.trim_start_sec > 0.0 {
            video_decoder.seek_seconds(clip.trim_start_sec)?;
        }

        let mut audio_decoder = open_stitch_audio_decoder(&clip_path, clip.trim_start_sec);
        let mut pending_audio = if let Some(decoder) = &audio_decoder {
            decoder.read_samples().unwrap_or(None)
        } else {
            None
        };

        let mut reorder_queue = Vec::with_capacity(REORDER_WINDOW);
        let mut video_eof = false;
        let mut clip_frames_written = 0u64;

        while clip_frames_written < target_frames {
            let Some(frame) = next_video_frame_in_presentation_order(
                &video_decoder,
                &mut reorder_queue,
                &mut video_eof,
            )?
            else {
                break;
            };

            let frame_time_sec = frame.pts_100ns as f64 / 10_000_000.0;
            if frame_time_sec + 0.0005 < clip.trim_start_sec {
                continue;
            }
            if frame_time_sec >= clip.trim_start_sec + clip.duration_sec + 0.0005 {
                break;
            }

            let video_timestamp_100ns = frames_written as i64 * frame_duration_100ns;
            if let Some(decoder) = audio_decoder.as_mut() {
                drain_audio_until(
                    StitchAudioDrain {
                        stream: &audio_stream,
                        encoder: &encoder,
                        decoder,
                        pending_audio: &mut pending_audio,
                        clip_trim_start_sec: clip.trim_start_sec,
                        clip_duration_sec: clip.duration_sec,
                        clip_timeline_start_100ns: timeline_cursor_100ns,
                        audio_cursor_100ns: &mut audio_cursor_100ns,
                    },
                    video_timestamp_100ns,
                )?;
            } else {
                write_silence_until(
                    &audio_stream,
                    &encoder,
                    video_timestamp_100ns,
                    &mut audio_cursor_100ns,
                )?;
            }

            let private_texture =
                copy_frame_to_private_texture(&d3d11_device, &d3d11_context, &copy_fence, &frame)?;
            encoder.write_frame_gpu(
                &private_texture,
                video_timestamp_100ns,
                frame_duration_100ns,
            )?;
            frames_written += 1;
            clip_frames_written += 1;
        }

        let clip_end_100ns = timeline_cursor_100ns + target_frames as i64 * frame_duration_100ns;
        if let Some(decoder) = audio_decoder.as_mut() {
            drain_audio_until(
                StitchAudioDrain {
                    stream: &audio_stream,
                    encoder: &encoder,
                    decoder,
                    pending_audio: &mut pending_audio,
                    clip_trim_start_sec: clip.trim_start_sec,
                    clip_duration_sec: clip.duration_sec,
                    clip_timeline_start_100ns: timeline_cursor_100ns,
                    audio_cursor_100ns: &mut audio_cursor_100ns,
                },
                clip_end_100ns,
            )?;
        }
        write_silence_until(
            &audio_stream,
            &encoder,
            clip_end_100ns,
            &mut audio_cursor_100ns,
        )?;

        println!(
            "[NativeStitch] clip {}/{} frames={} target={} audio_end={:.3}s",
            clip_index + 1,
            clips.len(),
            clip_frames_written,
            target_frames,
            audio_cursor_100ns as f64 / 10_000_000.0
        );
        timeline_cursor_100ns = clip_end_100ns;
    }

    encoder.finalize()?;
    println!(
        "[NativeStitch] Wrote {} frames in {:.3}s → {}",
        frames_written,
        started_at.elapsed().as_secs_f64(),
        output_path.display()
    );
    Ok(())
}

fn open_stitch_audio_decoder(path: &str, trim_start_sec: f64) -> Option<MfAudioDecoder> {
    let decoder =
        MfAudioDecoder::new_with_output_format(path, Some(AUDIO_SAMPLE_RATE), Some(AUDIO_CHANNELS))
            .ok()?;
    if trim_start_sec > 0.0 {
        let _ = decoder.seek((trim_start_sec * 10_000_000.0) as i64);
    }
    Some(decoder)
}

fn next_video_frame_in_presentation_order(
    decoder: &MfDecoder,
    reorder_queue: &mut Vec<DecodedFrame>,
    eof_reached: &mut bool,
) -> Result<Option<DecodedFrame>, String> {
    while reorder_queue.len() < REORDER_WINDOW && !*eof_reached {
        match decoder.read_frame()? {
            Some(frame) => reorder_queue.push(frame),
            None => *eof_reached = true,
        }
    }
    reorder_queue.sort_by(|left, right| right.pts_100ns.cmp(&left.pts_100ns));
    Ok(reorder_queue.pop())
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
    let private_desc = D3D11_TEXTURE2D_DESC {
        Width: desc.Width,
        Height: desc.Height,
        MipLevels: 1,
        ArraySize: 1,
        Format: desc.Format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: 0,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };

    let mut texture: Option<ID3D11Texture2D> = None;
    unsafe {
        device
            .CreateTexture2D(&private_desc, None, Some(&mut texture))
            .map_err(|e| {
                format!(
                    "CreateTexture2D stitch-private {}x{} fmt={:?} src_array={} src_mips={}: {e}",
                    private_desc.Width,
                    private_desc.Height,
                    private_desc.Format,
                    desc.ArraySize,
                    desc.MipLevels
                )
            })?;
    }
    let texture = texture.ok_or("CreateTexture2D stitch-private returned null")?;

    let source_resource: ID3D11Resource = frame
        .texture
        .cast()
        .map_err(|e| format!("frame texture -> ID3D11Resource: {e}"))?;
    let dest_resource: ID3D11Resource = texture
        .cast()
        .map_err(|e| format!("private texture -> ID3D11Resource: {e}"))?;

    unsafe {
        context.CopySubresourceRegion(
            &dest_resource,
            0,
            0,
            0,
            0,
            &source_resource,
            frame.subresource_index,
            None,
        );
    }
    copy_fence.signal_and_wait();
    Ok(texture)
}

struct StitchAudioDrain<'a> {
    stream: &'a AudioStream,
    encoder: &'a MfEncoder,
    decoder: &'a mut MfAudioDecoder,
    pending_audio: &'a mut Option<(Vec<u8>, i64)>,
    clip_trim_start_sec: f64,
    clip_duration_sec: f64,
    clip_timeline_start_100ns: i64,
    audio_cursor_100ns: &'a mut i64,
}

fn drain_audio_until(context: StitchAudioDrain<'_>, target_100ns: i64) -> Result<(), String> {
    let StitchAudioDrain {
        stream,
        encoder,
        decoder,
        pending_audio,
        clip_trim_start_sec,
        clip_duration_sec,
        clip_timeline_start_100ns,
        audio_cursor_100ns,
    } = context;
    loop {
        let Some((pcm, timestamp_100ns)) = pending_audio.take() else {
            break;
        };
        let source_time_sec = timestamp_100ns as f64 / 10_000_000.0;
        if source_time_sec >= clip_trim_start_sec + clip_duration_sec {
            *pending_audio = Some((pcm, timestamp_100ns));
            break;
        }

        if source_time_sec + 0.0005 >= clip_trim_start_sec {
            let relative_100ns =
                ((source_time_sec - clip_trim_start_sec) * 10_000_000.0).round() as i64;
            let desired_start_100ns = clip_timeline_start_100ns + relative_100ns.max(0);
            write_silence_until(stream, encoder, desired_start_100ns, audio_cursor_100ns)?;

            if *audio_cursor_100ns <= target_100ns {
                write_audio_chunk(stream, encoder, &pcm, audio_cursor_100ns)?;
            } else {
                *pending_audio = Some((pcm, timestamp_100ns));
                break;
            }
        }

        *pending_audio = decoder.read_samples()?;
        if *audio_cursor_100ns > target_100ns {
            break;
        }
    }
    Ok(())
}

fn write_audio_chunk(
    stream: &AudioStream,
    encoder: &MfEncoder,
    pcm: &[u8],
    audio_cursor_100ns: &mut i64,
) -> Result<(), String> {
    let bytes_per_frame = AUDIO_CHANNELS as usize * 4;
    if pcm.len() < bytes_per_frame {
        return Ok(());
    }
    let frames = (pcm.len() / bytes_per_frame) as u64;
    let bytes = frames as usize * bytes_per_frame;
    let duration_100ns = ((frames * 10_000_000) / AUDIO_SAMPLE_RATE as u64).max(1) as i64;
    stream.write_samples_direct(
        encoder.writer(),
        &pcm[..bytes],
        *audio_cursor_100ns,
        duration_100ns,
    )?;
    *audio_cursor_100ns += duration_100ns;
    Ok(())
}

fn write_silence_until(
    stream: &AudioStream,
    encoder: &MfEncoder,
    target_100ns: i64,
    audio_cursor_100ns: &mut i64,
) -> Result<(), String> {
    let bytes_per_frame = AUDIO_CHANNELS as usize * 4;
    while *audio_cursor_100ns < target_100ns {
        let remaining_100ns = target_100ns - *audio_cursor_100ns;
        let remaining_frames =
            ((remaining_100ns as u64 * AUDIO_SAMPLE_RATE as u64) / 10_000_000).max(1);
        let frames = remaining_frames.min(SILENCE_CHUNK_FRAMES);
        let silence = vec![0u8; frames as usize * bytes_per_frame];
        let duration_100ns = ((frames * 10_000_000) / AUDIO_SAMPLE_RATE as u64).max(1) as i64;
        stream.write_samples_direct(
            encoder.writer(),
            &silence,
            *audio_cursor_100ns,
            duration_100ns.min(remaining_100ns),
        )?;
        *audio_cursor_100ns += duration_100ns.min(remaining_100ns);
    }
    Ok(())
}
