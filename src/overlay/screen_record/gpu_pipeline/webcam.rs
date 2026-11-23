use windows::Win32::Graphics::Direct3D11::*;
use windows::core::Interface;

use super::super::d3d_interop::{create_d3d11_device, create_d3d11_device_on_adapter};
use super::super::mf_decode::{DxgiDeviceManager, MfDecoder};
use super::super::native_export::config::{BakedWebcamFrame, TrimSegment};
use super::ring_buffers::try_create_decode_input_ring;
use super::types::{DECODE_RING_SIZE, PipelineConfig, WebcamDecodeSetup};

#[cfg(test)]
thread_local! {
    /// Simulates a device where no zero-copy ring can be created (WARP, a
    /// non-DX12 path, shared NT handles refused) for exports started on this thread.
    pub(super) static FORCE_CPU_PIPELINE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Same switch for every stream: force the CPU decode and output paths.
pub(super) fn force_cpu_decode() -> bool {
    #[cfg(test)]
    if FORCE_CPU_PIPELINE.with(|flag| flag.get()) {
        return true;
    }
    std::env::var_os("SGT_FORCE_CPU_ENCODE").is_some()
}

pub(super) fn prepare_webcam_decode_setup(
    source_times: &[f64],
    config: &PipelineConfig,
    wgpu_adapter_luid: Option<windows::Win32::Foundation::LUID>,
    wgpu_device: &wgpu::Device,
) -> Result<Option<WebcamDecodeSetup>, String> {
    let Some(path) = config
        .webcam_video_path
        .as_ref()
        .filter(|path| !path.trim().is_empty())
    else {
        return Ok(None);
    };
    if !std::path::Path::new(path).exists() || config.webcam_frames.is_empty() {
        return Ok(None);
    }
    if config.webcam_frames.len() != source_times.len() {
        return Err(format!(
            "Webcam baked frames length {} does not match export frames {}",
            config.webcam_frames.len(),
            source_times.len()
        ));
    }

    let active_mask: Vec<bool> = config
        .webcam_frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            let webcam_media_time = source_times[index] - config.webcam_offset_sec;
            webcam_media_time >= 0.0
                && frame.visible
                && frame.opacity > 0.001
                && frame.width > 0.0
                && frame.height > 0.0
        })
        .collect();
    if !active_mask.iter().any(|active| *active) {
        return Ok(None);
    }

    let mut max_width = 0.0f64;
    let mut max_height = 0.0f64;
    for frame in &config.webcam_frames {
        if !(frame.visible && frame.opacity > 0.001) {
            continue;
        }
        max_width = max_width.max(frame.width.max(0.0));
        max_height = max_height.max(frame.height.max(0.0));
    }
    if max_width <= 0.0 || max_height <= 0.0 {
        return Ok(None);
    }

    let (d3d_device, d3d_context) = match wgpu_adapter_luid {
        Some(luid) => create_d3d11_device_on_adapter(luid)?,
        None => create_d3d11_device()?,
    };
    {
        let mt: ID3D11Multithread = d3d_device
            .cast()
            .map_err(|e| format!("QI ID3D11Multithread (webcam): {e}"))?;
        unsafe {
            let _ = mt.SetMultithreadProtected(true);
        }
    }
    let device_manager = DxgiDeviceManager::new(&d3d_device)?;
    let decoder = MfDecoder::new(path, &device_manager, true)?;
    let source_width = decoder.width();
    let source_height = decoder.height();
    let render_width = (max_width.ceil() as u32).clamp(2, decoder.width().max(2));
    let render_height = (max_height.ceil() as u32).clamp(2, decoder.height().max(2));
    drop(decoder);

    let ring = if force_cpu_decode() {
        None
    } else {
        try_create_decode_input_ring(&d3d_device, wgpu_device, render_width, render_height)
    };
    match ring {
        Some(_) => println!(
            "[Export] Zero-copy GPU webcam decode path ({}-slot ring, {}x{})",
            DECODE_RING_SIZE, render_width, render_height
        ),
        None => println!(
            "[Export] Webcam decode ring unavailable, using CPU webcam decode ({}x{})",
            render_width, render_height
        ),
    }

    let (webcam_times, webcam_trim_segments) = webcam_media_timeline(
        source_times,
        &config.trim_segments,
        config.webcam_offset_sec,
    );
    Ok(Some(WebcamDecodeSetup {
        d3d_device,
        d3d_context,
        ring,
        source_times: webcam_times,
        trim_segments: webcam_trim_segments,
        active_mask,
        source_width,
        source_height,
        render_width,
        render_height,
    }))
}

/// The editor bakes one webcam layout per frame of its own frame walk, while
/// the export looks layouts up by its own frame index. Picks, for every export
/// frame, the baked layout nearest in source time. Both lists are sorted.
pub(in crate::overlay::screen_record) fn align_webcam_frames_to_output(
    frames: Vec<BakedWebcamFrame>,
    source_times: &[f64],
) -> Vec<BakedWebcamFrame> {
    if frames.is_empty() {
        return frames;
    }
    let mut nearest = 0usize;
    source_times
        .iter()
        .map(|&time| {
            while nearest + 1 < frames.len()
                && (frames[nearest + 1].time - time).abs() <= (frames[nearest].time - time).abs()
            {
                nearest += 1;
            }
            frames[nearest].clone()
        })
        .collect()
}

/// Shifts the export timeline into webcam media time. Source times and trim
/// segments must move together: the decoder seeks to segment starts and finds
/// each frame's segment by comparing the two.
fn webcam_media_timeline(
    source_times: &[f64],
    trim_segments: &[TrimSegment],
    offset_sec: f64,
) -> (Vec<f64>, Vec<TrimSegment>) {
    let times = source_times.iter().map(|time| time - offset_sec).collect();
    let segments = trim_segments
        .iter()
        .map(|segment| TrimSegment {
            start_time: segment.start_time - offset_sec,
            end_time: segment.end_time - offset_sec,
        })
        .collect();
    (times, segments)
}

#[cfg(test)]
mod tests {
    use super::{align_webcam_frames_to_output, webcam_media_timeline};
    use crate::overlay::screen_record::native_export::config::{BakedWebcamFrame, TrimSegment};

    fn baked(time: f64, x: f64) -> BakedWebcamFrame {
        BakedWebcamFrame {
            time,
            visible: true,
            opacity: 1.0,
            x,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            roundness_px: 0.0,
            shadow_px: 0.0,
            mirror: false,
            render_scale: 1.0,
        }
    }

    #[test]
    fn webcam_layouts_follow_the_export_frames_by_time() {
        // The editor baked one more frame than the export renders, and its
        // frames after the cut start a little earlier.
        let editor = vec![
            baked(0.0, 0.0),
            baked(0.5, 1.0),
            baked(5.0, 2.0),
            baked(5.5, 3.0),
            baked(6.0, 4.0),
        ];
        let export_times = [0.0, 0.5, 5.2, 5.7];
        let aligned = align_webcam_frames_to_output(editor, &export_times);
        let xs: Vec<f64> = aligned.iter().map(|frame| frame.x).collect();
        assert_eq!(xs, [0.0, 1.0, 2.0, 3.0]);
        assert!(align_webcam_frames_to_output(Vec::new(), &export_times).is_empty());
    }

    #[test]
    fn webcam_times_and_trim_segments_shift_together() {
        let segments = [
            TrimSegment {
                start_time: 5.0,
                end_time: 6.0,
            },
            TrimSegment {
                start_time: 9.0,
                end_time: 10.0,
            },
        ];
        let source_times = [5.0, 5.5, 9.0, 9.5];
        let (times, shifted) = webcam_media_timeline(&source_times, &segments, 2.0);
        assert_eq!(times, [3.0, 3.5, 7.0, 7.5]);
        assert_eq!(shifted[0].start_time, 3.0);
        assert_eq!(shifted[1].start_time, 7.0);
        // Every frame still lands in its own segment, so the decoder neither
        // seeks to the wrong position nor re-seeks on every boundary check.
        for (index, time) in times.iter().enumerate() {
            let segment = shifted
                .iter()
                .position(|s| *time >= s.start_time - 0.001 && *time <= s.end_time + 0.001)
                .unwrap();
            assert_eq!(segment, index / 2);
        }
    }
}
