// --- ICON RENDERER ---
// Renders curated Material Symbols Rounded glyphs from deterministic build-time
// alpha atlases. Runtime code only decodes PNG pages, caches GPU textures, and
// tints the white masks; the signed host contains no SVG parser or rasterizer.

use eframe::egui;
use image::ImageDecoder;
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Cursor;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Icon {
    Settings,

    EyeOpen,
    EyeClosed,
    Microphone,
    Image,

    Text,        // 'T' icon for text presets
    Delete,      // Trash can (presets)
    DeleteLarge, // Larger trash can (history items)

    Folder,        // Open media folder
    Copy,          // Copy text
    CopySmall,     // Smaller copy icon for preset buttons
    Close,         // "X" for clearing search / closing
    Plus,          // Add/create action
    Edit,          // Rename/edit action
    DragIndicator, // Six-dot preset reorder handle

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
    DeployedCode,    // Image to 3D  (Android: ms_deployed_code)
    DrawCollage,     // Image to SVG (Android: ms_draw_collage)

    // Providers (aligned with the Android settings dialog)
    ElectricBolt, // groq        (Android: ms_electric_bolt)
    Whatshot,     // cerebras    (Android: ms_local_fire_department)
    AutoAwesome,  // google / gemini  (Android: ms_auto_awesome)
    Translate,    // google-gtx  (Android: ms_translate)
    Terminal,     // ollama      (Android: ms_terminal)
    Public,       // openrouter  (Android: ms_public)
    QrCode,       // qrserver
    SpeechToText, // parakeet / local ASR providers
    Rocket,       // taalas
    Search,
    Stat3,
    Stat2,
    Stat1,
    StatMinus1,
    StatMinus2,
    StatMinus3,

    // Reorder + dropdown/collapsing chevrons (keyboard-arrow family)
    ArrowUp,
    ArrowDown,
    ArrowRight,
    // Straight "flow" arrow (A → B), distinct from the chevron family above.
    ArrowRightAlt,
    // Notched flow arrow used inline inside "X → Y" labels (node titles etc.).
    LineEndArrowNotch,
    // Computer Control preset (robot/agent).
    SmartToy,

    // Section headers / status
    Key,
    Upgrade,
    CheckCircle,
    Warning,
    PottedPlant, // Donation section (Material Symbols potted_plant)

    // Window Controls
    Minimize,
    Maximize,
    Restore,
}

#[derive(Clone, Copy)]
struct AtlasPage {
    pixels: u32,
    width: u32,
    height: u32,
    png: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/icon_atlas_generated.rs"));

thread_local! {
    /// Physical raster size -> atlas texture. egui rendering is single-threaded
    /// per context, so a thread-local cache avoids synchronization overhead.
    static ICON_TEXTURES: RefCell<HashMap<u32, egui::TextureHandle>> = RefCell::new(HashMap::new());
}

fn nearest_atlas_page(target: u32) -> &'static AtlasPage {
    ICON_ATLAS_PAGES
        .iter()
        .min_by(|left, right| {
            left.pixels
                .abs_diff(target)
                .cmp(&right.pixels.abs_diff(target))
                .then_with(|| right.pixels.cmp(&left.pixels))
        })
        .expect("icon atlas has at least one page")
}

fn decode_atlas(page: &AtlasPage) -> egui::ColorImage {
    let decoder = image::codecs::png::PngDecoder::new(Cursor::new(page.png))
        .expect("generated icon atlas is a valid PNG");
    assert_eq!(decoder.dimensions(), (page.width, page.height));
    assert_eq!(decoder.color_type(), image::ColorType::L8);
    let mut alpha = vec![0; decoder.total_bytes() as usize];
    decoder
        .read_image(&mut alpha)
        .expect("generated icon atlas decodes");
    let mut rgba = Vec::with_capacity(alpha.len() * 4);
    for value in alpha {
        rgba.extend_from_slice(&[value, value, value, value]);
    }
    egui::ColorImage::from_rgba_premultiplied([page.width as usize, page.height as usize], &rgba)
}

fn icon_texture(ctx: &egui::Context, icon: Icon, target: u32) -> (egui::TextureId, egui::Rect) {
    let page = nearest_atlas_page(target);
    let texture = ICON_TEXTURES.with(|cache| {
        if let Some(id) = cache
            .borrow()
            .get(&page.pixels)
            .map(egui::TextureHandle::id)
        {
            return id;
        }
        let handle = ctx.load_texture(
            format!("icon-atlas::{}", page.pixels),
            decode_atlas(page),
            egui::TextureOptions::LINEAR,
        );
        let id = handle.id();
        cache.borrow_mut().insert(page.pixels, handle);
        id
    });
    let cell = page.pixels + ICON_ATLAS_GUTTER * 2;
    let index = icon.sprite_index() as u32;
    assert!((index as usize) < ICON_SPRITE_COUNT);
    let x = (index % ICON_ATLAS_COLUMNS) * cell + ICON_ATLAS_GUTTER;
    let y = (index / ICON_ATLAS_COLUMNS) * cell + ICON_ATLAS_GUTTER;
    let uv = egui::Rect::from_min_max(
        egui::pos2(x as f32 / page.width as f32, y as f32 / page.height as f32),
        egui::pos2(
            (x + page.pixels) as f32 / page.width as f32,
            (y + page.pixels) as f32 / page.height as f32,
        ),
    );
    (texture, uv)
}

fn resolved_color(icon: Icon, dark_mode: bool, requested: egui::Color32) -> egui::Color32 {
    if icon == Icon::StarFilled {
        egui::Color32::from_rgb(255, 193, 7)
    } else if icon == Icon::Star && !dark_mode {
        egui::Color32::from_rgb(110, 110, 110)
    } else {
        requested
    }
}

/// Paint `icon`, recolored to `color`, centered in `rect`.
fn render_icon_with_opacity(
    painter: &egui::Painter,
    rect: egui::Rect,
    icon: Icon,
    color: egui::Color32,
    opacity: f32,
) {
    let ctx = painter.ctx();
    // A filled favorite star is conventionally gold, regardless of widget state.
    // The outline (non-favorite) star is a thin frame that washes out on a light
    // background, so give it a darker, more visible shade in light mode.
    let color = resolved_color(icon, ctx.global_style().visuals.dark_mode, color)
        .gamma_multiply(opacity.clamp(0.0, 1.0));
    let ppp = ctx.pixels_per_point().max(0.01);
    // Material Symbols' "filled" glyphs fill their box more heavily than the old
    // thin line-art, so draw them inside ~84% of the allocated rect to keep the
    // visual weight in line with the previous icons (and existing call sizes).
    const GLYPH_FILL: f32 = 0.84;
    let target = rect.width().min(rect.height()) * GLYPH_FILL;
    if target <= 0.5 {
        return;
    }
    // Select the closest build-time raster, draw at a whole number of physical
    // pixels, and snap the destination to the physical grid. Every standard
    // 100-400% Windows scaling target has an exact pre-rasterized page.
    let px = (target * ppp).round().clamp(6.0, 512.0);
    let side = px / ppp;
    let (texture, uv) = icon_texture(ctx, icon, px as u32);
    let mut min = rect.center() - egui::vec2(side, side) * 0.5;
    min.x = (min.x * ppp).round() / ppp;
    min.y = (min.y * ppp).round() / ppp;
    let icon_rect = egui::Rect::from_min_size(min, egui::vec2(side, side));
    // Tint the white glyph -> `color` (white * color = color, alpha preserved).
    painter.image(texture, icon_rect, uv, color);
    // No dedicated "copy-off" Material symbol exists — overlay a slash.
    if icon == Icon::CopyDisabled {
        let r = icon_rect.shrink(side * 0.1);
        painter.line_segment(
            [r.left_bottom(), r.right_top()],
            egui::Stroke::new((side * 0.09).max(1.5), color),
        );
    }
}

fn render_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: egui::Color32) {
    render_icon_with_opacity(painter, rect, icon, color, 1.0);
}

/// Main entry point: Draw a clickable icon button (default `ICON_XL` — the
/// standard for standalone toolbar/control buttons).
pub fn icon_button(ui: &mut egui::Ui, icon: Icon) -> egui::Response {
    icon_button_sized(ui, icon, ICON_XL)
}

/// Draw a clickable icon button with custom size
pub fn icon_button_sized(ui: &mut egui::Ui, icon: Icon, size_val: f32) -> egui::Response {
    icon_button_sized_with_opacity(ui, icon, size_val, 1.0)
}

/// Draw a clickable icon button while fading only its paint. The allocated hit
/// target remains stable so proximity-revealed controls can still be reached.
pub fn icon_button_sized_with_opacity(
    ui: &mut egui::Ui,
    icon: Icon,
    size_val: f32,
    opacity: f32,
) -> egui::Response {
    let size = egui::vec2(size_val, size_val);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let opacity = opacity.clamp(0.0, 1.0);

    // 1. Background Hover Effect
    if response.hovered() {
        ui.painter().rect_filled(
            rect.shrink(2.0),
            4.0,
            ui.visuals().widgets.hovered.bg_fill.gamma_multiply(opacity),
        );
    }

    // 2. Determine Style
    let color = if response.hovered() {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().widgets.inactive.fg_stroke.color
    };

    // 3. Paint
    render_icon_with_opacity(ui.painter(), rect, icon, color, opacity);

    response
}

/// Draw a static icon (for labels/headers)
pub fn draw_icon_static(ui: &mut egui::Ui, icon: Icon, size_override: Option<f32>) {
    let side = size_override.unwrap_or(ICON_MD);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let color = ui.visuals().text_color();
    render_icon(ui.painter(), rect, icon, color);
}

/// Render an `"X → Y"` label with the Material `line_end_arrow_notch` icon in
/// place of the arrow, laid out inline in the current (horizontal) ui. The text
/// is a localized string that uses `→` as the split point; falls back to a
/// plain styled label when there's no arrow. `decorate` styles each text half
/// identically (size/strong/color); `color` tints the arrow icon (defaults to
/// the ui text color).
pub fn arrow_label(
    ui: &mut egui::Ui,
    text: &str,
    color: Option<egui::Color32>,
    decorate: impl Fn(egui::RichText) -> egui::RichText,
) {
    match text.split_once('→') {
        Some((lhs, rhs)) => {
            ui.label(decorate(egui::RichText::new(lhs.trim())));
            let side = ICON_SM;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
            let c = color.unwrap_or_else(|| ui.visuals().text_color());
            render_icon(ui.painter(), rect, Icon::LineEndArrowNotch, c);
            ui.label(decorate(egui::RichText::new(rhs.trim())));
        }
        None => {
            ui.label(decorate(egui::RichText::new(text)));
        }
    }
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
        "parakeet" | "qwen3" => Icon::SpeechToText,
        "taalas" => Icon::Rocket,
        _ => Icon::Settings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const WINDOWS_DPI_TARGETS: &[u32] = &[
        11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 29, 30, 33, 34, 35, 38,
        40, 41, 42, 44, 45, 47, 50, 53, 54, 59, 60, 67,
    ];

    #[test]
    fn generated_mapping_is_exhaustive_and_preserves_aliases() {
        assert_eq!(ALL_ICONS.len(), 71);
        let indices = ALL_ICONS
            .iter()
            .map(|icon| icon.sprite_index())
            .collect::<BTreeSet<_>>();
        assert_eq!(indices, (0..ICON_SPRITE_COUNT).collect());
        assert_eq!(Icon::Copy.sprite_index(), Icon::CopySmall.sprite_index());
        assert_eq!(Icon::Copy.sprite_index(), Icon::CopyDisabled.sprite_index());
        assert_eq!(
            Icon::Delete.sprite_index(),
            Icon::DeleteLarge.sprite_index()
        );
    }

    #[test]
    fn atlas_pages_cover_standard_windows_scaling() {
        assert!(
            ICON_ATLAS_PAGES
                .windows(2)
                .all(|pages| pages[0].pixels < pages[1].pixels)
        );
        for &target in WINDOWS_DPI_TARGETS {
            let selected = nearest_atlas_page(target).pixels;
            assert_eq!(selected, target);
        }
    }

    #[test]
    fn every_generated_cell_has_pixels_and_a_clear_gutter() {
        let total_png_bytes = ICON_ATLAS_PAGES
            .iter()
            .map(|page| page.png.len())
            .sum::<usize>();
        assert!(total_png_bytes < 410_000);
        for page in ICON_ATLAS_PAGES {
            let image = decode_atlas(page);
            let cell = page.pixels + ICON_ATLAS_GUTTER * 2;
            for index in 0..ICON_SPRITE_COUNT as u32 {
                let x = (index % ICON_ATLAS_COLUMNS) * cell + ICON_ATLAS_GUTTER;
                let y = (index / ICON_ATLAS_COLUMNS) * cell + ICON_ATLAS_GUTTER;
                let alpha = |x: u32, y: u32| image.pixels[(y * page.width + x) as usize].a();
                assert!(
                    (y..y + page.pixels)
                        .any(|row| { (x..x + page.pixels).any(|column| alpha(column, row) != 0) })
                );
                assert!((x..x + page.pixels).all(|column| {
                    alpha(column, y - 1) == 0 && alpha(column, y + page.pixels) == 0
                }));
                assert!(
                    (y..y + page.pixels)
                        .all(|row| { alpha(x - 1, row) == 0 && alpha(x + page.pixels, row) == 0 })
                );
            }
        }
    }

    #[test]
    fn star_colors_and_copy_disabled_art_remain_stable() {
        let requested = egui::Color32::from_rgb(12, 34, 56);
        assert_eq!(
            resolved_color(Icon::StarFilled, true, requested),
            egui::Color32::from_rgb(255, 193, 7)
        );
        assert_eq!(
            resolved_color(Icon::Star, false, requested),
            egui::Color32::from_rgb(110, 110, 110)
        );
        assert_eq!(resolved_color(Icon::Star, true, requested), requested);
        assert_eq!(Icon::CopyDisabled.sprite_index(), Icon::Copy.sprite_index());
    }
}
