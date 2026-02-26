use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExportConfig {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub source_width: u32,
    #[serde(default)]
    pub source_height: u32,
    #[serde(default)]
    pub source_video_path: String,
    pub framerate: u32,
    #[serde(default)]
    pub target_video_bitrate_kbps: u32,
    #[serde(default = "default_quality_gate_percent")]
    pub quality_gate_percent: f64,
    #[serde(default = "default_pre_render_policy")]
    pub pre_render_policy: String,
    pub audio_path: String,
    #[serde(default)]
    pub output_dir: String,
    pub trim_start: f64,
    pub duration: f64,
    pub segment: VideoSegment,
    pub background_config: BackgroundConfig,
    pub baked_path: Option<Vec<BakedCameraFrame>>,
    pub baked_cursor_path: Option<Vec<BakedCursorFrame>>,
    /// Raw mouse positions sent from frontend; Rust generates baked cursor path from these.
    #[serde(default)]
    pub mouse_positions: Vec<MousePosition>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ZoomKeyframe {
    pub time: f64,
    #[serde(default)]
    pub duration: f64,
    pub zoom_factor: f64,
    #[serde(default = "default_half")]
    pub position_x: f64,
    #[serde(default = "default_half")]
    pub position_y: f64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ZoomInfluencePoint {
    pub time: f64,
    pub value: f64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SpeedPoint {
    pub time: f64,
    pub speed: f64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SmoothCameraPoint {
    pub time: f64,
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

/// Raw mouse position from the recorder.
/// Note: cursor_type and cursor_rotation use snake_case in the JS wire format.
#[derive(Deserialize, Debug, Clone)]
pub struct MousePosition {
    pub x: f64,
    pub y: f64,
    pub timestamp: f64,
    #[serde(rename = "isClicked", default)]
    pub is_clicked: bool,
    #[serde(rename = "cursor_type", default)]
    pub cursor_type: Option<String>,
    #[serde(rename = "cursor_rotation", default)]
    pub cursor_rotation: Option<f64>,
}

/// A cursor visibility segment — time range where the cursor is visible.
/// None means feature off (always visible). Some([]) means always hidden.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CursorVisibilitySegment {
    pub start_time: f64,
    pub end_time: f64,
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
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default)]
    pub rotation: f64,
}

#[derive(Debug, Clone)]
pub struct ParsedBakedCursorFrame {
    pub time: f64,
    pub x: f64,
    pub y: f64,
    pub scale: f64,
    pub type_id: f32,
    pub opacity: f64,
    pub rotation: f64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct VideoSegment {
    pub crop: Option<CropRect>,
    #[serde(default, rename = "trimSegments")]
    pub trim_segments: Vec<TrimSegment>,
    #[serde(default, rename = "textSegments")]
    pub _text_segments: Vec<TextSegment>,
    #[serde(default)]
    pub zoom_keyframes: Vec<ZoomKeyframe>,
    #[serde(default)]
    pub zoom_influence_points: Vec<ZoomInfluencePoint>,
    #[serde(default)]
    pub speed_points: Vec<SpeedPoint>,
    #[serde(default)]
    pub smooth_motion_path: Vec<SmoothCameraPoint>,
    /// None = cursor-hiding feature off (always visible).
    /// Some([]) = feature on, no segments (always hidden).
    pub cursor_visibility_segments: Option<Vec<CursorVisibilitySegment>>,
    #[serde(default = "default_true")]
    pub use_custom_cursor: bool,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrimSegment {
    pub start_time: f64,
    pub end_time: f64,
}

// TextSegment: only needed for serde compat (flatten receives unknown fields).
#[derive(Deserialize, Debug, Clone)]
pub struct TextSegment {
    #[serde(flatten)]
    _rest: serde_json::Value,
}

#[derive(Deserialize, Debug, Clone)]
pub struct OverlayQuad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub u: f32,
    pub v: f32,
    pub uw: f32,
    pub vh: f32,
    pub alpha: f32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct OverlayFrame {
    pub quads: Vec<OverlayQuad>,
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
pub struct BackgroundConfig {
    pub scale: f64,
    pub border_radius: f64,
    pub background_type: String,
    #[serde(default)]
    pub custom_background: Option<String>,
    pub shadow: f64,
    pub cursor_scale: f64,
    #[serde(default)]
    pub cursor_shadow: f64,
    #[serde(default)]
    pub motion_blur_cursor: f64,
    #[serde(default)]
    pub motion_blur_zoom: f64,
    #[serde(default)]
    pub motion_blur_pan: f64,
    // Cursor physics / appearance fields used by the Rust path generator
    #[serde(default)]
    pub cursor_pack: Option<String>,
    #[serde(default)]
    pub cursor_default_variant: Option<String>,
    #[serde(default)]
    pub cursor_text_variant: Option<String>,
    #[serde(default)]
    pub cursor_pointer_variant: Option<String>,
    #[serde(default)]
    pub cursor_open_hand_variant: Option<String>,
    #[serde(default)]
    pub cursor_movement_delay: Option<f64>,
    #[serde(default)]
    pub cursor_smoothness: Option<f64>,
    #[serde(default)]
    pub cursor_wiggle_strength: Option<f64>,
    #[serde(default)]
    pub cursor_wiggle_damping: Option<f64>,
    #[serde(default)]
    pub cursor_wiggle_response: Option<f64>,
    #[serde(default)]
    pub cursor_tilt_angle: Option<f64>,
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExportRuntimeDiagnostics {
    pub backend: String,
    pub encoder: String,
    pub codec: String,
    pub turbo: bool,
    pub sfe: bool,
    pub pre_render_policy: String,
    pub quality_gate_percent: f64,
    pub actual_total_bitrate_kbps: f64,
    pub expected_total_bitrate_kbps: f64,
    pub bitrate_deviation_percent: f64,
}

fn default_opacity() -> f64 {
    1.0
}

fn default_half() -> f64 {
    0.5
}

fn default_true() -> bool {
    true
}

fn default_quality_gate_percent() -> f64 {
    3.0
}

fn default_pre_render_policy() -> String {
    "idle_only".to_string()
}

pub fn compute_default_video_bitrate_kbps(width: u32, height: u32, fps: u32) -> u32 {
    let bits_per_pixel = 0.09_f64;
    let kbps = (width as f64 * height as f64 * fps.max(1) as f64 * bits_per_pixel) / 1000.0;
    kbps.round().clamp(600.0, 80000.0) as u32
}
