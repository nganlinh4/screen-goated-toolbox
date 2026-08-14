use super::config::{
    AudioDownloadConfig, BackgroundConfig, BakedCameraFrame, BakedCursorFrame,
    CompositionExportConfig, DeviceAudioPoint, ExportConfig, ImportedAudioSegmentConfig,
    MousePosition, VideoSegment,
};

const MAX_DIMENSION: u32 = 16_384;
const MAX_PIXELS: u64 = 67_108_864;
const MAX_FRAMERATE: u32 = 240;
const MAX_FRAME_COUNT: f64 = 2_000_000.0;
const MAX_CLIPS: usize = 512;
const MAX_TIMELINE_ITEMS: usize = 2_000_000;
const MAX_PATH_BYTES: usize = 32 * 1024;
const MAX_SHORT_TEXT_BYTES: usize = 4 * 1024;
const MAX_CUSTOM_BACKGROUND_SOURCE_BYTES: usize = 90 * 1024 * 1024;

pub(crate) fn validate_identifier(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 128 {
        return Err(format!("{label} must contain 1 to 128 characters"));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(format!(
            "{label} may contain only ASCII letters, digits, '-' and '_'"
        ));
    }
    Ok(())
}

fn validate_path(value: &str, label: &str, required: bool) -> Result<(), String> {
    if required && value.trim().is_empty() {
        return Err(format!("{label} is required"));
    }
    if value.len() > MAX_PATH_BYTES || value.contains('\0') {
        return Err(format!("{label} is too long or contains a null byte"));
    }
    Ok(())
}

fn validate_short_text(value: &str, label: &str, required: bool) -> Result<(), String> {
    if required && value.trim().is_empty() {
        return Err(format!("{label} is required"));
    }
    if value.len() > MAX_SHORT_TEXT_BYTES || value.contains('\0') {
        return Err(format!("{label} is too long or contains a null byte"));
    }
    Ok(())
}

fn validate_dimensions(width: u32, height: u32, label: &str) -> Result<(), String> {
    if width == 0 || height == 0 || width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(format!(
            "{label} dimensions must be between 1 and {MAX_DIMENSION} pixels"
        ));
    }
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(format!("{label} exceeds the {MAX_PIXELS}-pixel limit"));
    }
    Ok(())
}

fn validate_optional_dimensions(width: u32, height: u32, label: &str) -> Result<(), String> {
    if width == 0 && height == 0 {
        return Ok(());
    }
    validate_dimensions(width, height, label)
}

fn finite(value: f64, label: &str) -> Result<(), String> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(format!("{label} must be finite"))
    }
}

fn validate_duration(duration: f64, fps: u32, label: &str) -> Result<(), String> {
    finite(duration, label)?;
    if duration <= 0.0 || duration * f64::from(fps) > MAX_FRAME_COUNT {
        return Err(format!(
            "{label} must be positive and contain at most {MAX_FRAME_COUNT:.0} output frames"
        ));
    }
    Ok(())
}

fn validate_background(config: &BackgroundConfig) -> Result<(), String> {
    for (label, value) in [
        ("background scale", config.scale),
        ("background border radius", config.border_radius),
        ("legacy bottom crop", config.crop_bottom),
        ("background shadow", config.shadow),
        ("cursor scale", config.cursor_scale),
        ("cursor shadow", config.cursor_shadow),
        ("cursor motion blur", config.motion_blur_cursor),
        ("zoom motion blur", config.motion_blur_zoom),
        ("pan motion blur", config.motion_blur_pan),
        ("volume", config.volume),
    ] {
        finite(value, label)?;
    }
    if !(0.0..100.0).contains(&config.crop_bottom) {
        return Err(
            "legacy bottom crop must be between 0 (inclusive) and 100 (exclusive)".to_string(),
        );
    }
    for (label, value) in [
        ("cursor movement delay", config.cursor_movement_delay),
        ("cursor smoothness", config.cursor_smoothness),
        ("cursor wiggle strength", config.cursor_wiggle_strength),
        ("cursor wiggle damping", config.cursor_wiggle_damping),
        ("cursor wiggle response", config.cursor_wiggle_response),
        ("cursor tilt angle", config.cursor_tilt_angle),
    ] {
        if let Some(value) = value {
            finite(value, label)?;
        }
    }
    for (label, value) in [
        ("cursor pack", config.cursor_pack.as_deref()),
        (
            "default cursor variant",
            config.cursor_default_variant.as_deref(),
        ),
        ("text cursor variant", config.cursor_text_variant.as_deref()),
        (
            "pointer cursor variant",
            config.cursor_pointer_variant.as_deref(),
        ),
        (
            "open-hand cursor variant",
            config.cursor_open_hand_variant.as_deref(),
        ),
    ] {
        if let Some(value) = value {
            validate_short_text(value, label, false)?;
        }
    }
    if config.background_type == "custom" {
        let source = config
            .custom_background
            .as_deref()
            .ok_or_else(|| "custom background source is required".to_string())?;
        if source.is_empty()
            || source.contains('\0')
            || source.len() > MAX_CUSTOM_BACKGROUND_SOURCE_BYTES
        {
            return Err("custom background source is invalid or too large".to_string());
        }
        if !source.starts_with("data:image/") && !source.contains("/bg-downloaded/") {
            return Err("unsupported custom background source".to_string());
        }
    } else if super::background_presets::get_builtin_background(&config.background_type).is_none() {
        return Err("unknown built-in background type".to_string());
    } else if let Some(source) = &config.custom_background
        && source.len() > MAX_CUSTOM_BACKGROUND_SOURCE_BYTES
    {
        return Err("unused custom background source is too large".to_string());
    }
    Ok(())
}

fn validate_volume_points(points: &[DeviceAudioPoint], label: &str) -> Result<(), String> {
    if points.len() > MAX_TIMELINE_ITEMS {
        return Err(format!("{label} contains too many points"));
    }
    for point in points {
        finite(point.time, label)?;
        finite(point.volume, label)?;
        if point.time < 0.0 || point.volume < 0.0 {
            return Err(format!("{label} contains an invalid point"));
        }
    }
    Ok(())
}

fn validate_audio_segments(
    segments: &[ImportedAudioSegmentConfig],
    label: &str,
) -> Result<(), String> {
    if segments.len() > MAX_CLIPS {
        return Err(format!("{label} exceeds the {MAX_CLIPS}-segment limit"));
    }
    for segment in segments {
        validate_path(&segment.raw_audio_path, label, true)?;
        for (field, value) in [
            ("audio duration", segment.duration),
            ("audio start time", segment.start_time),
            ("audio in point", segment.in_point),
            ("audio out point", segment.out_point),
            ("audio playback rate", segment.playback_rate),
        ] {
            finite(value, field)?;
        }
        if segment.duration < 0.0
            || segment.start_time < 0.0
            || segment.in_point < 0.0
            || segment.out_point < segment.in_point
            || segment.playback_rate <= 0.0
        {
            return Err(format!("{label} contains an invalid timeline range"));
        }
        if segment.volume_points.len() > MAX_TIMELINE_ITEMS {
            return Err(format!("{label} contains too many volume points"));
        }
        validate_volume_points(&segment.volume_points, label)?;
    }
    Ok(())
}

fn validate_mouse_positions(points: &[MousePosition], label: &str) -> Result<(), String> {
    if points.len() > MAX_TIMELINE_ITEMS {
        return Err(format!("{label} contains too many points"));
    }
    for point in points {
        for (field, value) in [
            ("mouse x", Some(point.x)),
            ("mouse y", Some(point.y)),
            ("mouse timestamp", Some(point.timestamp)),
            ("mouse rotation", point.cursor_rotation),
            ("capture width", point.capture_width),
            ("capture height", point.capture_height),
        ] {
            if let Some(value) = value {
                finite(value, field)?;
            }
        }
        if point.timestamp < 0.0
            || point.capture_width.is_some_and(|value| value <= 0.0)
            || point.capture_height.is_some_and(|value| value <= 0.0)
        {
            return Err(format!("{label} contains an invalid point"));
        }
        if let Some(cursor_type) = &point.cursor_type {
            validate_short_text(cursor_type, "cursor type", false)?;
        }
    }
    Ok(())
}

fn validate_baked_frames(
    camera: Option<&[BakedCameraFrame]>,
    cursor: Option<&[BakedCursorFrame]>,
) -> Result<(), String> {
    if camera.is_some_and(|frames| frames.len() > MAX_TIMELINE_ITEMS)
        || cursor.is_some_and(|frames| frames.len() > MAX_TIMELINE_ITEMS)
    {
        return Err("export contains too many baked frames".to_string());
    }
    for frame in camera.into_iter().flatten() {
        for (label, value) in [
            ("baked camera time", frame.time),
            ("baked camera x", frame.x),
            ("baked camera y", frame.y),
            ("baked camera zoom", frame.zoom),
        ] {
            finite(value, label)?;
        }
        if frame.time < 0.0 || frame.zoom <= 0.0 {
            return Err("baked camera frame contains an invalid value".to_string());
        }
    }
    for frame in cursor.into_iter().flatten() {
        for (label, value) in [
            ("baked cursor time", frame.time),
            ("baked cursor x", frame.x),
            ("baked cursor y", frame.y),
            ("baked cursor scale", frame.scale),
            ("baked cursor opacity", frame.opacity),
            ("baked cursor rotation", frame.rotation),
        ] {
            finite(value, label)?;
        }
        if frame.time < 0.0 || frame.scale < 0.0 || !(0.0..=1.0).contains(&frame.opacity) {
            return Err("baked cursor frame contains an invalid value".to_string());
        }
        validate_short_text(&frame.cursor_type, "baked cursor type", true)?;
    }
    Ok(())
}

fn validate_segment(segment: &VideoSegment) -> Result<(), String> {
    if let Some(crop) = &segment.crop {
        for (label, value) in [
            ("crop x", crop.x),
            ("crop y", crop.y),
            ("crop width", crop.width),
            ("crop height", crop.height),
        ] {
            finite(value, label)?;
        }
        if crop.x < 0.0
            || crop.y < 0.0
            || crop.width <= 0.0
            || crop.height <= 0.0
            || crop.x + crop.width > 1.0 + f64::EPSILON
            || crop.y + crop.height > 1.0 + f64::EPSILON
        {
            return Err("crop must be a non-empty normalized rectangle".to_string());
        }
    }
    let counts = [
        segment.trim_segments.len(),
        segment._text_segments.len(),
        segment.zoom_blocks.len(),
        segment.zoom_influence_points.len(),
        segment.speed_points.len(),
        segment.device_audio_points.len(),
        segment.mic_audio_points.len(),
        segment.smooth_motion_path.len(),
        segment.keystroke_events.len(),
        segment
            .cursor_visibility_segments
            .as_ref()
            .map_or(0, Vec::len),
    ];
    if counts.into_iter().any(|count| count > MAX_TIMELINE_ITEMS) {
        return Err("video segment contains too many timeline items".to_string());
    }
    for (label, value) in [
        ("device audio offset", segment.device_audio_offset_sec),
        ("microphone audio offset", segment.mic_audio_offset_sec),
        ("webcam offset", segment.webcam_offset_sec),
        ("keystroke delay", segment.keystroke_delay_sec),
    ] {
        finite(value, label)?;
    }
    for range in &segment.trim_segments {
        if !range.start_time.is_finite()
            || !range.end_time.is_finite()
            || range.start_time < 0.0
            || range.end_time < range.start_time
        {
            return Err("trim segment contains an invalid time range".to_string());
        }
    }
    for point in &segment.speed_points {
        if !point.time.is_finite() || !point.speed.is_finite() || point.speed <= 0.0 {
            return Err("speed point contains an invalid value".to_string());
        }
    }
    for block in &segment.zoom_blocks {
        for (label, value) in [
            ("zoom start", block.start_time),
            ("zoom end", block.end_time),
            ("zoom ease in", block.ease_in),
            ("zoom ease out", block.ease_out),
            ("zoom factor", block.zoom_factor),
            ("zoom x", block.position_x),
            ("zoom y", block.position_y),
        ] {
            finite(value, label)?;
        }
        if block.start_time < 0.0 || block.end_time < block.start_time || block.zoom_factor <= 0.0 {
            return Err("zoom block contains an invalid value".to_string());
        }
    }
    for point in &segment.zoom_influence_points {
        finite(point.time, "zoom influence time")?;
        finite(point.value, "zoom influence value")?;
        if point.time < 0.0 {
            return Err("zoom influence point has a negative time".to_string());
        }
    }
    for point in &segment.smooth_motion_path {
        for (label, value) in [
            ("smooth camera time", point.time),
            ("smooth camera x", point.x),
            ("smooth camera y", point.y),
            ("smooth camera zoom", point.zoom),
        ] {
            finite(value, label)?;
        }
        if point.time < 0.0 || point.zoom <= 0.0 {
            return Err("smooth camera point contains an invalid value".to_string());
        }
    }
    validate_volume_points(&segment.device_audio_points, "device audio envelope")?;
    validate_volume_points(&segment.mic_audio_points, "microphone audio envelope")?;
    for event in &segment.keystroke_events {
        finite(event.start_time, "keystroke start")?;
        finite(event.end_time, "keystroke end")?;
        if event.start_time < 0.0 || event.end_time < event.start_time {
            return Err("keystroke event contains an invalid time range".to_string());
        }
        validate_short_text(&event.event_type, "keystroke event type", true)?;
    }
    for range in segment.cursor_visibility_segments.iter().flatten() {
        finite(range.start_time, "cursor visibility start")?;
        finite(range.end_time, "cursor visibility end")?;
        if range.start_time < 0.0 || range.end_time < range.start_time {
            return Err("cursor visibility segment contains an invalid range".to_string());
        }
    }
    Ok(())
}

fn validate_common(
    dimensions: (u32, u32),
    fps: u32,
    bitrate: u32,
    quality_gate: f64,
    policy: &str,
    format: &str,
    output_dir: &str,
) -> Result<(), String> {
    validate_dimensions(dimensions.0, dimensions.1, "export")?;
    if !(1..=MAX_FRAMERATE).contains(&fps) {
        return Err(format!("framerate must be between 1 and {MAX_FRAMERATE}"));
    }
    if bitrate > 1_000_000 {
        return Err("target bitrate exceeds the supported limit".to_string());
    }
    finite(quality_gate, "quality gate")?;
    if !(0.0..=100.0).contains(&quality_gate) {
        return Err("quality gate must be between 0 and 100".to_string());
    }
    if !matches!(policy, "off" | "idle_only" | "aggressive") {
        return Err("unsupported pre-render policy".to_string());
    }
    if !matches!(format, "mp4" | "gif" | "both") {
        return Err("unsupported export format".to_string());
    }
    validate_path(output_dir, "output directory", false)
}

impl ExportConfig {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_common(
            (self.width, self.height),
            self.framerate,
            self.target_video_bitrate_kbps,
            self.quality_gate_percent,
            &self.pre_render_policy,
            &self.format,
            &self.output_dir,
        )?;
        validate_duration(self.duration, self.framerate, "export duration")?;
        finite(self.trim_start, "trim start")?;
        if self.trim_start < 0.0 {
            return Err("trim start must not be negative".to_string());
        }
        validate_optional_dimensions(self.source_width, self.source_height, "source video")?;
        validate_path(&self.source_video_path, "source video", true)?;
        for (path, label) in [
            (&self.device_audio_path, "device audio"),
            (&self.mic_audio_path, "microphone audio"),
            (&self.webcam_video_path, "webcam video"),
        ] {
            validate_path(path, label, false)?;
        }
        validate_segment(&self.segment)?;
        validate_background(&self.background_config)?;
        validate_mouse_positions(&self.mouse_positions, "mouse path")?;
        validate_baked_frames(
            self.baked_path.as_deref(),
            self.baked_cursor_path.as_deref(),
        )?;
        validate_audio_segments(&self.audio_segments, "audio segments")?;
        validate_audio_segments(&self.narration_segments, "narration segments")?;
        validate_volume_points(&self.audio_track_volume_points, "audio track envelope")?;
        validate_volume_points(
            &self.narration_track_volume_points,
            "narration track envelope",
        )?;
        Ok(())
    }
}

impl CompositionExportConfig {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_identifier(&self.session_id, "session id")?;
        validate_common(
            (self.width, self.height),
            self.framerate,
            self.target_video_bitrate_kbps,
            self.quality_gate_percent,
            &self.pre_render_policy,
            &self.format,
            &self.output_dir,
        )?;
        if self.clips.is_empty() || self.clips.len() > MAX_CLIPS {
            return Err(format!("composition must contain 1 to {MAX_CLIPS} clips"));
        }
        for clip in &self.clips {
            validate_identifier(&clip.job_id, "job id")?;
            validate_identifier(&clip.clip_id, "clip id")?;
            validate_short_text(&clip.clip_name, "clip name", true)?;
            validate_path(&clip.source_video_path, "source video", true)?;
            for (path, label) in [
                (&clip.device_audio_path, "device audio"),
                (&clip.mic_audio_path, "microphone audio"),
                (&clip.webcam_video_path, "webcam video"),
            ] {
                validate_path(path, label, false)?;
            }
            validate_optional_dimensions(clip.source_width, clip.source_height, "source video")?;
            validate_duration(clip.duration, self.framerate, "clip duration")?;
            finite(clip.trim_start, "clip trim start")?;
            if clip.trim_start < 0.0 {
                return Err("clip trim start must not be negative".to_string());
            }
            validate_segment(&clip.segment)?;
            validate_background(&clip.background_config)?;
            validate_mouse_positions(&clip.mouse_positions, "clip mouse path")?;
        }
        validate_audio_segments(&self.audio_segments, "audio segments")?;
        validate_audio_segments(&self.narration_segments, "narration segments")?;
        validate_volume_points(&self.audio_track_volume_points, "audio track envelope")?;
        validate_volume_points(
            &self.narration_track_volume_points,
            "narration track envelope",
        )?;
        Ok(())
    }
}

impl AudioDownloadConfig {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_path(&self.output_dir, "output directory", false)?;
        validate_short_text(&self.track_label, "track label", false)?;
        if self.clips.is_empty() || self.clips.len() > MAX_CLIPS {
            return Err(format!("audio export must contain 1 to {MAX_CLIPS} clips"));
        }
        for clip in &self.clips {
            validate_identifier(&clip.clip_id, "clip id")?;
            validate_short_text(&clip.clip_name, "clip name", true)?;
            for (path, label) in [
                (&clip.source_video_path, "source video"),
                (&clip.device_audio_path, "device audio"),
                (&clip.mic_audio_path, "microphone audio"),
            ] {
                validate_path(path, label, false)?;
            }
            validate_duration(clip.duration, 60, "clip duration")?;
            finite(clip.trim_start, "clip trim start")?;
            if clip.trim_start < 0.0 {
                return Err("clip trim start must not be negative".to_string());
            }
            validate_segment(&clip.segment)?;
        }
        validate_audio_segments(&self.audio_segments, "audio segments")?;
        validate_audio_segments(&self.narration_segments, "narration segments")?;
        validate_volume_points(&self.audio_track_volume_points, "audio track envelope")?;
        validate_volume_points(
            &self.narration_track_volume_points,
            "narration track envelope",
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_background, validate_identifier};
    use crate::overlay::screen_record::native_export::config::BackgroundConfig;

    #[test]
    fn identifiers_cannot_escape_owned_directories() {
        for unsafe_id in ["", ".", "..", "../outside", r"..\outside", "C:temp"] {
            assert!(validate_identifier(unsafe_id, "id").is_err());
        }
        assert!(validate_identifier("session_123-abc", "id").is_ok());
    }

    #[test]
    fn legacy_bottom_crop_keeps_a_nonempty_source_rect() {
        let background = |crop_bottom| -> BackgroundConfig {
            serde_json::from_value(serde_json::json!({
                "scale": 100.0,
                "borderRadius": 0.0,
                "cropBottom": crop_bottom,
                "backgroundType": "solid",
                "shadow": 0.0,
                "cursorScale": 1.0
            }))
            .expect("background config parses")
        };
        assert!(validate_background(&background(0.0)).is_ok());
        assert!(validate_background(&background(99.9)).is_ok());
        assert!(validate_background(&background(-0.1)).is_err());
        assert!(validate_background(&background(100.0)).is_err());
    }
}
