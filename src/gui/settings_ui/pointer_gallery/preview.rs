use super::{CollectionState, CollectionStatus};
use crate::gui::locale::LocaleText;
use eframe::egui;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

const SLOT_GAP: f32 = 3.0;
const ANI_FRAME_INTERVAL_SECS: f64 = 0.10;
const ANI_REPAINT_MS: u64 = 100;
const MAX_ANI_PREVIEW_FRAMES: u32 = 8;
const PREVIEW_RENDER_SIZE: i32 = 72;
const PREVIEW_CACHE_VERSION: &str = "v1";

pub(super) struct PreviewTexture {
    pub(super) frames: Vec<egui::TextureHandle>,
    pub(super) animated: bool,
    pub(super) ani_step: usize,
    pub(super) last_update_secs: f64,
}

pub(super) fn preview_strip_width(slot_count: usize, icon_size: f32) -> f32 {
    let gaps = slot_count.saturating_sub(1) as f32 * SLOT_GAP;
    slot_count as f32 * icon_size + gaps
}

pub(super) fn render_collection_previews(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    collection: &CollectionState,
    texture_cache: &mut HashMap<String, PreviewTexture>,
    slot_names: &[&str],
    icon_size: f32,
) {
    let now = ctx.input(|i| i.time);
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = SLOT_GAP;
        for file_name in slot_names {
            let Some(path) = collection.files.get(*file_name) else {
                render_preview_placeholder(ui, icon_size, file_name);
                continue;
            };

            let key = path.to_string_lossy().to_string();

            if !texture_cache.contains_key(&key) {
                if let Some(texture) = load_cursor_preview_texture(ctx, path, &key, now) {
                    texture_cache.insert(key.clone(), texture);
                }
            }

            if let Some(preview) = texture_cache.get_mut(&key) {
                if preview.animated && preview.frames.len() > 1 {
                    if now - preview.last_update_secs >= ANI_FRAME_INTERVAL_SECS {
                        preview.ani_step = (preview.ani_step + 1) % preview.frames.len();
                        preview.last_update_secs = now;
                    }
                    ctx.request_repaint_after(Duration::from_millis(ANI_REPAINT_MS));
                }

                let frame_idx = preview.ani_step.min(preview.frames.len().saturating_sub(1));
                if let Some(texture) = preview.frames.get(frame_idx) {
                    let response = ui.image((texture.id(), egui::vec2(icon_size, icon_size)));
                    response.on_hover_text(*file_name);
                } else {
                    render_preview_placeholder(ui, icon_size, file_name);
                }
            } else {
                render_preview_placeholder(ui, icon_size, file_name);
            }
        }
    });
}

pub(super) fn render_status_label(ui: &mut egui::Ui, status: &CollectionStatus, text: &LocaleText) {
    match status {
        CollectionStatus::Queued => {
            ui.label(egui::RichText::new(text.pointer_status_queued).strong());
        }
        CollectionStatus::Downloading { downloaded, total } => {
            ui.label(
                egui::RichText::new(
                    text.pointer_status_downloading_fmt
                        .replacen("{}", &downloaded.to_string(), 1)
                        .replacen("{}", &total.to_string(), 1),
                )
                .color(egui::Color32::from_rgb(255, 165, 0))
                .strong(),
            );
        }
        CollectionStatus::Paused { downloaded, total } => {
            ui.label(
                egui::RichText::new(
                    text.pointer_status_paused_fmt
                        .replacen("{}", &downloaded.to_string(), 1)
                        .replacen("{}", &total.to_string(), 1),
                )
                .color(egui::Color32::from_rgb(255, 215, 0))
                .strong(),
            );
        }
        CollectionStatus::Ready => {
            ui.label(
                egui::RichText::new(text.pointer_status_ready)
                    .color(egui::Color32::from_rgb(34, 139, 34))
                    .strong(),
            );
        }
        CollectionStatus::Applying => {
            ui.label(
                egui::RichText::new(text.pointer_status_applying)
                    .color(egui::Color32::from_rgb(255, 215, 0))
                    .strong(),
            );
        }
        CollectionStatus::Applied => {
            ui.label(
                egui::RichText::new(text.pointer_status_applied)
                    .color(egui::Color32::from_rgb(34, 139, 34))
                    .strong(),
            );
        }
        CollectionStatus::Error(message) => {
            ui.label(
                egui::RichText::new(text.pointer_status_error)
                    .color(egui::Color32::from_rgb(205, 92, 92))
                    .strong(),
            )
            .on_hover_text(message);
        }
    }
}

fn render_preview_placeholder(ui: &mut egui::Ui, icon_size: f32, file_name: &str) {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(icon_size, icon_size), egui::Sense::hover());
    let stroke = egui::Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.35));
    ui.painter()
        .rect_stroke(rect.shrink(0.5), 2.0, stroke, egui::StrokeKind::Inside);
    response.on_hover_text(file_name);
}

#[cfg(target_os = "windows")]
fn load_cursor_preview_texture(
    ctx: &egui::Context,
    path: &Path,
    cache_key: &str,
    now: f64,
) -> Option<PreviewTexture> {
    let mut frames = Vec::new();

    if let Some(total_steps) = ani_frame_count(path).filter(|steps| *steps > 1) {
        for (idx, ani_step) in sample_ani_steps(total_steps).iter().copied().enumerate() {
            let Some(image) = load_or_render_cached_preview_image(path, ani_step, PREVIEW_RENDER_SIZE)
            else {
                continue;
            };
            frames.push(ctx.load_texture(
                format!("pointer_preview_{}_{}", cache_key, idx),
                image,
                egui::TextureOptions::LINEAR,
            ));
        }
    }

    if frames.is_empty() {
        let image = load_or_render_cached_preview_image(path, 0, PREVIEW_RENDER_SIZE)?;
        frames.push(ctx.load_texture(
            format!("pointer_preview_{}", cache_key),
            image,
            egui::TextureOptions::LINEAR,
        ));
    }

    Some(PreviewTexture {
        animated: frames.len() > 1,
        frames,
        ani_step: 0,
        last_update_secs: now,
    })
}

#[cfg(target_os = "windows")]
fn load_or_render_cached_preview_image(path: &Path, ani_step: u32, size: i32) -> Option<egui::ColorImage> {
    if let Some(cache_path) = preview_cache_file(path, ani_step, size) {
        if let Some(image) = read_cached_preview_png(&cache_path, size) {
            return Some(image);
        }
    }

    let image = cursor_file_to_color_image(path, ani_step, size)?;
    if let Some(cache_path) = preview_cache_file(path, ani_step, size) {
        let _ = write_cached_preview_png(&cache_path, &image);
    }
    Some(image)
}

#[cfg(target_os = "windows")]
fn preview_cache_file(path: &Path, ani_step: u32, size: i32) -> Option<std::path::PathBuf> {
    let collection_dir = path.parent()?;
    let gallery_root = collection_dir.parent().unwrap_or(collection_dir);
    let collection_id = sanitize_cache_component(collection_dir.file_name()?.to_string_lossy().as_ref());
    let stem = sanitize_cache_component(path.file_stem()?.to_string_lossy().as_ref());
    let ext = sanitize_cache_component(path.extension()?.to_string_lossy().as_ref());
    let (file_len, modified_ns) = source_file_fingerprint(path)?;

    let file_name = format!(
        "{}_{}_{}_{}_{}_{}.png",
        stem, ext, file_len, modified_ns, size, ani_step
    );

    Some(
        gallery_root
            .join(".preview-cache")
            .join(PREVIEW_CACHE_VERSION)
            .join(collection_id)
            .join(file_name),
    )
}

#[cfg(target_os = "windows")]
fn source_file_fingerprint(path: &Path) -> Option<(u64, u128)> {
    let metadata = fs::metadata(path).ok()?;
    let len = metadata.len();
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    Some((len, modified_ns))
}

#[cfg(target_os = "windows")]
fn sanitize_cache_component(raw: &str) -> String {
    let mut sanitized = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }
    if sanitized.is_empty() {
        "cursor".to_string()
    } else {
        sanitized
    }
}

#[cfg(target_os = "windows")]
fn read_cached_preview_png(path: &Path, expected_size: i32) -> Option<egui::ColorImage> {
    let image = image::open(path).ok()?.to_rgba8();
    if image.width() as i32 != expected_size || image.height() as i32 != expected_size {
        return None;
    }
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        image.as_raw(),
    ))
}

#[cfg(target_os = "windows")]
fn write_cached_preview_png(path: &Path, image: &egui::ColorImage) -> Option<()> {
    let parent = path.parent()?;
    if fs::create_dir_all(parent).is_err() {
        return None;
    }

    let width = image.size[0] as u32;
    let height = image.size[1] as u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for pixel in &image.pixels {
        rgba.extend_from_slice(&pixel.to_array());
    }

    image::save_buffer(path, &rgba, width, height, image::ColorType::Rgba8).ok()?;
    Some(())
}

#[cfg(not(target_os = "windows"))]
fn load_cursor_preview_texture(
    _ctx: &egui::Context,
    _path: &Path,
    _cache_key: &str,
    _now: f64,
) -> Option<PreviewTexture> {
    None
}

#[cfg(target_os = "windows")]
fn cursor_file_to_color_image(path: &Path, ani_step: u32, size: i32) -> Option<egui::ColorImage> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyCursor, DrawIconEx, LoadCursorFromFileW, DI_NORMAL,
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let cursor = LoadCursorFromFileW(PCWSTR(wide.as_ptr())).ok()?;

        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            let _ = DestroyCursor(cursor);
            return None;
        }

        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, screen_dc);
            let _ = DestroyCursor(cursor);
            return None;
        }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        let old_bm = SelectObject(mem_dc, dib.into());

        if !bits.is_null() {
            std::ptr::write_bytes(bits, 0, (size * size * 4) as usize);
        }

        let _ = DrawIconEx(
            mem_dc,
            0,
            0,
            cursor.into(),
            size,
            size,
            ani_step,
            None,
            DI_NORMAL,
        );

        let bytes = std::slice::from_raw_parts(bits as *const u8, (size * size * 4) as usize);
        let mut rgba = Vec::with_capacity(bytes.len());
        for px in bytes.chunks_exact(4) {
            rgba.push(px[2]);
            rgba.push(px[1]);
            rgba.push(px[0]);
            rgba.push(px[3]);
        }

        let _ = SelectObject(mem_dc, old_bm);
        let _ = DeleteObject(dib.into());
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);
        let _ = DestroyCursor(cursor);

        Some(egui::ColorImage::from_rgba_unmultiplied(
            [size as usize, size as usize],
            &rgba,
        ))
    }
}

#[cfg(not(target_os = "windows"))]
fn cursor_file_to_color_image(
    _path: &Path,
    _ani_step: u32,
    _size: i32,
) -> Option<egui::ColorImage> {
    None
}

fn sample_ani_steps(total_steps: u32) -> Vec<u32> {
    if total_steps <= 1 {
        return vec![0];
    }

    let sampled_steps = total_steps.min(MAX_ANI_PREVIEW_FRAMES);
    let mut steps = Vec::with_capacity(sampled_steps as usize);
    for idx in 0..sampled_steps {
        let step = idx.saturating_mul(total_steps) / sampled_steps;
        if steps.last().copied() != Some(step) {
            steps.push(step);
        }
    }

    if steps.is_empty() {
        steps.push(0);
    }

    steps
}

fn ani_frame_count(path: &Path) -> Option<u32> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"ACON" {
        return None;
    }

    let mut offset = 12usize;
    while offset + 8 <= bytes.len() {
        let chunk_id = &bytes[offset..offset + 4];
        let chunk_size =
            u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        let data_start = offset + 8;
        let data_end = data_start.saturating_add(chunk_size);
        if data_end > bytes.len() {
            break;
        }

        if chunk_id == b"anih" && chunk_size >= 12 {
            let steps = u32::from_le_bytes(bytes[data_start + 8..data_start + 12].try_into().ok()?);
            if steps > 0 {
                return Some(steps);
            }
        }

        let padded = if chunk_size % 2 == 1 {
            chunk_size + 1
        } else {
            chunk_size
        };
        offset = data_start.saturating_add(padded);
    }

    None
}
