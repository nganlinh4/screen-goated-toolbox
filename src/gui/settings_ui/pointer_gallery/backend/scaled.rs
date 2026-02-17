use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const SCALE_CACHE_VERSION: &str = "v2-hq";

#[cfg(target_os = "windows")]
pub(super) fn scaled_cursor_file_map_for_size(
    files: &HashMap<String, PathBuf>,
    target_size: u32,
    live_preview_only: bool,
) -> HashMap<String, PathBuf> {
    let mut mapped = HashMap::with_capacity(files.len());

    for (file_name, source_path) in files {
        if !should_scale_file(file_name, source_path, live_preview_only) {
            mapped.insert(file_name.clone(), source_path.clone());
            continue;
        }

        let maybe_scaled = scaled_path_for_file(source_path, target_size).and_then(|destination| {
            ensure_scaled_cur(source_path, &destination, target_size)
                .ok()
                .map(|_| destination)
        });

        match maybe_scaled {
            Some(path) => {
                mapped.insert(file_name.clone(), path);
            }
            None => {
                mapped.insert(file_name.clone(), source_path.clone());
            }
        }
    }

    mapped
}

#[cfg(not(target_os = "windows"))]
pub(super) fn scaled_cursor_file_map_for_size(
    files: &HashMap<String, PathBuf>,
    _target_size: u32,
    _live_preview_only: bool,
) -> HashMap<String, PathBuf> {
    files.clone()
}

#[cfg(target_os = "windows")]
fn should_scale_file(file_name: &str, source_path: &Path, live_preview_only: bool) -> bool {
    let is_cur = source_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("cur"));
    if !is_cur {
        return false;
    }

    if live_preview_only {
        return file_name.eq_ignore_ascii_case("pointer.cur");
    }

    true
}

#[cfg(target_os = "windows")]
fn scaled_path_for_file(source_path: &Path, target_size: u32) -> Option<PathBuf> {
    let parent = source_path.parent()?;
    let file_name = source_path.file_name()?;
    Some(
        parent
            .join(".scaled")
            .join(SCALE_CACHE_VERSION)
            .join(target_size.to_string())
            .join(file_name),
    )
}

#[cfg(target_os = "windows")]
fn ensure_scaled_cur(
    source_path: &Path,
    destination: &Path,
    target_size: u32,
) -> Result<bool, String> {
    if is_fresh_scaled_copy(source_path, destination) {
        return Ok(false);
    }

    let metadata = parse_cur_metadata(source_path).unwrap_or(CurMetadata {
        width: 32,
        height: 32,
        hotspot_x: 0,
        hotspot_y: 0,
    });
    let source_render_size = metadata.width.max(metadata.height).clamp(16, 128);
    let source_rgba = render_cursor_rgba(source_path, source_render_size)?;
    let rgba = if source_render_size == target_size {
        source_rgba
    } else {
        resize_rgba_square(&source_rgba, source_render_size, target_size)?
    };

    let png_bytes = encode_png_rgba(&rgba, target_size)?;

    let hotspot_x = scale_hotspot(metadata.hotspot_x, metadata.width, target_size);
    let hotspot_y = scale_hotspot(metadata.hotspot_y, metadata.height, target_size);
    let cur_bytes = build_single_image_cur(target_size, hotspot_x, hotspot_y, &png_bytes)?;

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed creating scaled cursor folder {:?}: {}", parent, e))?;
    }

    let temp_path = destination.with_extension("tmp");
    fs::write(&temp_path, &cur_bytes)
        .map_err(|e| format!("Failed writing scaled cursor {:?}: {}", temp_path, e))?;
    fs::rename(&temp_path, destination).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        format!(
            "Failed moving scaled cursor into place {:?}: {}",
            destination, e
        )
    })?;

    Ok(true)
}

#[cfg(target_os = "windows")]
fn resize_rgba_square(
    source_rgba: &[u8],
    source_size: u32,
    target_size: u32,
) -> Result<Vec<u8>, String> {
    use image::imageops::FilterType;
    use image::RgbaImage;

    let source_image = RgbaImage::from_raw(source_size, source_size, source_rgba.to_vec())
        .ok_or_else(|| "Failed creating source RGBA image for resizing.".to_string())?;
    let resized = image::imageops::resize(
        &source_image,
        target_size,
        target_size,
        FilterType::Lanczos3,
    );
    Ok(resized.into_raw())
}

#[cfg(target_os = "windows")]
fn is_fresh_scaled_copy(source_path: &Path, destination: &Path) -> bool {
    let Ok(dest_meta) = fs::metadata(destination) else {
        return false;
    };
    if dest_meta.len() == 0 {
        return false;
    }
    let Ok(src_meta) = fs::metadata(source_path) else {
        return false;
    };

    match (src_meta.modified(), dest_meta.modified()) {
        (Ok(src), Ok(dest)) => dest >= src,
        _ => false,
    }
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
struct CurMetadata {
    width: u32,
    height: u32,
    hotspot_x: u16,
    hotspot_y: u16,
}

#[cfg(target_os = "windows")]
fn parse_cur_metadata(path: &Path) -> Option<CurMetadata> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() < 22 {
        return None;
    }

    let reserved = u16::from_le_bytes([bytes[0], bytes[1]]);
    let icon_type = u16::from_le_bytes([bytes[2], bytes[3]]);
    let count = u16::from_le_bytes([bytes[4], bytes[5]]);
    if reserved != 0 || icon_type != 2 || count == 0 {
        return None;
    }

    let entry_count = usize::from(count);
    let mut best: Option<CurMetadata> = None;
    for idx in 0..entry_count {
        let base = 6 + idx * 16;
        if base + 16 > bytes.len() {
            break;
        }
        let width = if bytes[base] == 0 {
            256
        } else {
            bytes[base] as u32
        };
        let height = if bytes[base + 1] == 0 {
            256
        } else {
            bytes[base + 1] as u32
        };
        let hotspot_x = u16::from_le_bytes([bytes[base + 4], bytes[base + 5]]);
        let hotspot_y = u16::from_le_bytes([bytes[base + 6], bytes[base + 7]]);

        let candidate = CurMetadata {
            width,
            height,
            hotspot_x,
            hotspot_y,
        };

        let replace = match best {
            None => true,
            Some(current) => {
                let current_area = current.width.saturating_mul(current.height);
                let candidate_area = candidate.width.saturating_mul(candidate.height);
                candidate_area > current_area
                    || (candidate_area == current_area
                        && candidate.width.max(candidate.height)
                            > current.width.max(current.height))
            }
        };
        if replace {
            best = Some(candidate);
        }
    }

    best
}

#[cfg(target_os = "windows")]
fn scale_hotspot(original: u16, source_dim: u32, target_dim: u32) -> u16 {
    if source_dim == 0 || target_dim == 0 {
        return 0;
    }

    let scaled = ((original as u32) * target_dim + (source_dim / 2)) / source_dim;
    let capped = scaled
        .min(target_dim.saturating_sub(1))
        .min(u16::MAX as u32);
    capped as u16
}

#[cfg(target_os = "windows")]
fn encode_png_rgba(rgba: &[u8], target_size: u32) -> Result<Vec<u8>, String> {
    use image::codecs::png::PngEncoder;
    use image::ImageEncoder;

    let mut png_bytes = Vec::new();
    let encoder = PngEncoder::new(&mut png_bytes);
    encoder
        .write_image(
            rgba,
            target_size,
            target_size,
            image::ColorType::Rgba8.into(),
        )
        .map_err(|e| format!("Failed encoding scaled cursor PNG: {}", e))?;
    Ok(png_bytes)
}

#[cfg(target_os = "windows")]
fn build_single_image_cur(
    target_size: u32,
    hotspot_x: u16,
    hotspot_y: u16,
    png_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let png_len = u32::try_from(png_bytes.len())
        .map_err(|_| "Scaled cursor image is too large to store.".to_string())?;
    let entry_offset = 6u32 + 16u32;
    let width_byte = if target_size >= 256 {
        0
    } else {
        target_size as u8
    };
    let height_byte = if target_size >= 256 {
        0
    } else {
        target_size as u8
    };

    let mut out = Vec::with_capacity(entry_offset as usize + png_bytes.len());
    out.extend_from_slice(&0u16.to_le_bytes()); // Reserved
    out.extend_from_slice(&2u16.to_le_bytes()); // CUR type
    out.extend_from_slice(&1u16.to_le_bytes()); // One image
    out.push(width_byte);
    out.push(height_byte);
    out.push(0); // color count
    out.push(0); // reserved
    out.extend_from_slice(&hotspot_x.to_le_bytes());
    out.extend_from_slice(&hotspot_y.to_le_bytes());
    out.extend_from_slice(&png_len.to_le_bytes());
    out.extend_from_slice(&entry_offset.to_le_bytes());
    out.extend_from_slice(png_bytes);
    Ok(out)
}

#[cfg(target_os = "windows")]
fn render_cursor_rgba(path: &Path, target_size: u32) -> Result<Vec<u8>, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        DestroyCursor, DrawIconEx, LoadCursorFromFileW, LoadImageW, DI_NORMAL, IMAGE_CURSOR,
        LR_LOADFROMFILE,
    };

    let size_i32 = i32::try_from(target_size).map_err(|_| "Invalid cursor size.".to_string())?;
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let cursor = match LoadImageW(
            None,
            PCWSTR(wide.as_ptr()),
            IMAGE_CURSOR,
            size_i32,
            size_i32,
            LR_LOADFROMFILE,
        ) {
            Ok(handle) => windows::Win32::UI::WindowsAndMessaging::HCURSOR(handle.0),
            Err(_) => LoadCursorFromFileW(PCWSTR(wide.as_ptr()))
                .map_err(|e| format!("Failed loading cursor {:?}: {}", path, e))?,
        };

        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            let _ = DestroyCursor(cursor);
            return Err("Failed acquiring screen device context.".to_string());
        }

        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, screen_dc);
            let _ = DestroyCursor(cursor);
            return Err("Failed creating compatible device context.".to_string());
        }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size_i32,
                biHeight: -size_i32,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
            .map_err(|e| format!("Failed creating bitmap buffer: {}", e))?;
        let old_bm = SelectObject(mem_dc, dib.into());

        if !bits.is_null() {
            std::ptr::write_bytes(bits, 0, (size_i32 * size_i32 * 4) as usize);
        }

        let _ = DrawIconEx(
            mem_dc,
            0,
            0,
            cursor.into(),
            size_i32,
            size_i32,
            0,
            None,
            DI_NORMAL,
        );

        let bytes =
            std::slice::from_raw_parts(bits as *const u8, (size_i32 * size_i32 * 4) as usize);
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

        Ok(rgba)
    }
}
