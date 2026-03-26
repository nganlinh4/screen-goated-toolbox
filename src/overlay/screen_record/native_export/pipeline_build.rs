// Pipeline uniform builder, GPU compositor setup, and export result handling.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::config::{self, ExportConfig, ExportRuntimeDiagnostics};
use super::cursor::{collect_used_cursor_slots, parse_baked_cursor_frames};
use super::overlay::load_custom_background_rgba;
use super::sampling::{sample_baked_path, sample_parsed_baked_cursor};
use super::{background_presets, gif, staging};

use super::gpu_export::{CompositorUniformParams, CompositorUniforms, GpuCompositor, create_uniforms};
use super::gpu_pipeline;
use super::mf_decode;

use super::EXPORT_CANCELLED;

pub(super) fn sample_capture_dimensions_at_time(
    time: f64,
    mouse_positions: &[config::MousePosition],
    fallback_width: f64,
    fallback_height: f64,
) -> (f64, f64) {
    let mut prev: Option<&config::MousePosition> = None;
    let mut next: Option<&config::MousePosition> = None;

    for position in mouse_positions {
        let has_dims = position
            .capture_width
            .zip(position.capture_height)
            .map(|(w, h)| w.is_finite() && h.is_finite() && w > 1.0 && h > 1.0)
            .unwrap_or(false);
        if !has_dims {
            continue;
        }
        if (position.timestamp - time).abs() < 0.001 {
            return (
                position.capture_width.unwrap_or(fallback_width).max(1.0),
                position.capture_height.unwrap_or(fallback_height).max(1.0),
            );
        }
        if position.timestamp <= time {
            prev = Some(position);
            continue;
        }
        next = Some(position);
        break;
    }

    match (prev, next) {
        (Some(p), Some(n)) => {
            let dt = (n.timestamp - p.timestamp).max(0.000001);
            let t = ((time - p.timestamp) / dt).clamp(0.0, 1.0);
            let pw = p.capture_width.unwrap_or(fallback_width).max(1.0);
            let ph = p.capture_height.unwrap_or(fallback_height).max(1.0);
            let nw = n.capture_width.unwrap_or(fallback_width).max(1.0);
            let nh = n.capture_height.unwrap_or(fallback_height).max(1.0);
            (pw + (nw - pw) * t, ph + (nh - ph) * t)
        }
        (Some(p), None) => (
            p.capture_width.unwrap_or(fallback_width).max(1.0),
            p.capture_height.unwrap_or(fallback_height).max(1.0),
        ),
        (None, Some(n)) => (
            n.capture_width.unwrap_or(fallback_width).max(1.0),
            n.capture_height.unwrap_or(fallback_height).max(1.0),
        ),
        (None, None) => (fallback_width.max(1.0), fallback_height.max(1.0)),
    }
}

/// Builds the per-frame uniforms closure and related GPU state.
/// Returns a tuple of (compositor, build_uniforms_closure, pipeline_config_partial, extra_state).
#[allow(clippy::too_many_arguments)]
pub(super) fn setup_gpu_and_uniforms(
    config: &ExportConfig,
    baked_path: &[config::BakedCameraFrame],
    baked_cursor: &[config::BakedCursorFrame],
    cursor_slot_overrides: &[staging::CursorSlotOverride],
    atlas_rgba: &Option<Vec<u8>>,
    atlas_w: u32,
    atlas_h: u32,
    src_w: u32,
    src_h: u32,
    crop_w: u32,
    crop_h: u32,
    crop_x_offset: f64,
    crop_y_offset: f64,
    out_w: u32,
    out_h: u32,
) -> Result<GpuSetupResult, String> {
    let out_aspect = out_w as f64 / out_h as f64;
    let scale_factor = config.background_config.scale / 100.0;

    // Initialize GPU compositor
    let gpu_init_start = Instant::now();
    let mut compositor = GpuCompositor::new(out_w, out_h, crop_w, crop_h, out_w, out_h)
        .map_err(|e| format!("GPU init failed: {}", e))?;
    let gpu_device_secs = gpu_init_start.elapsed().as_secs_f64();
    eprintln!(
        "[Export][Timing] GPU compositor init: {:.3}s",
        gpu_device_secs
    );

    let used_cursor_slots = collect_used_cursor_slots(baked_cursor);
    let cursor_init_start = Instant::now();
    compositor.init_cursor_texture_fast(&used_cursor_slots);
    for override_tile in cursor_slot_overrides {
        compositor.upload_cursor_slot_rgba(override_tile.slot_id, override_tile.rgba.as_slice());
    }
    let cursor_init_secs = cursor_init_start.elapsed().as_secs_f64();
    eprintln!(
        "[Export][Timing] Cursor texture init + overrides: {:.3}s",
        cursor_init_secs
    );

    // Upload sprite atlas
    if let Some(rgba) = atlas_rgba {
        compositor.upload_atlas(rgba, atlas_w, atlas_h);
    }

    let mut use_custom_background = false;
    let mut actual_bg_w = out_w as f32;
    let mut actual_bg_h = out_h as f32;
    if config.background_config.background_type == "custom" {
        if let Some(custom_background) = &config.background_config.custom_background {
            let bg_load_start = Instant::now();
            match load_custom_background_rgba(custom_background) {
                Ok((rgba_arc, tw, th)) => {
                    let bg_load_secs = bg_load_start.elapsed().as_secs_f64();
                    let bg_upload_start = Instant::now();
                    compositor.upload_background(rgba_arc.as_slice(), tw, th);
                    let bg_upload_secs = bg_upload_start.elapsed().as_secs_f64();
                    eprintln!(
                        "[CustomBg] export load {:.3}s + gpu upload {:.3}s ({}x{}, rgba={}B)",
                        bg_load_secs, bg_upload_secs, tw, th, rgba_arc.len()
                    );
                    use_custom_background = true;
                    actual_bg_w = tw as f32;
                    actual_bg_h = th as f32;
                }
                Err(e) => return Err(format!("Custom background load failed: {}", e)),
            }
        } else {
            return Err("Custom background is selected but missing path".to_string());
        }
    }

    let bg = &config.background_config.background_type;
    let background_preset = background_presets::get_builtin_background(bg)
        .copied()
        .unwrap_or_default();
    let bg_style = background_preset.family_code;

    let shadow_opacity = if config.background_config.shadow > 0.0 {
        0.5_f32
    } else {
        0.0
    };
    let shadow_blur = config.background_config.shadow as f32;
    let border_radius = config.background_config.border_radius as f32;
    let shadow_offset = config.background_config.shadow as f32 * 0.5;
    let cursor_scale_cfg = config.background_config.cursor_scale;
    let cursor_shadow = (config.background_config.cursor_shadow as f32 / 100.0).clamp(0.0, 2.0);
    println!(
        "[Export] Cursor shadow setting: {}% (normalized {:.3})",
        config.background_config.cursor_shadow, cursor_shadow
    );

    let parsed_baked_cursor = parse_baked_cursor_frames(baked_cursor);

    Ok(GpuSetupResult {
        compositor,
        gpu_device_secs,
        cursor_init_secs,
        uniform_params: UniformBuildParams {
            baked_path: baked_path.to_vec(),
            parsed_baked_cursor,
            src_w,
            src_h,
            crop_w,
            crop_h,
            crop_x_offset,
            crop_y_offset,
            out_w,
            out_h,
            out_aspect,
            scale_factor,
            cursor_scale_cfg,
            cursor_shadow,
            use_custom_background,
            actual_bg_w,
            actual_bg_h,
            shadow_opacity,
            shadow_blur,
            shadow_offset,
            border_radius,
            background_preset,
            bg_style,
        },
    })
}

pub(super) struct GpuSetupResult {
    pub compositor: GpuCompositor,
    pub gpu_device_secs: f64,
    pub cursor_init_secs: f64,
    pub uniform_params: UniformBuildParams,
}

pub(super) struct UniformBuildParams {
    pub baked_path: Vec<config::BakedCameraFrame>,
    pub parsed_baked_cursor: Vec<config::ParsedBakedCursorFrame>,
    pub src_w: u32,
    pub src_h: u32,
    pub crop_w: u32,
    pub crop_h: u32,
    pub crop_x_offset: f64,
    pub crop_y_offset: f64,
    pub out_w: u32,
    pub out_h: u32,
    pub out_aspect: f64,
    pub scale_factor: f64,
    pub cursor_scale_cfg: f64,
    pub cursor_shadow: f32,
    pub use_custom_background: bool,
    pub actual_bg_w: f32,
    pub actual_bg_h: f32,
    pub shadow_opacity: f32,
    pub shadow_blur: f32,
    pub shadow_offset: f32,
    pub border_radius: f32,
    pub background_preset: background_presets::BuiltInBackgroundPreset,
    pub bg_style: f32,
}

impl UniformBuildParams {
    pub fn build_uniforms(
        &self,
        config: &ExportConfig,
        base_time: f64,
        cam_pan_time: f64,
        cam_zoom_time: f64,
        cursor_time: f64,
    ) -> CompositorUniforms {
        let crop = &config.segment.crop;
        let (cam_x_raw, cam_y_raw, _) = sample_baked_path(cam_pan_time, &self.baked_path);
        let (_, _, zoom) = sample_baked_path(cam_zoom_time, &self.baked_path);
        let cursor_sample =
            sample_parsed_baked_cursor(cursor_time, &self.parsed_baked_cursor);
        let (capture_w, capture_h) = sample_capture_dimensions_at_time(
            base_time,
            &config.mouse_positions,
            self.src_w as f64,
            self.src_h as f64,
        );
        let logical_crop_w = if let Some(c) = crop {
            (capture_w * c.width).max(1.0)
        } else {
            capture_w.max(1.0)
        };
        let logical_crop_h = if let Some(c) = crop {
            (capture_h * c.height).max(1.0)
        } else {
            capture_h.max(1.0)
        };

        let ow = self.out_w as f64;
        let oh = self.out_h as f64;
        let cw = self.crop_w as f64;
        let ch = self.crop_h as f64;
        let ow32 = self.out_w as f32;
        let oh32 = self.out_h as f32;

        let dynamic_crop_aspect = logical_crop_w / logical_crop_h.max(1.0);
        let (video_w, video_h) = if dynamic_crop_aspect > self.out_aspect {
            let w = ow * self.scale_factor;
            let h = w / dynamic_crop_aspect;
            (w.max(1.0), h.max(1.0))
        } else {
            let h = oh * self.scale_factor;
            let w = h * dynamic_crop_aspect;
            (w.max(1.0), h.max(1.0))
        };
        let size_ratio = (ow / logical_crop_w.max(1.0)).min(oh / logical_crop_h.max(1.0));

        let cam_x = cam_x_raw - self.crop_x_offset;
        let cam_y = cam_y_raw - self.crop_y_offset;
        let zvw = video_w * zoom;
        let zvh = video_h * zoom;
        let rx = (cam_x / cw).clamp(0.0, 1.0);
        let ry = (cam_y / ch).clamp(0.0, 1.0);
        let zsx = (1.0 - zoom) * rx;
        let zsy = (1.0 - zoom) * ry;
        let bcx = (1.0 - video_w / ow) / 2.0 * zoom;
        let bcy = (1.0 - video_h / oh) / 2.0 * zoom;
        let ox = zsx + bcx;
        let oy = zsy + bcy;

        let (cp_x, cp_y, cs, co, ct, cr) =
            if let Some((cx, cy, c_s, c_t, c_o, c_r)) = cursor_sample {
                if c_o < 0.001 {
                    (-100.0_f32, -100.0, 0.0, 0.0, 0.0, 0.0)
                } else {
                    let rel_x = (cx - self.crop_x_offset) / cw;
                    let rel_y = (cy - self.crop_y_offset) / ch;
                    let fs = c_s * self.cursor_scale_cfg * zoom * size_ratio;
                    (
                        rel_x as f32,
                        rel_y as f32,
                        fs as f32,
                        c_o as f32,
                        c_t,
                        c_r as f32,
                    )
                }
            } else {
                (-1.0, -1.0, 0.0, 0.0, 0.0, 0.0)
            };

        create_uniforms(CompositorUniformParams {
            video_offset: (ox as f32, oy as f32),
            video_scale: (zvw as f32 / ow32, zvh as f32 / oh32),
            output_size: (ow32, oh32),
            video_size: (zvw as f32, zvh as f32),
            border_radius: self.border_radius,
            shadow_offset: self.shadow_offset,
            shadow_blur: self.shadow_blur,
            shadow_opacity: self.shadow_opacity,
            gradient_color1: self.background_preset.gradient_color1,
            gradient_color2: self.background_preset.gradient_color2,
            gradient_color3: self.background_preset.gradient_color3,
            gradient_color4: self.background_preset.gradient_color4,
            gradient_color5: self.background_preset.gradient_color5,
            bg_params1: self.background_preset.bg_params1,
            bg_params2: self.background_preset.bg_params2,
            bg_params3: self.background_preset.bg_params3,
            bg_params4: self.background_preset.bg_params4,
            bg_params5: self.background_preset.bg_params5,
            bg_params6: self.background_preset.bg_params6,
            time: base_time as f32,
            render_mode: 0.0,
            cursor_pos: (cp_x, cp_y),
            cursor_scale: cs,
            cursor_opacity: co,
            cursor_type_id: ct,
            cursor_rotation: cr,
            cursor_shadow: self.cursor_shadow,
            use_background_texture: self.use_custom_background,
            bg_zoom: zoom as f32,
            bg_anchor: (rx as f32, ry as f32),
            background_style: self.bg_style,
            bg_tex_w: self.actual_bg_w,
            bg_tex_h: self.actual_bg_h,
        })
    }
}

#[expect(clippy::too_many_arguments, reason = "export result handling needs all output context")]
pub(super) fn handle_export_result(
    result: Result<gpu_pipeline::ZeroCopyExportResult, String>,
    config: &ExportConfig,
    encode_output_path: &std::path::Path,
    final_output_path: &std::path::Path,
    mixed_audio_cleanup_path: &Option<PathBuf>,
    is_gif: bool,
    out_w: u32,
    out_h: u32,
    bitrate: u32,
    parse_secs: f64,
    gpu_device_secs: f64,
    cursor_init_secs: f64,
    export_total_start: Instant,
) -> Result<serde_json::Value, String> {
    let _ = mf_decode::mf_shutdown();

    match result {
        Ok(r) => {
            if EXPORT_CANCELLED.load(Ordering::SeqCst) {
                let _ = fs::remove_file(encode_output_path);
                if let Some(path) = mixed_audio_cleanup_path {
                    let _ = fs::remove_file(path);
                }
                println!("[Export][Summary] status=cancelled (partial encode finalized)");
                return Ok(serde_json::json!({ "status": "cancelled" }));
            }

            if is_gif {
                println!("[Export] Converting to GIF via FFmpeg...");
                let gif_max_w = out_w.min(960);
                match gif::convert_mp4_to_gif(encode_output_path, final_output_path, gif_max_w) {
                    Ok(()) => {
                        let _ = fs::remove_file(encode_output_path);
                        println!("[Export] GIF conversion complete");
                    }
                    Err(e) => {
                        let _ = fs::remove_file(encode_output_path);
                        let _ = fs::remove_file(final_output_path);
                        if let Some(path) = mixed_audio_cleanup_path {
                            let _ = fs::remove_file(path);
                        }
                        println!("[Export][Summary] status=error (gif convert) error={}", e);
                        return Err(e);
                    }
                }
            }

            let total_secs = export_total_start.elapsed().as_secs_f64();
            let output_bytes = fs::metadata(final_output_path)
                .map(|m| m.len())
                .unwrap_or(0);
            let output_duration_sec =
                (r.frames_encoded as f64 / config.framerate as f64).max(0.001);
            let actual_total_bitrate_kbps =
                (output_bytes as f64 * 8.0 / output_duration_sec / 1000.0).max(0.0);

            let diagnostics = ExportRuntimeDiagnostics {
                backend: "zero_copy_gpu".to_string(),
                encoder: "mf_h264".to_string(),
                codec: "h264".to_string(),
                turbo: false,
                sfe: false,
                pre_render_policy: config.pre_render_policy.clone(),
                quality_gate_percent: config.quality_gate_percent,
                actual_total_bitrate_kbps,
                expected_total_bitrate_kbps: bitrate as f64,
                bitrate_deviation_percent: if bitrate > 0 {
                    ((actual_total_bitrate_kbps - bitrate as f64).abs() / bitrate as f64) * 100.0
                } else {
                    0.0
                },
            };

            println!(
                "[Export][Summary] status=success out={}x{} fps={} dur={:.3}s frames={} parse={:.3}s gpu={:.3}s cursor={:.3}s total={:.3}s pipeline=zero_copy actual_kbps={:.1}",
                out_w, out_h, config.framerate, config.duration, r.frames_encoded,
                parse_secs, gpu_device_secs, cursor_init_secs, total_secs, actual_total_bitrate_kbps
            );
            if let Some(path) = mixed_audio_cleanup_path {
                let _ = fs::remove_file(path);
            }

            Ok(serde_json::json!({
                "status": "success",
                "path": final_output_path.to_string_lossy(),
                "frames": r.frames_encoded,
                "bytes": output_bytes,
                "pipeline": "zero_copy_gpu",
                "elapsedSecs": r.elapsed_secs,
                "fps": r.fps,
                "diagnostics": diagnostics
            }))
        }
        Err(e) => {
            let _ = fs::remove_file(encode_output_path);
            let _ = fs::remove_file(final_output_path);
            if let Some(path) = mixed_audio_cleanup_path {
                let _ = fs::remove_file(path);
            }

            if EXPORT_CANCELLED.load(Ordering::SeqCst) {
                println!("[Export][Summary] status=cancelled");
                return Ok(serde_json::json!({ "status": "cancelled" }));
            }

            println!("[Export][Summary] status=error error={}", e);
            Err(e)
        }
    }
}
