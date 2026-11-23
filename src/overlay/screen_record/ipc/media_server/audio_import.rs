//! Audio imports (HTTP upload and native path share one routine) and the black
//! placeholder video that lets an audio-only import open in the editor.

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::overlay::screen_record::d3d_interop::create_d3d11_device;
use crate::overlay::screen_record::mf_decode::{DxgiDeviceManager, mf_startup};
use crate::overlay::screen_record::mf_encode::{
    EncoderConfig, MfEncoder, VideoCodec, VideoInputSurfaceFormat,
};
use crate::overlay::screen_record::owned_file::OwnedFile;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::core::Interface;

use super::media_probe::probe_audio_duration_seconds;
use super::{recordings_dir, unique_import_stamp};

pub(super) struct ImportedAudio {
    pub(super) path: PathBuf,
    pub(super) duration_sec: f64,
    pub(super) bytes: u64,
}

/// The extension a managed copy keeps. The list is the one the host routes
/// drops by, so a container is never relabeled as another format.
pub(super) fn audio_import_extension(file_name: &str) -> Result<&'static str, String> {
    let extension = Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");
    sgt_recorder_protocol::supported_audio_extension(extension)
        .ok_or_else(|| format!("Unsupported audio file type: .{extension}"))
}

/// Fills a fresh managed file with `fill`; the copy is removed on failure. A
/// duration that cannot be probed is reported as 0 for the editor to measure.
pub(super) fn import_audio_into_managed(
    extension: &'static str,
    trace_id: &str,
    fill: impl FnOnce(&Path) -> Result<u64, String>,
) -> Result<ImportedAudio, String> {
    let recordings_dir = recordings_dir();
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|error| format!("Create recordings dir: {error}"))?;
    let output = OwnedFile::new(recordings_dir.join(format!(
        "imported-audio-{}.{extension}",
        unique_import_stamp()
    )));
    let bytes = fill(output.path())?;
    let duration_sec = probe_audio_duration_seconds(output.path()).unwrap_or_else(|err| {
        crate::log_info!(
            "[AudioImport:{}] duration probe failed: {} - falling back to 0",
            trace_id,
            err
        );
        0.0
    });
    Ok(ImportedAudio {
        path: output.keep(),
        duration_sec,
        bytes,
    })
}

const PLACEHOLDER_FRAME_100NS: i64 = 10_000_000;

/// A week of audio is already far past any real import; a longer duration is
/// a corrupt header, and at 1 fps it would encode for hours.
const MAX_PLACEHOLDER_SEC: f64 = 7.0 * 24.0 * 3600.0;

/// Total length and frame count of the 1 fps black placeholder.
fn placeholder_frame_plan(duration_sec: f64) -> Result<(i64, u32), String> {
    if !duration_sec.is_finite() || duration_sec > MAX_PLACEHOLDER_SEC {
        return Err(format!(
            "Audio duration {duration_sec}s is outside the supported placeholder length"
        ));
    }
    let duration_sec = duration_sec.max(0.1);
    let total_duration_100ns = (duration_sec * 10_000_000.0).ceil() as i64;
    let frame_count = ((total_duration_100ns + PLACEHOLDER_FRAME_100NS - 1)
        / PLACEHOLDER_FRAME_100NS)
        .max(1) as u32;
    Ok((total_duration_100ns, frame_count))
}

pub fn create_audio_placeholder_video(duration_sec: f64, trace_id: &str) -> Result<String, String> {
    let (total_duration_100ns, frame_count) = placeholder_frame_plan(duration_sec)?;
    let recordings_dir = recordings_dir();
    std::fs::create_dir_all(&recordings_dir)
        .map_err(|error| format!("Create recordings dir: {error}"))?;
    // A half-written placeholder is removed if any encoding step fails.
    let output = OwnedFile::new(recordings_dir.join(format!(
        "imported-audio-placeholder-{}.mp4",
        unique_import_stamp()
    )));
    let output_path_arg = output.path().to_string_lossy().to_string();
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
    let frame_duration_100ns = PLACEHOLDER_FRAME_100NS;
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
    let output_path = output.keep();

    crate::log_info!(
        "[AudioImport:{}][PlaceholderVideo][MF] complete total {:.3}s output=\"{}\" duration={:.3}s frames={}",
        trace_id,
        started_at.elapsed().as_secs_f64(),
        output_path.display(),
        total_duration_100ns as f64 / 10_000_000.0,
        frame_count
    );
    Ok(output_path.to_string_lossy().to_string())
}

pub fn import_audio_path_to_managed_media_file(
    source_path: &Path,
    trace_id: &str,
) -> Result<(String, f64), String> {
    if !source_path.is_file() {
        return Err(format!("Audio file not found: {}", source_path.display()));
    }
    let extension = audio_import_extension(&source_path.to_string_lossy())?;
    let started_at = Instant::now();
    let imported = import_audio_into_managed(extension, trace_id, |output| {
        std::fs::copy(source_path, output).map_err(|error| {
            format!(
                "Copy imported audio failed from '{}' to '{}': {error}",
                source_path.display(),
                output.display()
            )
        })
    })?;
    crate::log_info!(
        "[AudioImport:{}][Path] complete total {:.3}s file=\"{}\" output=\"{}\" duration={:.3}s",
        trace_id,
        started_at.elapsed().as_secs_f64(),
        source_path.display(),
        imported.path.display(),
        imported.duration_sec
    );
    Ok((
        imported.path.to_string_lossy().to_string(),
        imported.duration_sec,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_import_keeps_known_extensions_and_rejects_others() {
        assert_eq!(audio_import_extension("book.M4B").unwrap(), "m4b");
        assert_eq!(audio_import_extension("take.MKA").unwrap(), "mka");
        assert_eq!(audio_import_extension("voice.opus").unwrap(), "opus");
        assert!(audio_import_extension("song.wma").is_err());
        assert!(audio_import_extension("noext").is_err());
        // Every type the host routes to the recorder imports as itself.
        for extension in sgt_recorder_protocol::SUPPORTED_AUDIO_EXTENSIONS {
            let name = format!("drop.{}", extension.to_ascii_uppercase());
            assert_eq!(audio_import_extension(&name).unwrap(), *extension);
        }
    }

    #[test]
    fn placeholder_length_is_bounded_and_rounds_up_to_whole_frames() {
        assert_eq!(placeholder_frame_plan(2.5), Ok((25_000_000, 3)));
        assert_eq!(placeholder_frame_plan(0.0), Ok((1_000_000, 1)));
        assert!(placeholder_frame_plan(1e300).is_err(), "must not overflow");
        assert!(
            placeholder_frame_plan(1e9).is_err(),
            "a corrupt duration must not encode a billion frames"
        );
    }
}
