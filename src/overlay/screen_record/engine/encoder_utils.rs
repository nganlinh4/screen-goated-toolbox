use wc_windows::core::Interface as WcInterface;
use windows::core::Interface as AppInterface;
use windows_capture::{
    encoder::{
        AudioSettingsBuilder, ContainerSettingsBuilder, VideoEncoder, VideoSettingsBuilder,
        VideoSettingsSubType,
    },
    windows_bindings as wc_windows,
};

use super::types::{
    DEFAULT_TARGET_FPS, ENCODER_MAX_PENDING_FRAMES, MF_HW_ACCEL_AUTO_PIXELS_PER_SEC_THRESHOLD,
    WINDOW_CAPTURE_MAX_PENDING_FRAMES, WINDOW_CAPTURE_QUEUE_TARGET_MS,
    WINDOW_CAPTURE_VRAM_POOL_MAX_FRAMES, WINDOW_CAPTURE_VRAM_POOL_MIN_FRAMES,
};

pub(crate) fn clone_wc_interface_to_app<TFrom, TTo>(src: &TFrom) -> Result<TTo, String>
where
    TFrom: WcInterface,
    TTo: AppInterface,
{
    let raw = src.as_raw();
    let borrowed = unsafe { <TTo as AppInterface>::from_raw_borrowed(&raw) }
        .ok_or_else(|| "null COM pointer".to_string())?;
    Ok(borrowed.clone())
}

pub(crate) fn clone_app_interface_to_wc<TFrom, TTo>(src: &TFrom) -> Result<TTo, String>
where
    TFrom: AppInterface,
    TTo: WcInterface,
{
    let raw = src.as_raw();
    let borrowed = unsafe { <TTo as WcInterface>::from_raw_borrowed(&raw) }
        .ok_or_else(|| "null COM pointer".to_string())?;
    Ok(borrowed.clone())
}

pub(crate) fn select_target_fps(monitor_hz: u32) -> u32 {
    // Prefer exact monitor divisors in the 50-60fps export band.
    // Example: 165Hz -> 55fps (exact), which removes recurring pacing drift.
    for candidate in (50..=60).rev() {
        if monitor_hz.is_multiple_of(candidate) {
            return candidate;
        }
    }

    DEFAULT_TARGET_FPS
}

pub(crate) fn compute_window_max_pending_frames(target_fps: u32) -> usize {
    let target_fps = target_fps.max(1) as usize;
    let buffered_frames = (target_fps * WINDOW_CAPTURE_QUEUE_TARGET_MS).div_ceil(1000);
    buffered_frames.clamp(
        ENCODER_MAX_PENDING_FRAMES,
        WINDOW_CAPTURE_MAX_PENDING_FRAMES,
    )
}

pub(crate) fn compute_window_vram_pool_frames(max_pending_frames: usize) -> usize {
    max_pending_frames.div_ceil(2).clamp(
        WINDOW_CAPTURE_VRAM_POOL_MIN_FRAMES,
        WINDOW_CAPTURE_VRAM_POOL_MAX_FRAMES,
    )
}

pub(crate) fn should_ignore_window_frame(frame_w: u32, frame_h: u32) -> bool {
    use super::types::MIN_VALID_WINDOW_FRAME_DIM;
    frame_w < MIN_VALID_WINDOW_FRAME_DIM || frame_h < MIN_VALID_WINDOW_FRAME_DIM
}

pub(crate) fn mf_hw_accel_override() -> Option<bool> {
    match std::env::var("SCREEN_RECORD_MF_HW_ACCEL") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            Some(matches!(normalized.as_str(), "1" | "true" | "yes" | "on"))
        }
        Err(_) => None,
    }
}

pub(crate) fn should_prefer_mf_hw_accel(
    target_type: &str,
    target_fps: u32,
    width: u32,
    height: u32,
) -> bool {
    if let Some(explicit) = mf_hw_accel_override() {
        return explicit;
    }

    let pixels_per_sec = (width as u64)
        .saturating_mul(height as u64)
        .saturating_mul(target_fps.max(1) as u64);

    target_type == "window" || pixels_per_sec >= MF_HW_ACCEL_AUTO_PIXELS_PER_SEC_THRESHOLD
}

pub(crate) struct ScopedMfHwAccelEnv {
    previous: Option<std::ffi::OsString>,
}

impl ScopedMfHwAccelEnv {
    pub(crate) fn set(enabled: bool) -> Self {
        let key = "SCREEN_RECORD_MF_HW_ACCEL";
        let previous = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, if enabled { "1" } else { "0" });
        }
        Self { previous }
    }
}

impl Drop for ScopedMfHwAccelEnv {
    fn drop(&mut self) {
        let key = "SCREEN_RECORD_MF_HW_ACCEL";
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

pub(crate) struct MfEncoderCreateConfig<'a> {
    pub(crate) enc_w: u32,
    pub(crate) enc_h: u32,
    pub(crate) target_fps: u32,
    pub(crate) final_bitrate: u32,
    pub(crate) sample_rate: u32,
    pub(crate) channels: u32,
    pub(crate) video_path: &'a std::path::Path,
    pub(crate) prefer_hw: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EncoderCanvas {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(crate) fn exact_encoder_canvas(width: u32, height: u32) -> EncoderCanvas {
    EncoderCanvas {
        width: width.max(128) & !1,
        height: height.max(128) & !1,
    }
}

pub(crate) fn aligned_encoder_canvas(width: u32, height: u32) -> EncoderCanvas {
    EncoderCanvas {
        width: (width.max(128) + 15) & !15,
        height: (height.max(128) + 15) & !15,
    }
}

pub(crate) fn create_video_encoder_with_mf_mode(
    config: MfEncoderCreateConfig<'_>,
) -> Result<VideoEncoder, Box<dyn std::error::Error + Send + Sync>> {
    let _env_scope = ScopedMfHwAccelEnv::set(config.prefer_hw);
    let encoder = VideoEncoder::new(
        VideoSettingsBuilder::new(config.enc_w, config.enc_h)
            .sub_type(VideoSettingsSubType::H264)
            .bitrate(config.final_bitrate)
            .frame_rate(config.target_fps),
        AudioSettingsBuilder::new()
            .sample_rate(config.sample_rate)
            .channel_count(config.channels)
            .bitrate(192_000)
            .disabled(false),
        ContainerSettingsBuilder::new(),
        config.video_path,
    )?;
    Ok(encoder)
}

pub(crate) fn create_video_encoder_with_canvas_fallback(
    config: MfEncoderCreateConfig<'_>,
    capture_width: u32,
    capture_height: u32,
) -> Result<(VideoEncoder, EncoderCanvas), Box<dyn std::error::Error + Send + Sync>> {
    let exact_canvas = exact_encoder_canvas(capture_width, capture_height);
    let aligned_canvas = aligned_encoder_canvas(capture_width, capture_height);
    let canvases = if exact_canvas == aligned_canvas {
        [Some(exact_canvas), None]
    } else {
        [Some(exact_canvas), Some(aligned_canvas)]
    };
    let mut last_error = None;

    for canvas in canvases.into_iter().flatten() {
        match create_video_encoder_with_mf_mode(MfEncoderCreateConfig {
            enc_w: canvas.width,
            enc_h: canvas.height,
            target_fps: config.target_fps,
            final_bitrate: config.final_bitrate,
            sample_rate: config.sample_rate,
            channels: config.channels,
            video_path: config.video_path,
            prefer_hw: config.prefer_hw,
        }) {
            Ok(encoder) => {
                if canvas != exact_canvas {
                    eprintln!(
                        "[CaptureBackend] Exact encoder canvas {}x{} rejected; using 16-aligned fallback {}x{}",
                        exact_canvas.width, exact_canvas.height, canvas.width, canvas.height
                    );
                }
                return Ok((encoder, canvas));
            }
            Err(error) => {
                if canvas == exact_canvas && aligned_canvas != exact_canvas {
                    eprintln!(
                        "[CaptureBackend] Exact encoder canvas {}x{} init failed; retrying 16-aligned {}x{}: {}",
                        canvas.width,
                        canvas.height,
                        aligned_canvas.width,
                        aligned_canvas.height,
                        error
                    );
                }
                last_error = Some(error);
            }
        }
    }

    Err(last_error.expect("encoder canvas attempts must include at least one candidate"))
}
