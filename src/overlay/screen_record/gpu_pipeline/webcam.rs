use windows::Win32::Graphics::Direct3D11::*;
use windows::core::Interface;

use super::super::d3d_interop::{create_d3d11_device, create_d3d11_device_on_adapter};
use super::super::mf_decode::{DxgiDeviceManager, MfDecoder};
use super::ring_buffers::try_create_decode_input_ring;
use super::types::{DECODE_RING_SIZE, PipelineConfig, WebcamDecodeSetup};

pub(super) fn prepare_webcam_decode_setup(
    source_times: &[f64],
    config: &PipelineConfig,
    wgpu_vendor: u32,
    wgpu_device_id: u32,
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

    let (d3d_device, d3d_context) = if wgpu_vendor != 0 {
        create_d3d11_device_on_adapter(wgpu_vendor, wgpu_device_id)?
    } else {
        create_d3d11_device()?
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

    let ring = try_create_decode_input_ring(&d3d_device, wgpu_device, render_width, render_height)
        .ok_or_else(|| "Webcam zero-copy decode ring init failed".to_string())?;
    println!(
        "[Export] Zero-copy GPU webcam decode path ({}-slot ring, {}x{})",
        DECODE_RING_SIZE, render_width, render_height
    );

    Ok(Some(WebcamDecodeSetup {
        d3d_device,
        d3d_context,
        ring,
        source_times: source_times
            .iter()
            .map(|time| time - config.webcam_offset_sec)
            .collect(),
        active_mask,
        source_width,
        source_height,
        render_width,
        render_height,
    }))
}
