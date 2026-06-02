use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::overlay::screen_record::d3d_interop::create_d3d11_device;
use crate::overlay::screen_record::mf_decode::{DxgiDeviceManager, mf_startup};
use crate::overlay::screen_record::mf_encode::{
    EncoderConfig, MfEncoder, VideoCodec, VideoInputSurfaceFormat,
};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::core::Interface;

use super::{import_normalize, recordings_dir};

const SUPPORTED_AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "mp3", "flac", "ogg", "m4a", "aac", "alac", "aiff", "aif", "wma", "opus", "mka",
];

pub(super) fn managed_import_audio_path(
    recordings_dir: &Path,
    ts: u128,
    extension: &str,
) -> PathBuf {
    recordings_dir.join(format!("imported-audio-{ts}.{extension}"))
}

fn managed_audio_placeholder_video_path(recordings_dir: &Path, ts: u128) -> PathBuf {
    recordings_dir.join(format!("imported-audio-placeholder-{ts}.mp4"))
}

pub(super) fn normalized_audio_extension(raw: &str) -> &'static str {
    let lower = raw.to_ascii_lowercase();
    SUPPORTED_AUDIO_EXTENSIONS
        .iter()
        .copied()
        .find(|candidate| *candidate == lower.as_str())
        .unwrap_or("mp3")
}

pub fn create_audio_placeholder_video(duration_sec: f64, trace_id: &str) -> Result<String, String> {
    let duration_sec = duration_sec.max(0.1);
    let recordings_dir = recordings_dir();
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|error| format!("Create recordings dir: {error}"))?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let output_path = managed_audio_placeholder_video_path(&recordings_dir, ts);
    let output_path_arg = output_path.to_string_lossy().to_string();
    let started_at = Instant::now();

    mf_startup()?;
    let (d3d11_device, _d3d11_context) = create_d3d11_device()?;
    let multithread: ID3D11Multithread = d3d11_device
        .cast()
        .map_err(|e| format!("QI ID3D11Multithread: {e}"))?;
    unsafe {
        let _ = multithread.SetMultithreadProtected(true);
    }
    let device_manager = DxgiDeviceManager::new(&d3d11_device)?;

    let width = 640u32;
    let height = 360u32;
    let fps = 1u32;
    let frame_duration_100ns = 10_000_000i64 / fps as i64;
    let total_duration_100ns = (duration_sec * 10_000_000.0).ceil() as i64;
    let frame_count =
        ((total_duration_100ns + frame_duration_100ns - 1) / frame_duration_100ns).max(1) as u32;
    let black_frame = vec![0u8; (width * height * 4) as usize];
    let initial_data = D3D11_SUBRESOURCE_DATA {
        pSysMem: black_frame.as_ptr() as *const _,
        SysMemPitch: width * 4,
        SysMemSlicePitch: 0,
    };
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut texture = None;
    unsafe {
        d3d11_device
            .CreateTexture2D(&desc, Some(&initial_data), Some(&mut texture))
            .map_err(|e| format!("Create black placeholder texture: {e}"))?;
    }
    let texture = texture.ok_or("CreateTexture2D returned null")?;

    let encoder_config = EncoderConfig {
        codec: VideoCodec::H264,
        input_surface_format: VideoInputSurfaceFormat::Argb32,
        width,
        height,
        fps_num: fps,
        fps_den: 1,
        bitrate_kbps: 120,
    };
    let (encoder, _) = MfEncoder::new(&output_path_arg, encoder_config, &device_manager, None)?;
    for frame_idx in 0..frame_count {
        let timestamp_100ns = frame_idx as i64 * frame_duration_100ns;
        let duration_100ns = if frame_idx + 1 == frame_count {
            (total_duration_100ns - timestamp_100ns).max(1)
        } else {
            frame_duration_100ns
        };
        encoder.write_frame_gpu(&texture, timestamp_100ns, duration_100ns)?;
    }
    encoder.finalize()?;

    crate::log_info!(
        "[AudioImport:{}][PlaceholderVideo][MF] complete total {:.3}s output=\"{}\" duration={:.3}s frames={}",
        trace_id,
        started_at.elapsed().as_secs_f64(),
        output_path.display(),
        duration_sec,
        frame_count
    );
    Ok(output_path.to_string_lossy().to_string())
}

pub fn import_audio_path_to_managed_media_file(
    source_path: &Path,
    trace_id: &str,
) -> Result<(String, f64), String> {
    if !source_path.exists() || !source_path.is_file() {
        return Err(format!("Audio file not found: {}", source_path.display()));
    }

    let recordings_dir = recordings_dir();
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|error| format!("Create recordings dir: {error}"))?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let raw_ext = source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty())
        .unwrap_or("mp3");
    let extension = normalized_audio_extension(raw_ext);
    let output_path = managed_import_audio_path(&recordings_dir, ts, extension);
    let started_at = Instant::now();

    std::fs::copy(source_path, &output_path).map_err(|error| {
        format!(
            "Copy imported audio failed from '{}' to '{}': {error}",
            source_path.display(),
            output_path.display()
        )
    })?;
    let duration_sec =
        import_normalize::probe_audio_duration_seconds(&output_path).unwrap_or_else(|err| {
            crate::log_info!(
                "[AudioImport:{}][Path] duration probe failed: {} - falling back to 0",
                trace_id,
                err
            );
            0.0
        });
    crate::log_info!(
        "[AudioImport:{}][Path] complete total {:.3}s file=\"{}\" output=\"{}\" duration={:.3}s",
        trace_id,
        started_at.elapsed().as_secs_f64(),
        source_path.display(),
        output_path.display(),
        duration_sec
    );

    Ok((output_path.to_string_lossy().to_string(), duration_sec))
}
