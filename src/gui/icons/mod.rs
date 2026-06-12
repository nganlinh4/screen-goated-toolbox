// --- ICON RENDERER ---
// Renders curated Material Symbols Rounded glyphs. Each icon is a tiny
// white-filled SVG (see `svg/`); it is rasterized once per (icon, pixel size)
// via resvg, cached as a GPU texture, and recolored to the requested color by
// tinting the white texture. Replaces the old hand-drawn vector art.

use eframe::egui;
use std::cell::RefCell;
use std::collections::HashMap;

/// Standard icon sizes (logical px). Reference these instead of magic numbers so
/// the icon scale stays consistent and is tunable in ONE place.
/// - `ICON_XS` micro badge (e.g. search-capability marker)
/// - `ICON_SM` compact rows / icons next to ~13px text
/// - `ICON_MD` default inline icon (headers, provider badges, node types)
/// - `ICON_LG` emphasis: row-action buttons, preset-type icon, modal-title icon
/// - `ICON_XL` standalone toolbar/control button (`icon_button` default)
///
/// Dropdown chevrons instead track `ui.spacing().icon_width` (egui's combo-arrow
/// size) so they always match egui's own widgets.
pub const ICON_XS: f32 = 13.0;
pub const ICON_SM: f32 = 14.0;
pub const ICON_MD: f32 = 16.0;
pub const ICON_LG: f32 = 18.0;
pub const ICON_XL: f32 = 20.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    Settings,

    EyeOpen,
    EyeClosed,
    Microphone,
    Image,

    Text,        // 'T' icon for text presets
    Delete,      // Trash can (presets)
    DeleteLarge, // Larger trash can (history items)

    Folder,    // Open media folder
    Copy,      // Copy text
    CopySmall, // Smaller copy icon for preset buttons
    Close,     // "X" for clearing search / closing
    Plus,      // Add/create action
    Edit,      // Rename/edit action

    TextSelect,      // Text-selection preset (italic glyph)
    Keyboard,        // Typing preset (keyboard glyph)
    Speaker,         // Device audio source
    SpeakerDisabled, // Speaker with cross (disabled TTS)
    CopyDisabled,    // Copy icon with cross (disabled auto-copy)
    Lightbulb,       // Tips
    Realtime,        // Streaming waves (realtime audio)
    Rtt,             // Live Translate / real-time text
    Star,            // Outline star (non-favorite)
    StarFilled,      // Filled star (favorite)
    Sun,             // Light mode
    Moon,            // Dark mode
    Device,          // Monitor/Device (system theme)
    DragHandle,      // Drag handle for reordering
    History,         // History (clock)
    Priority,        // Model priority chain
    Pointer,         // Mouse pointer/cursor

    // Title-bar / launch / settings (icon choices aligned with the Android app)
    Album,           // Be a DJ / PromptDJ  (Android: ms_album)
    Movie,           // Download manager — video downloader  (Android: ms_movie)
    Videocam,        // Screen record
    AutoStories,     // Help assistant  (Android: ms_auto_stories)
    BarChart,        // Usage statistics  (Android: ms_bar_chart)
    Download,        // Downloaded tools  (Android: ms_download)
    SettingsVoice,   // TTS / voice settings  (Android: ms_settings_voice)
    BreakfastDining, // Translation gummy ("bánh mỳ")  (Android: ms_breakfast_dining)

    // Providers (aligned with the Android settings dialog)
    ElectricBolt, // groq        (Android: ms_electric_bolt)
    Whatshot,     // cerebras    (Android: ms_local_fire_department)
    AutoAwesome,  // google / gemini  (Android: ms_auto_awesome)
    Translate,    // google-gtx  (Android: ms_translate)
    Terminal,     // ollama      (Android: ms_terminal)
    Public,       // openrouter  (Android: ms_public)
    QrCode,       // qrserver
    SpeechToText, // parakeet / local ASR providers
    Psychology,   // local AI / ASR providers
    Rocket,       // taalas
    Search,

    // Reorder + dropdown/collapsing chevrons (keyboard-arrow family)
    ArrowUp,
    ArrowDown,
    ArrowRight,

    // Section headers / status
    Key,
    Upgrade,
    CheckCircle,
    Warning,

    // Window Controls
    Minimize,
    Maximize,
    Restore,
}

/// White-filled Material Symbols Rounded SVG for an icon. Some icons share art
/// (rendered at different sizes / with an overlay).
fn icon_svg_bytes(icon: Icon) -> &'static [u8] {
    match icon {
        Icon::Settings => include_bytes!("svg/settings.svg"),
        Icon::EyeOpen => include_bytes!("svg/eye_open.svg"),
        Icon::EyeClosed => include_bytes!("svg/eye_closed.svg"),
        Icon::Microphone => include_bytes!("svg/microphone.svg"),
        Icon::Image => include_bytes!("svg/image.svg"),
        Icon::Text => include_bytes!("svg/text.svg"),
        Icon::Delete | Icon::DeleteLarge => include_bytes!("svg/delete.svg"),
        Icon::Folder => include_bytes!("svg/folder.svg"),
        Icon::Copy | Icon::CopySmall | Icon::CopyDisabled => include_bytes!("svg/copy.svg"),
        Icon::Close => include_bytes!("svg/close.svg"),
        Icon::Plus => include_bytes!("svg/plus.svg"),
        Icon::Edit => include_bytes!("svg/edit.svg"),
        Icon::TextSelect => include_bytes!("svg/format_italic.svg"),
        Icon::Keyboard => include_bytes!("svg/keyboard.svg"),
        Icon::Speaker => include_bytes!("svg/speaker.svg"),
        Icon::SpeakerDisabled => include_bytes!("svg/speaker_disabled.svg"),
        Icon::Lightbulb => include_bytes!("svg/lightbulb.svg"),
        Icon::Realtime => include_bytes!("svg/realtime.svg"),
        Icon::Rtt => include_bytes!("svg/rtt.svg"),
        Icon::Star => include_bytes!("svg/star.svg"),
        Icon::StarFilled => include_bytes!("svg/star_filled.svg"),
        Icon::Sun => include_bytes!("svg/sun.svg"),
        Icon::Moon => include_bytes!("svg/moon.svg"),
        Icon::Device => include_bytes!("svg/device.svg"),
        Icon::DragHandle => include_bytes!("svg/drag_handle.svg"),
        Icon::History => include_bytes!("svg/history.svg"),
        Icon::Priority => include_bytes!("svg/priority.svg"),
        Icon::Pointer => include_bytes!("svg/pointer.svg"),
        Icon::Album => include_bytes!("svg/album.svg"),
        Icon::Movie => include_bytes!("svg/movie.svg"),
        Icon::Videocam => include_bytes!("svg/videocam.svg"),
        Icon::AutoStories => include_bytes!("svg/auto_stories.svg"),
        Icon::BarChart => include_bytes!("svg/bar_chart.svg"),
        Icon::Download => include_bytes!("svg/download.svg"),
        Icon::SettingsVoice => include_bytes!("svg/settings_voice.svg"),
        Icon::BreakfastDining => include_bytes!("svg/breakfast_dining.svg"),
        Icon::ElectricBolt => include_bytes!("svg/electric_bolt.svg"),
        Icon::Whatshot => include_bytes!("svg/whatshot.svg"),
        Icon::AutoAwesome => include_bytes!("svg/auto_awesome.svg"),
        Icon::Translate => include_bytes!("svg/translate.svg"),
        Icon::Terminal => include_bytes!("svg/terminal.svg"),
        Icon::Public => include_bytes!("svg/public.svg"),
        Icon::QrCode => include_bytes!("svg/qr_code.svg"),
        Icon::SpeechToText => include_bytes!("svg/speech_to_text.svg"),
        Icon::Psychology => include_bytes!("svg/psychology.svg"),
        Icon::Rocket => include_bytes!("svg/rocket.svg"),
        Icon::Search => include_bytes!("svg/search.svg"),
        Icon::ArrowUp => include_bytes!("svg/keyboard_arrow_up.svg"),
        Icon::ArrowDown => include_bytes!("svg/keyboard_arrow_down.svg"),
        Icon::ArrowRight => include_bytes!("svg/keyboard_arrow_right.svg"),
        Icon::Key => include_bytes!("svg/key.svg"),
        Icon::Upgrade => include_bytes!("svg/upgrade.svg"),
        Icon::CheckCircle => include_bytes!("svg/check_circle.svg"),
        Icon::Warning => include_bytes!("svg/warning.svg"),
        Icon::Minimize => include_bytes!("svg/remove.svg"),
        Icon::Maximize => include_bytes!("svg/maximize.svg"),
        Icon::Restore => include_bytes!("svg/restore.svg"),
    }
}

thread_local! {
    /// (svg pointer, physical px) -> cached texture. egui rendering is
    /// single-threaded per context, so a thread-local cache is sufficient.
    static ICON_TEXTURES: RefCell<HashMap<(usize, u32), egui::TextureHandle>> =
        RefCell::new(HashMap::new());
}

/// Rasterize a white SVG into a premultiplied-RGBA image at `px` square.
fn rasterize(bytes: &[u8], px: u32) -> egui::ColorImage {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
        .expect("bundled icon svg is valid");
    let size = tree.size();
    let mut pixmap = tiny_skia::Pixmap::new(px, px).expect("icon pixmap allocation");
    let scale = px as f32 / size.width().max(size.height());
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    egui::ColorImage::from_rgba_premultiplied([px as usize, px as usize], pixmap.data())
}

/// (Cached) texture id for an icon at a given physical pixel size.
fn icon_texture(ctx: &egui::Context, icon: Icon, px: u32) -> egui::TextureId {
    let bytes = icon_svg_bytes(icon);
    let key = (bytes.as_ptr() as usize, px);
    ICON_TEXTURES.with(|cache| {
        if let Some(id) = cache.borrow().get(&key).map(egui::TextureHandle::id) {
            return id;
        }
        let image = rasterize(bytes, px);
        let handle = ctx.load_texture(
            format!("icon::{:x}::{px}", key.0),
            image,
            egui::TextureOptions::LINEAR,
        );
        let id = handle.id();
        cache.borrow_mut().insert(key, handle);
        id
    })
}

/// Paint `icon`, recolored to `color`, centered in `rect`.
fn render_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: egui::Color32) {
    let ctx = painter.ctx();
    // A filled favorite star is conventionally gold, regardless of widget state.
    // The outline (non-favorite) star is a thin frame that washes out on a light
    // background, so give it a darker, more visible shade in light mode.
    let color = if icon == Icon::StarFilled {
        egui::Color32::from_rgb(255, 193, 7)
    } else if icon == Icon::Star && !ctx.global_style().visuals.dark_mode {
        egui::Color32::from_rgb(110, 110, 110)
    } else {
        color
    };
    let ppp = ctx.pixels_per_point().max(0.01);
    // Material Symbols' "filled" glyphs fill their box more heavily than the old
    // thin line-art, so draw them inside ~84% of the allocated rect to keep the
    // visual weight in line with the previous icons (and existing call sizes).
    const GLYPH_FILL: f32 = 0.84;
    let target = rect.width().min(rect.height()) * GLYPH_FILL;
    if target <= 0.5 {
        return;
    }
    // Crispness: rasterize at an EXACT whole number of physical pixels, then draw
    // the texture 1:1 at a pixel-snapped position. Drawing at a fractional size or
    // a sub-pixel offset makes egui bilinear-sample between texels -> the icon
    // looks blurry. Snapping the min corner to the physical grid fixes it.
    let px = (target * ppp).round().clamp(6.0, 512.0);
    let side = px / ppp;
    let tex = icon_texture(ctx, icon, px as u32);
    let mut min = rect.center() - egui::vec2(side, side) * 0.5;
    min.x = (min.x * ppp).round() / ppp;
    min.y = (min.y * ppp).round() / ppp;
    let icon_rect = egui::Rect::from_min_size(min, egui::vec2(side, side));
    // Tint the white glyph -> `color` (white * color = color, alpha preserved).
    painter.image(
        tex,
        icon_rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        color,
    );
    // No dedicated "copy-off" Material symbol exists — overlay a slash.
    if icon == Icon::CopyDisabled {
        let r = icon_rect.shrink(side * 0.1);
        painter.line_segment(
            [r.left_bottom(), r.right_top()],
            egui::Stroke::new((side * 0.09).max(1.5), color),
        );
    }
}

/// Main entry point: Draw a clickable icon button (default `ICON_XL` — the
/// standard for standalone toolbar/control buttons).
pub fn icon_button(ui: &mut egui::Ui, icon: Icon) -> egui::Response {
    icon_button_sized(ui, icon, ICON_XL)
}

/// Draw a clickable icon button with custom size
pub fn icon_button_sized(ui: &mut egui::Ui, icon: Icon, size_val: f32) -> egui::Response {
    let size = egui::vec2(size_val, size_val);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    // 1. Background Hover Effect
    if response.hovered() {
        ui.painter()
            .rect_filled(rect.shrink(2.0), 4.0, ui.visuals().widgets.hovered.bg_fill);
    }

    // 2. Determine Style
    let color = if response.hovered() {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };

    // 3. Paint
    render_icon(ui.painter(), rect, icon, color);

    response
}

/// Draw a static icon (for labels/headers)
pub fn draw_icon_static(ui: &mut egui::Ui, icon: Icon, size_override: Option<f32>) {
    let side = size_override.unwrap_or(ICON_MD);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let color = ui.visuals().text_color();
    render_icon(ui.painter(), rect, icon, color);
}

/// Paint an icon directly (for custom layouts where icon_button isn't suitable)
pub fn paint_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: egui::Color32) {
    render_icon(painter, rect, icon, color);
}

/// Map an AI/service provider id to its representative icon.
pub fn provider_icon(provider: &str) -> Icon {
    // Icon per provider, matching the Android settings dialog.
    match provider {
        "google" | "gemini-live" => Icon::AutoAwesome,
        "google-gtx" => Icon::Translate,
        "groq" => Icon::ElectricBolt,
        "cerebras" => Icon::Whatshot,
        "openrouter" => Icon::Public,
        "ollama" => Icon::Terminal,
        "qrserver" => Icon::QrCode,
        "parakeet" => Icon::SpeechToText,
        "qwen3" => Icon::Psychology,
        "taalas" => Icon::Rocket,
        _ => Icon::Settings,
    }
}
