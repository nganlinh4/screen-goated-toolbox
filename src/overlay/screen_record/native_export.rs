use resvg::usvg::{self, fontdb, Options, Tree};
use serde::Deserialize;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use tiny_skia::{Pixmap, Transform};

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
    // ADDED: Accept quality string from frontend ("original" | "balanced")
    #[serde(default = "default_quality")]
    pub quality: String,
    pub segment: VideoSegment,
    pub background_config: BackgroundConfig,
    pub mouse_positions: Vec<MousePosition>,
    pub video_data: Option<Vec<u8>>,
    pub audio_data: Option<Vec<u8>>,
    pub baked_path: Option<Vec<BakedCameraFrame>>,
    pub baked_cursor_path: Option<Vec<BakedCursorFrame>>,
}

fn default_quality() -> String {
    "balanced".to_string()
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BakedCameraFrame {
    pub time: f64,
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BakedCursorFrame {
    pub time: f64,
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub is_clicked: bool,
    #[serde(rename = "type")]
    pub cursor_type: String,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VideoSegment {
    pub trim_start: f64,
    pub trim_end: f64,
    pub zoom_keyframes: Vec<ZoomKeyframe>,
    pub smooth_motion_path: Option<Vec<MotionPoint>>,
    pub crop: Option<CropRect>,
    #[serde(default)]
    pub text_segments: Vec<TextSegment>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TextSegment {
    pub id: String,
    pub start_time: f64,
    pub end_time: f64,
    pub text: String,
    pub style: TextStyle,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TextStyle {
    pub font_size: f64,
    pub color: String,
    pub x: f64, // 0-100
    pub y: f64, // 0-100
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
    pub easing_type: String,
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

// --- TEXT RENDERER ---

struct TextRenderer {
    font_db: Arc<fontdb::Database>,
}

impl TextRenderer {
    fn new() -> Self {
        let mut font_db = fontdb::Database::new();
        // Load embedded Google Sans Flex
        font_db.load_font_data(crate::assets::GOOGLE_SANS_FLEX.to_vec());
        // Fallback system fonts
        font_db.load_system_fonts();

        Self {
            font_db: Arc::new(font_db),
        }
    }

    fn render_text(&self, buffer: &mut [u8], width: u32, height: u32, segment: &TextSegment) {
        let mut pixmap = Pixmap::new(width, height).unwrap();

        // Convert hex color to CSS style
        let color = &segment.style.color;

        // Calculate position (percent to pixels)
        let x = (segment.style.x / 100.0) * width as f64;
        let y = (segment.style.y / 100.0) * height as f64;
        let font_size = segment.style.font_size;

        // Create SVG wrapper for text with shadow matching HTML canvas
        let svg_data = format!(
            r#"
            <svg width="{}" height="{}" xmlns="http://www.w3.org/2000/svg">
                <defs>
                    <filter id="shadow" x="-20%" y="-20%" width="140%" height="140%">
                        <feGaussianBlur in="SourceAlpha" stdDeviation="2"/>
                        <feOffset dx="2" dy="2" result="offsetblur"/>
                        <feComponentTransfer>
                            <feFuncA type="linear" slope="0.7"/>
                        </feComponentTransfer>
                        <feMerge>
                            <feMergeNode/>
                            <feMergeNode in="SourceGraphic"/>
                        </feMerge>
                    </filter>
                </defs>
                <text x="{}" y="{}" font-family="Google Sans Flex" font-size="{}" fill="{}" text-anchor="middle" filter="url(#shadow)" font-weight="500">
                    {}
                </text>
            </svg>
            "#,
            width, height, x, y, font_size, color, segment.text
        );

        let opt = Options {
            fontdb: self.font_db.clone(),
            ..Default::default()
        };

        if let Ok(tree) = Tree::from_str(&svg_data, &opt) {
            resvg::render(&tree, Transform::identity(), &mut pixmap.as_mut());

            let src_data = pixmap.data();
            for i in (0..buffer.len()).step_by(4) {
                let src_a = src_data[i + 3] as u32;
                if src_a > 0 {
                    let src_r = src_data[i] as u32;
                    let src_g = src_data[i + 1] as u32;
                    let src_b = src_data[i + 2] as u32;

                    let dst_r = buffer[i] as u32;
                    let dst_g = buffer[i + 1] as u32;
                    let dst_b = buffer[i + 2] as u32;

                    let inv_a = 255 - src_a;

                    buffer[i] = ((src_r * 255 + dst_r * inv_a) / 255) as u8;
                    buffer[i + 1] = ((src_g * 255 + dst_g * inv_a) / 255) as u8;
                    buffer[i + 2] = ((src_b * 255 + dst_b * inv_a) / 255) as u8;
                }
            }
        }
    }
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
            hex_to_linear(0x25, 0x63, 0xEB),
            hex_to_linear(0x7C, 0x3A, 0xED),
        ),
        "gradient2" => (
            hex_to_linear(0xFB, 0x71, 0x85),
            hex_to_linear(0xFD, 0xBA, 0x74),
        ),
        "gradient3" => (
            hex_to_linear(0x10, 0xB9, 0x81),
            hex_to_linear(0x2D, 0xD4, 0xBF),
        ),
        _ => (
            hex_to_linear(0x0A, 0x0A, 0x0A),
            hex_to_linear(0x00, 0x00, 0x00),
        ),
    }
}

// --- NEW SAMPLING LOGIC using Baked Path ---

fn sample_baked_path(time: f64, baked_path: &[BakedCameraFrame]) -> (f64, f64, f64) {
    if baked_path.is_empty() {
        return (0.0, 0.0, 1.0);
    }

    let idx = baked_path.partition_point(|p| p.time < time);

    if idx == 0 {
        let p = &baked_path[0];
        return (p.x, p.y, p.zoom);
    }

    if idx >= baked_path.len() {
        let p = baked_path.last().unwrap();
        return (p.x, p.y, p.zoom);
    }

    let p1 = &baked_path[idx - 1];
    let p2 = &baked_path[idx];

    let t = (time - p1.time) / (p2.time - p1.time).max(0.0001);
    let t = t.clamp(0.0, 1.0);

    let x = p1.x + (p2.x - p1.x) * t;
    let y = p1.y + (p2.y - p1.y) * t;
    let zoom = p1.zoom + (p2.zoom - p1.zoom) * t;

    (x, y, zoom)
}

fn sample_baked_cursor(
    time: f64,
    baked_path: &[BakedCursorFrame],
) -> Option<(f64, f64, f64, bool, String)> {
    if baked_path.is_empty() {
        return None;
    }

    let idx = baked_path.partition_point(|p| p.time < time);

    if idx == 0 {
        let p = &baked_path[0];
        return Some((p.x, p.y, p.scale, p.is_clicked, p.cursor_type.clone()));
    }

    if idx >= baked_path.len() {
        let p = baked_path.last().unwrap();
        return Some((p.x, p.y, p.scale, p.is_clicked, p.cursor_type.clone()));
    }

    let p1 = &baked_path[idx - 1];
    let p2 = &baked_path[idx];

    let t = (time - p1.time) / (p2.time - p1.time).max(0.0001);
    let t = t.clamp(0.0, 1.0);

    let x = p1.x + (p2.x - p1.x) * t;
    let y = p1.y + (p2.y - p1.y) * t;
    let scale = p1.scale + (p2.scale - p1.scale) * t;

    let is_clicked = if t < 0.5 {
        p1.is_clicked
    } else {
        p2.is_clicked
    };

    let cursor_type = if t < 0.5 {
        p1.cursor_type.clone()
    } else {
        p2.cursor_type.clone()
    };

    Some((x, y, scale, is_clicked, cursor_type))
}

pub fn start_native_export(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let mut config: ExportConfig = serde_json::from_value(args).map_err(|e| e.to_string())?;

    println!("[Export] Starting GPU-accelerated export...");

    let baked_path = config.baked_path.unwrap_or_default();
    let baked_cursor = config.baked_cursor_path.unwrap_or_default();
    let text_renderer = TextRenderer::new();

    // 0. Handle Source Video/Audio
    let mut temp_video_path: Option<PathBuf> = None;
    let mut temp_audio_path: Option<PathBuf> = None;

    let source_video_path = if let Some(video_data) = config.video_data.take() {
        let path = std::env::temp_dir().join("sgt_temp_source.mp4");
        fs::write(&path, video_data).map_err(|e| format!("Failed to write temp video: {}", e))?;
        temp_video_path = Some(path.clone());
        path.to_string_lossy().to_string()
    } else {
        unsafe { VIDEO_PATH.clone() }.ok_or("No source video found")?
    };

    let source_audio_path = if let Some(audio_data) = config.audio_data.take() {
        let path = std::env::temp_dir().join("sgt_temp_source_audio.wav");
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

    // 3. Calculate dimensions
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
    // Ensure even dimensions
    let out_w = out_w - (out_w % 2);
    let out_h = out_h - (out_h % 2);

    let scale_factor = config.background_config.scale / 100.0;
    let crop_aspect = crop_w as f64 / crop_h as f64;
    let out_aspect = out_w as f64 / out_h as f64;

    let (video_w, video_h) = if crop_aspect > out_aspect {
        let w = (out_w as f64 * scale_factor) as u32;
        let h = (w as f64 / crop_aspect) as u32;
        (w & !1, h & !1)
    } else {
        let h = (out_h as f64 * scale_factor) as u32;
        let w = (h as f64 * crop_aspect) as u32;
        (w & !1, h & !1)
    };

    // 4. Initialize GPU compositor with cursor
    let mut compositor = GpuCompositor::new(out_w, out_h, crop_w, crop_h)
        .map_err(|e| format!("GPU init failed: {}", e))?;

    compositor.init_cursor_texture();

    // 5. Start FFmpeg decoder
    let crop_filter = if let Some(c) = crop {
        let crop_x = (src_w as f64 * c.x) as u32;
        let crop_y = (src_h as f64 * c.y) as u32;
        format!("crop={}:{}:{}:{}", crop_w, crop_h, crop_x, crop_y)
    } else {
        "null".to_string()
    };

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

    // Determine CRF based on Quality setting
    let crf = if config.quality == "original" {
        "17" // High quality, near lossless
    } else {
        "23" // Balanced (Standard default)
    };

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
        crf.to_string(), // Applied here
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
    ]);

    if has_audio {
        encoder_args.extend([
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            "192k".to_string(),
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
    let frame_size = (crop_w * crop_h * 4) as usize;
    let mut buffer = vec![0u8; frame_size];

    let dt = 1.0 / config.framerate as f64;
    let step = dt * config.speed;
    let mut current_time = config.trim_start;
    let end_time = config.trim_start + config.duration;
    let mut frame_count = 0u32;

    while current_time < end_time {
        if std::io::Read::read_exact(&mut decoder_stdout, &mut buffer).is_err() {
            break;
        }

        let (raw_cam_x, raw_cam_y, zoom) =
            sample_baked_path(current_time - config.trim_start, &baked_path);

        let cursor_state = sample_baked_cursor(current_time - config.trim_start, &baked_cursor);

        let cam_x = raw_cam_x - crop_x_offset;
        let cam_y = raw_cam_y - crop_y_offset;

        compositor.upload_frame(&buffer);

        let zoomed_video_w = video_w as f64 * zoom;
        let zoomed_video_h = video_h as f64 * zoom;

        let ratio_x = (cam_x / crop_w as f64).clamp(0.0, 1.0);
        let ratio_y = (cam_y / crop_h as f64).clamp(0.0, 1.0);

        let zoom_shift_x = (1.0 - zoom) * ratio_x;
        let zoom_shift_y = (1.0 - zoom) * ratio_y;

        let bg_scale = config.background_config.scale / 100.0;
        let bg_center_x = (1.0 - bg_scale) / 2.0 * zoom;
        let bg_center_y = (1.0 - bg_scale) / 2.0 * zoom;

        let offset_x = zoom_shift_x + bg_center_x;
        let offset_y = zoom_shift_y + bg_center_y;

        let (cursor_pos_x, cursor_pos_y, cursor_scale, cursor_clicked, cursor_type_id) =
            if let Some((cx, cy, c_scale, is_clicked, c_type)) = cursor_state {
                let rel_x = (cx - crop_x_offset) / crop_w as f64;
                let rel_y = (cy - crop_y_offset) / crop_h as f64;

                let type_id = match c_type.as_str() {
                    "text" => 1.0,
                    "pointer" => 2.0,
                    _ => 0.0,
                };

                let size_ratio = (out_w as f64 / crop_w as f64).min(out_h as f64 / crop_h as f64);
                let cursor_final_scale =
                    c_scale * config.background_config.cursor_scale * zoom * size_ratio;

                (
                    rel_x as f32,
                    rel_y as f32,
                    cursor_final_scale as f32,
                    if is_clicked { 1.0 } else { 0.0 },
                    type_id,
                )
            } else {
                (-1.0, -1.0, 0.0, 0.0, 0.0)
            };

        // Increase shadow opacity slightly to match Canvas visuals (heuristic)
        let shadow_opacity = (config.background_config.shadow / 100.0).min(0.5);
        let shadow_blur = (config.background_config.shadow * 1.5) as f32;

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
            shadow_blur,
            shadow_opacity as f32,
            gradient1,
            gradient2,
            (current_time - config.trim_start) as f32,
            (cursor_pos_x, cursor_pos_y),
            cursor_scale,
            cursor_clicked,
            cursor_type_id,
        );

        let mut rendered = compositor.render_frame(&uniforms);

        // --- RENDER TEXT OVERLAY ---
        // Overlay any text segments active at this time
        for text_seg in &config.segment.text_segments {
            if current_time >= text_seg.start_time && current_time <= text_seg.end_time {
                text_renderer.render_text(&mut rendered, out_w, out_h, text_seg);
            }
        }

        let _ = encoder_stdin.write_all(&rendered);

        frame_count += 1;
        current_time += step;
    }

    drop(encoder_stdin);
    let _ = decoder.wait();
    let encoder_result = encoder.wait();

    if let Some(p) = temp_video_path {
        let _ = fs::remove_file(p);
    }
    if let Some(p) = temp_audio_path {
        let _ = fs::remove_file(p);
    }

    match encoder_result {
        Ok(status) if status.success() => Ok(serde_json::json!({
            "status": "success",
            "path": output_path.to_string_lossy(),
            "frames": frame_count
        })),
        _ => Err("Encoder failed".to_string()),
    }
}
