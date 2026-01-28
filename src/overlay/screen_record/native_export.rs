use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use super::gpu_export::{create_uniforms, GpuCompositor};
use crate::overlay::screen_record::engine::VIDEO_PATH;

// --- Structs for JSON Deserialization ---

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExportConfig {
    pub width: u32,
    pub height: u32,
    pub framerate: u32,
    pub audio_path: String,
    pub trim_start: f64,
    pub duration: f64,
    pub speed: f64,
    pub segment: VideoSegment,
    pub background_config: BackgroundConfig,
    pub mouse_positions: Vec<MousePosition>,
    pub video_data: Option<Vec<u8>>,
    pub audio_data: Option<Vec<u8>>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VideoSegment {
    pub trim_start: f64,
    pub trim_end: f64,
    pub zoom_keyframes: Vec<ZoomKeyframe>,
    pub smooth_motion_path: Option<Vec<MotionPoint>>,
    pub crop: Option<CropRect>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct CropRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ZoomKeyframe {
    pub time: f64,
    pub zoom_factor: f64,
    pub position_x: f64,
    pub position_y: f64,
    pub easing_type: String, // 'linear', 'easeOut', 'easeInOut'
}

#[derive(Deserialize, Debug, Clone)]
pub struct MotionPoint {
    pub time: f64,
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundConfig {
    pub scale: f64,
    pub border_radius: f64,
    pub background_type: String,
    pub custom_background: Option<String>,
    pub shadow: f64,
    pub cursor_scale: f64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MousePosition {
    pub x: i32,
    pub y: i32,
    pub timestamp: f64,
    #[serde(rename = "isClicked")]
    pub is_clicked: bool,
    pub cursor_type: String,
}

// --- GRADIENT COLORS ---

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn hex_to_linear(r: u8, g: u8, b: u8) -> [f32; 4] {
    [
        srgb_to_linear(r as f32 / 255.0),
        srgb_to_linear(g as f32 / 255.0),
        srgb_to_linear(b as f32 / 255.0),
        1.0,
    ]
}

fn get_gradient_colors(bg_type: &str) -> ([f32; 4], [f32; 4]) {
    match bg_type {
        "gradient1" => (
            hex_to_linear(0x25, 0x63, 0xEB), // #2563EB Blue
            hex_to_linear(0x7C, 0x3A, 0xED), // #7C3AED Violet
        ),
        "gradient2" => (
            hex_to_linear(0xFB, 0x71, 0x85), // #FB7185 Rose
            hex_to_linear(0xFD, 0xBA, 0x74), // #FDBA74 Orange
        ),
        "gradient3" => (
            hex_to_linear(0x10, 0xB9, 0x81), // #10B981 Emerald
            hex_to_linear(0x2D, 0xD4, 0xBF), // #2DD4BF Teal
        ),
        _ => (
            hex_to_linear(0x0A, 0x0A, 0x0A), // #0A0A0A Dark
            hex_to_linear(0x00, 0x00, 0x00), // #000000 Black
        ),
    }
}

// --- EASING FUNCTIONS ---
fn ease_out_cubic(x: f64) -> f64 {
    1.0 - (1.0 - x).powi(3)
}

// --- INTERPOLATION ---

fn interpolate_zoom(
    time: f64,
    motion_path: &Option<Vec<MotionPoint>>,
    manual_keyframes: &[ZoomKeyframe],
    crop_x: f64,
    crop_y: f64,
    crop_w: f64,
    crop_h: f64,
) -> (f64, f64, f64) {
    // 1. Auto Zoom Path (Highest Priority)
    if let Some(path) = motion_path {
        if !path.is_empty() {
            let idx = path
                .iter()
                .position(|p| p.time >= time)
                .unwrap_or(path.len().saturating_sub(1));

            if idx == 0 {
                let p = &path[0];
                return (p.x, p.y, p.zoom);
            }

            let p2 = &path[idx.min(path.len() - 1)];
            let p1 = &path[(idx - 1).max(0)];
            let t = ((time - p1.time) / (p2.time - p1.time).max(0.001)).clamp(0.0, 1.0);

            let x = p1.x + (p2.x - p1.x) * t;
            let y = p1.y + (p2.y - p1.y) * t;
            let zoom = p1.zoom + (p2.zoom - p1.zoom) * t;
            return (x, y, zoom);
        }
    }

    // 2. Manual Keyframes
    if manual_keyframes.is_empty() {
        // Center of Crop
        let cx = crop_x + crop_w / 2.0;
        let cy = crop_y + crop_h / 2.0;
        return (cx, cy, 1.0);
    }

    // Find next and previous keyframes
    // Assumes keyframes are NOT sorted in JSON, so we sort them first?
    // Usually they are sorted by time, but for safety in Rust let's do a linear scan.

    // Sort keyframes by time
    let mut sorted: Vec<&ZoomKeyframe> = manual_keyframes.iter().collect();
    sorted.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap());

    // Find bounding keyframes
    let next_idx = sorted.iter().position(|k| k.time > time);

    // Helper to convert normalized 0-1 pos (relative to crop) to global pixels
    let to_global = |pos_x: f64, pos_y: f64| -> (f64, f64) {
        (crop_x + (pos_x * crop_w), crop_y + (pos_y * crop_h))
    };

    if let Some(idx) = next_idx {
        let next_kf = sorted[idx];
        let prev_kf = if idx > 0 { sorted[idx - 1] } else { sorted[0] };

        // Transition logic matches TypeScript: check if within transition duration window
        const TRANSITION_DURATION: f64 = 1.0;

        // Case A: Transition between two defined keyframes
        if idx > 0 && (next_kf.time - prev_kf.time) <= TRANSITION_DURATION {
            let progress = (time - prev_kf.time) / (next_kf.time - prev_kf.time);
            let eased = ease_out_cubic(progress.clamp(0.0, 1.0));

            let (p1x, p1y) = to_global(prev_kf.position_x, prev_kf.position_y);
            let (p2x, p2y) = to_global(next_kf.position_x, next_kf.position_y);

            let z = prev_kf.zoom_factor + (next_kf.zoom_factor - prev_kf.zoom_factor) * eased;
            let x = p1x + (p2x - p1x) * eased;
            let y = p1y + (p2y - p1y) * eased;
            return (x, y, z);
        }

        // Case B: Transition approaching next keyframe (from hold)
        let time_to_next = next_kf.time - time;
        if time_to_next <= TRANSITION_DURATION {
            let progress = (TRANSITION_DURATION - time_to_next) / TRANSITION_DURATION;
            let eased = ease_out_cubic(progress.clamp(0.0, 1.0));

            // Start state is either previous keyframe OR default centered
            let (start_z, start_px, start_py) = if idx > 0 {
                (prev_kf.zoom_factor, prev_kf.position_x, prev_kf.position_y)
            } else {
                (1.0, 0.5, 0.5)
            };

            let (p1x, p1y) = to_global(start_px, start_py);
            let (p2x, p2y) = to_global(next_kf.position_x, next_kf.position_y);

            let z = start_z + (next_kf.zoom_factor - start_z) * eased;
            let x = p1x + (p2x - p1x) * eased;
            let y = p1y + (p2y - p1y) * eased;
            return (x, y, z);
        }

        // Case C: Holding previous keyframe value
        if idx > 0 {
            let (cx, cy) = to_global(prev_kf.position_x, prev_kf.position_y);
            return (cx, cy, prev_kf.zoom_factor);
        }

        // Before first keyframe (default state)
        let (cx, cy) = to_global(0.5, 0.5);
        return (cx, cy, 1.0);
    }

    // After last keyframe
    let last = sorted.last().unwrap();
    let (cx, cy) = to_global(last.position_x, last.position_y);
    (cx, cy, last.zoom_factor)
}

// --- MAIN EXPORT FUNCTION ---

pub fn start_native_export(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let mut config: ExportConfig = serde_json::from_value(args).map_err(|e| e.to_string())?;

    println!("[Export] Starting GPU-accelerated export...");
    println!(
        "[Export] Config dimensions from frontend: {}x{}",
        config.width, config.height
    );
    println!("[Export] Segment crop: {:?}", config.segment.crop);

    if let Some(path) = &config.segment.smooth_motion_path {
        println!("[Export] Motion Path points: {}", path.len());
        if !path.is_empty() {
            println!("[Export] First motion point: {:?}", path[0]);
        }
    } else {
        println!("[Export] No motion path provided (Manual Mode)");
    }

    println!(
        "[Export] Manual Keyframes: {}",
        config.segment.zoom_keyframes.len()
    );

    // 0. Handle Source Video/Audio
    let mut temp_video_path: Option<PathBuf> = None;
    let mut temp_audio_path: Option<PathBuf> = None;

    let source_video_path = if let Some(video_data) = config.video_data.take() {
        let path = std::env::temp_dir().join("sgt_temp_source.mp4");
        println!("[Export] Writing {} bytes to temp video", video_data.len());
        fs::write(&path, video_data).map_err(|e| format!("Failed to write temp video: {}", e))?;
        temp_video_path = Some(path.clone());
        path.to_string_lossy().to_string()
    } else {
        unsafe { VIDEO_PATH.clone() }.ok_or("No source video found")?
    };

    let source_audio_path = if let Some(audio_data) = config.audio_data.take() {
        let path = std::env::temp_dir().join("sgt_temp_source_audio.wav");
        println!("[Export] Writing {} bytes to temp audio", audio_data.len());
        fs::write(&path, audio_data).map_err(|e| format!("Failed to write temp audio: {}", e))?;
        temp_audio_path = Some(path.clone());
        Some(path.to_string_lossy().to_string())
    } else if !config.audio_path.is_empty() {
        Some(config.audio_path.clone())
    } else {
        None
    };

    let output_path = dirs::download_dir()
        .unwrap_or(PathBuf::from("."))
        .join(format!(
            "SGT_Export_{}.mp4",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));

    // 1. Setup FFmpeg
    let ffmpeg_path = super::get_ffmpeg_path();
    let ffprobe_path = super::get_ffprobe_path();

    if !ffmpeg_path.exists() {
        return Err("FFmpeg not found.".to_string());
    }

    // 2. Probe source dimensions
    let probe = Command::new(&ffprobe_path)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=s=x:p=0",
            &source_video_path,
        ])
        .output()
        .map_err(|e| format!("Probe failed: {}", e))?;

    let dim_str = String::from_utf8_lossy(&probe.stdout);
    let dims: Vec<&str> = dim_str.trim().split('x').collect();
    let src_w: u32 = dims.get(0).and_then(|s| s.parse().ok()).unwrap_or(1920);
    let src_h: u32 = dims.get(1).and_then(|s| s.parse().ok()).unwrap_or(1080);

    println!("[Export] Source: {}x{}", src_w, src_h);

    // 3. Calculate dimensions - USE CROP DIMENSIONS FOR ASPECT RATIO
    // Apply crop to get the actual visible portion dimensions
    let crop = &config.segment.crop;
    let crop_w = if let Some(c) = crop {
        (src_w as f64 * c.width) as u32
    } else {
        src_w
    };
    let crop_h = if let Some(c) = crop {
        (src_h as f64 * c.height) as u32
    } else {
        src_h
    };

    // Calculate absolute offset of the crop in source coordinates
    let crop_x_offset = if let Some(c) = crop {
        src_w as f64 * c.x
    } else {
        0.0
    };
    let crop_y_offset = if let Some(c) = crop {
        src_h as f64 * c.y
    } else {
        0.0
    };

    println!(
        "[Export] Crop: {}x{} at ({},{}), Global Size: {}x{}",
        crop_w, crop_h, crop_x_offset, crop_y_offset, src_w, src_h
    );

    // If config dimensions are 0, use cropped dimensions (matching preview behavior)
    let out_w = if config.width == 0 {
        crop_w
    } else {
        config.width
    };
    let out_h = if config.height == 0 {
        crop_h
    } else {
        config.height
    };
    let out_w = out_w - (out_w % 2);
    let out_h = out_h - (out_h % 2);

    // Calculate video size maintaining CROPPED aspect ratio (not source)
    let scale_factor = config.background_config.scale / 100.0;
    let crop_aspect = crop_w as f64 / crop_h as f64;
    let out_aspect = out_w as f64 / out_h as f64;

    // Fit video within output while maintaining cropped aspect ratio
    let (video_w, video_h) = if crop_aspect > out_aspect {
        // Cropped source is wider - fit to width
        let w = (out_w as f64 * scale_factor) as u32;
        let h = (w as f64 / crop_aspect) as u32;
        (w & !1, h & !1)
    } else {
        // Cropped source is taller - fit to height
        let h = (out_h as f64 * scale_factor) as u32;
        let w = (h as f64 * crop_aspect) as u32;
        (w & !1, h & !1)
    };

    let pad_x = (out_w - video_w) / 2;
    let pad_y = (out_h - video_h) / 2;

    println!(
        "[Export] Output: {}x{}, Video: {}x{}, Offset: {},{}",
        out_w, out_h, video_w, video_h, pad_x, pad_y
    );

    // 4. Initialize GPU compositor - use CROPPED dimensions as video input size
    println!("[Export] Initializing GPU...");
    let compositor = GpuCompositor::new(out_w, out_h, crop_w, crop_h)
        .map_err(|e| format!("GPU init failed: {}", e))?;

    // 5. Start FFmpeg decoder - apply crop filter to extract the cropped region
    // Build crop filter if we have a crop rect
    let crop_filter = if let Some(c) = crop {
        let crop_x = (src_w as f64 * c.x) as u32;
        let crop_y = (src_h as f64 * c.y) as u32;
        format!("crop={}:{}:{}:{}", crop_w, crop_h, crop_x, crop_y)
    } else {
        "null".to_string()
    };

    println!("[Export] Using crop filter: {}", crop_filter);

    let mut decoder = Command::new(&ffmpeg_path)
        .args([
            "-ss",
            &config.trim_start.to_string(),
            "-t",
            &config.duration.to_string(),
            "-i",
            &source_video_path,
            "-vf",
            &crop_filter,
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{}x{}", crop_w, crop_h),
            "-",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Decoder failed: {}", e))?;

    let mut decoder_stdout = decoder
        .stdout
        .take()
        .ok_or("Failed to open decoder stdout")?;

    // 6. Start FFmpeg encoder
    let has_audio = source_audio_path.is_some();

    let mut encoder_args = vec![
        "-y".to_string(),
        "-f".to_string(),
        "rawvideo".to_string(),
        "-pix_fmt".to_string(),
        "rgba".to_string(),
        "-s".to_string(),
        format!("{}x{}", out_w, out_h),
        "-r".to_string(),
        config.framerate.to_string(),
        "-i".to_string(),
        "-".to_string(),
    ];

    if let Some(audio) = &source_audio_path {
        let audio_filter = if config.speed != 1.0 {
            format!("atempo={}", config.speed.clamp(0.5, 2.0))
        } else {
            "anull".to_string()
        };
        encoder_args.extend([
            "-ss".to_string(),
            config.trim_start.to_string(),
            "-t".to_string(),
            config.duration.to_string(),
            "-i".to_string(),
            audio.clone(),
            "-af".to_string(),
            audio_filter,
        ]);
    }

    encoder_args.extend([
        "-c:v".to_string(),
        "libx264".to_string(),
        "-preset".to_string(),
        "fast".to_string(),
        "-crf".to_string(),
        "18".to_string(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
    ]);

    if has_audio {
        encoder_args.extend([
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            "192k".to_string(),
            "-shortest".to_string(),
        ]);
    }

    encoder_args.extend([
        "-movflags".to_string(),
        "+faststart".to_string(),
        output_path.to_str().unwrap().to_string(),
    ]);

    let mut encoder = Command::new(&ffmpeg_path)
        .args(&encoder_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Encoder failed: {}", e))?;

    let mut encoder_stdin = encoder.stdin.take().ok_or("Failed to open encoder stdin")?;

    // 7. Process frames
    let (gradient1, gradient2) = get_gradient_colors(&config.background_config.background_type);

    // Use CROPPED dimensions for frame buffer size (decoder outputs cropped frames)
    let frame_size = (crop_w * crop_h * 4) as usize;
    let mut buffer = vec![0u8; frame_size];

    let dt = 1.0 / config.framerate as f64;
    let step = dt * config.speed;
    let mut current_time = config.trim_start;
    let end_time = config.trim_start + config.duration;
    let mut frame_count = 0u32;
    let total_frames = (config.duration * config.framerate as f64 / config.speed) as u32;

    let start = std::time::Instant::now();
    println!("[Export] Processing frames with GPU...");

    while current_time < end_time {
        // Read frame from decoder (cropped frame)
        if std::io::Read::read_exact(&mut decoder_stdout, &mut buffer).is_err() {
            println!("[Export] Decoder finished at frame {}", frame_count);
            break;
        }

        // Get zoom/pan for this frame using either Auto-Zoom or Manual Keyframes
        let (raw_cam_x, raw_cam_y, zoom) = interpolate_zoom(
            current_time - config.trim_start, // Time relative to segment start
            &config.segment.smooth_motion_path,
            &config.segment.zoom_keyframes,
            crop_x_offset,
            crop_y_offset,
            crop_w as f64,
            crop_h as f64,
        );

        // Adjust camera coordinate to be relative to the CROPPED frame
        // If the original camera was at (1000, 500) and we cropped the region starting at (500, 0),
        // the new camera center is at (1000-500, 500-0) = (500, 500) inside the crop.
        let cam_x = raw_cam_x - crop_x_offset;
        let _cam_y = raw_cam_y - crop_y_offset;

        // Upload frame to GPU
        compositor.upload_frame(&buffer);

        // Calculate video position based on zoom
        let zoomed_video_w = video_w as f64 * zoom;
        let zoomed_video_h = video_h as f64 * zoom;

        // Backend Calculation for offset_x (Normalized top-left of video content)
        // Formula matches frontend logic:
        // FinalX = zoomOffsetX + (x * zoomFactor)
        // zoomOffsetX = (CanvW - CanvW * Zoom) * PositionRatio
        // x (centered background) = (CanvW - CanvW * BgScale) / 2
        //
        // Normalized Offset = [(1 - Zoom) * Ratio] + [(1 - BgScale) / 2 * Zoom]

        let ratio_x = (cam_x / crop_w as f64).clamp(0.0, 1.0);
        let ratio_y = (_cam_y / crop_h as f64).clamp(0.0, 1.0);

        // Part 1: Zoom Translation (anchor shift)
        let zoom_shift_x = (1.0 - zoom) * ratio_x;
        let zoom_shift_y = (1.0 - zoom) * ratio_y;

        // Part 2: Background Scale centering (scaled by zoom because it's inside the zoom transform)
        let bg_scale = config.background_config.scale / 100.0;
        let bg_center_x = (1.0 - bg_scale) / 2.0 * zoom;
        let bg_center_y = (1.0 - bg_scale) / 2.0 * zoom; // Ignoring cropBottom for now to match X logic

        let offset_x = zoom_shift_x + bg_center_x;
        let offset_y = zoom_shift_y + bg_center_y;

        if frame_count == total_frames / 2 {
            println!("[Export] DEBUG MID FRAME:");
            println!("  raw_cam: ({}, {}), zoom: {}", raw_cam_x, raw_cam_y, zoom);
            println!("  local_cam: ({}, {}), crop_w: {}", cam_x, _cam_y, crop_w);
            println!("  ratio: ({}, {})", ratio_x, ratio_y);
            println!("  zoom_shift: ({}, {})", zoom_shift_x, zoom_shift_y);
            println!("  bg_center: ({}, {})", bg_center_x, bg_center_y);
            println!("  FINAL OFFSET: ({}, {})", offset_x, offset_y);
        }

        // Create uniforms
        let uniforms = create_uniforms(
            (offset_x as f32, offset_y as f32),
            (
                zoomed_video_w as f32 / out_w as f32,
                zoomed_video_h as f32 / out_h as f32,
            ),
            (out_w as f32, out_h as f32),
            (zoomed_video_w as f32, zoomed_video_h as f32),
            config.background_config.border_radius as f32,
            config.background_config.shadow as f32 / 4.0,
            config.background_config.shadow as f32 / 2.0,
            (config.background_config.shadow / 100.0).min(0.5) as f32,
            gradient1,
            gradient2,
            (current_time - config.trim_start) as f32,
        );

        // Render on GPU
        let rendered = compositor.render_frame(&uniforms);

        // Write to encoder
        encoder_stdin
            .write_all(&rendered)
            .map_err(|e| e.to_string())?;

        frame_count += 1;
        current_time += step;

        if frame_count % 60 == 0 {
            let elapsed = start.elapsed().as_secs_f64();
            let fps = frame_count as f64 / elapsed;
            println!("[Export] Frame {}, {:.1} fps", frame_count, fps);
        }
    }

    // Cleanup
    drop(encoder_stdin);
    let _ = decoder.wait();
    let encoder_result = encoder.wait();

    if let Some(p) = temp_video_path {
        let _ = fs::remove_file(p);
    }
    if let Some(p) = temp_audio_path {
        let _ = fs::remove_file(p);
    }

    let elapsed = start.elapsed().as_secs_f64();
    println!(
        "[Export] Completed {} frames in {:.1}s ({:.1} fps)",
        frame_count,
        elapsed,
        frame_count as f64 / elapsed
    );

    match encoder_result {
        Ok(status) if status.success() => {
            println!("[Export] Success! Output: {:?}", output_path);
            Ok(serde_json::json!({
                "status": "success",
                "path": output_path.to_string_lossy(),
                "frames": frame_count,
                "duration_seconds": elapsed,
                "fps": frame_count as f64 / elapsed
            }))
        }
        _ => Err("Encoder failed".to_string()),
    }
}
