// Disk cache for pre-rendered animated cursor frames.
//
// Computed once from SVG freeze-frame capture, saved to
// %LOCALAPPDATA%/screen-goated-toolbox/cursor-anim-cache/.
// Subsequent loads skip all SVG rendering and read straight from disk.
//
// File format per slot: simple length-prefixed frames with a header.
// Export frames are PNG (decoded to RGBA for the persistent ANIMATED_CURSORS store).
// Preview frames are frozen SVG text (returned as raw bytes for JS to reconstruct).

use std::fs;
use std::path::PathBuf;

use super::config::AnimatedCursorSlotData;
use super::staging;

const MAGIC: &[u8; 8] = b"SGT_ANIM";
const FORMAT_VERSION: u32 = 2;
const TILE: u32 = 512;

fn cache_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| {
        base.join("screen-goated-toolbox")
            .join("cursor-anim-cache")
    })
}

fn cache_path(slot_id: u32, svg_hash: &str) -> Option<PathBuf> {
    cache_dir().map(|dir| dir.join(format!("slot_{slot_id}_{svg_hash}.bin")))
}

/// Try to load cached animation data for a slot.
/// On success, populates the persistent ANIMATED_CURSORS store with export
/// frames and returns the preview PNG bytes for JS to reconstruct canvases.
pub fn load_cache(
    slot_id: u32,
    svg_hash: &str,
) -> Option<CacheLoadResult> {
    let path = cache_path(slot_id, svg_hash)?;
    if !path.exists() {
        return None;
    }
    let data = fs::read(&path).ok()?;
    let result = parse_cache_file(&data, slot_id)?;

    // Populate the persistent export store immediately.
    staging::set_animated_cursor_slot(AnimatedCursorSlotData {
        slot_id,
        loop_duration: result.loop_duration,
        frames: result.export_rgba_frames,
    });

    Some(CacheLoadResult {
        loop_duration: result.loop_duration,
        natural_width: result.natural_width,
        natural_height: result.natural_height,
        preview_pngs: result.preview_pngs,
    })
}

pub struct CacheLoadResult {
    pub loop_duration: f64,
    pub natural_width: u32,
    pub natural_height: u32,
    /// PNG-encoded preview frames (128×128) for JS to reconstruct canvases.
    pub preview_pngs: Vec<Vec<u8>>,
}

struct ParsedCache {
    loop_duration: f64,
    natural_width: u32,
    natural_height: u32,
    export_rgba_frames: Vec<Vec<u8>>,
    preview_pngs: Vec<Vec<u8>>,
}

fn parse_cache_file(data: &[u8], expected_slot: u32) -> Option<ParsedCache> {
    let mut pos = 0;

    // Magic
    if data.len() < 8 || &data[0..8] != MAGIC {
        return None;
    }
    pos += 8;

    let read_u32 = |p: &mut usize| -> Option<u32> {
        if *p + 4 > data.len() { return None; }
        let v = u32::from_le_bytes(data[*p..*p + 4].try_into().ok()?);
        *p += 4;
        Some(v)
    };
    let read_f64 = |p: &mut usize| -> Option<f64> {
        if *p + 8 > data.len() { return None; }
        let v = f64::from_le_bytes(data[*p..*p + 8].try_into().ok()?);
        *p += 8;
        Some(v)
    };

    let version = read_u32(&mut pos)?;
    if version != FORMAT_VERSION {
        return None;
    }
    let slot_id = read_u32(&mut pos)?;
    if slot_id != expected_slot {
        return None;
    }
    let loop_duration = read_f64(&mut pos)?;
    let natural_width = read_u32(&mut pos)?;
    let natural_height = read_u32(&mut pos)?;
    let export_count = read_u32(&mut pos)? as usize;
    let preview_count = read_u32(&mut pos)? as usize;

    let expected_rgba = (TILE * TILE * 4) as usize;

    // Read export frames (PNG → decode to RGBA)
    let mut export_rgba_frames = Vec::with_capacity(export_count);
    for _ in 0..export_count {
        let png_len = read_u32(&mut pos)? as usize;
        if pos + png_len > data.len() { return None; }
        let png_data = &data[pos..pos + png_len];
        pos += png_len;

        let img = image::load_from_memory(png_data).ok()?;
        let rgba = if img.width() == TILE && img.height() == TILE {
            img.into_rgba8().into_raw()
        } else {
            image::imageops::resize(
                &img.into_rgba8(),
                TILE,
                TILE,
                image::imageops::FilterType::Triangle,
            )
            .into_raw()
        };
        if rgba.len() != expected_rgba {
            return None;
        }
        export_rgba_frames.push(rgba);
    }

    // Read preview frames (kept as PNG bytes — JS decodes them)
    let mut preview_pngs = Vec::with_capacity(preview_count);
    for _ in 0..preview_count {
        let png_len = read_u32(&mut pos)? as usize;
        if pos + png_len > data.len() { return None; }
        preview_pngs.push(data[pos..pos + png_len].to_vec());
        pos += png_len;
    }

    Some(ParsedCache {
        loop_duration,
        natural_width,
        natural_height,
        export_rgba_frames,
        preview_pngs,
    })
}

/// Save animation data to disk cache. Also populates the persistent export store.
pub fn save_cache(
    slot_id: u32,
    svg_hash: &str,
    loop_duration: f64,
    natural_width: u32,
    natural_height: u32,
    export_png_bytes: &[Vec<u8>],
    preview_png_bytes: &[Vec<u8>],
) -> Result<(), String> {
    let dir = cache_dir().ok_or("no local data dir")?;
    fs::create_dir_all(&dir).map_err(|e| format!("mkdir: {e}"))?;

    // Remove any old cache files for this slot (different hash).
    if let Ok(entries) = fs::read_dir(&dir) {
        let prefix = format!("slot_{slot_id}_");
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with(&prefix) && name_str.ends_with(".bin") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    let path = cache_path(slot_id, svg_hash).ok_or("no cache path")?;

    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(MAGIC);
    buf.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    buf.extend_from_slice(&slot_id.to_le_bytes());
    buf.extend_from_slice(&loop_duration.to_le_bytes());
    buf.extend_from_slice(&natural_width.to_le_bytes());
    buf.extend_from_slice(&natural_height.to_le_bytes());
    buf.extend_from_slice(&(export_png_bytes.len() as u32).to_le_bytes());
    buf.extend_from_slice(&(preview_png_bytes.len() as u32).to_le_bytes());

    for png in export_png_bytes {
        buf.extend_from_slice(&(png.len() as u32).to_le_bytes());
        buf.extend_from_slice(png);
    }
    for png in preview_png_bytes {
        buf.extend_from_slice(&(png.len() as u32).to_le_bytes());
        buf.extend_from_slice(png);
    }

    fs::write(&path, &buf).map_err(|e| format!("write: {e}"))?;

    // Also decode export PNGs to RGBA and populate the persistent store.
    let expected_rgba = (TILE * TILE * 4) as usize;
    let mut rgba_frames = Vec::with_capacity(export_png_bytes.len());
    for (i, png) in export_png_bytes.iter().enumerate() {
        let img = image::load_from_memory(png)
            .map_err(|e| format!("PNG decode export frame {i}: {e}"))?;
        let rgba = if img.width() == TILE && img.height() == TILE {
            img.into_rgba8().into_raw()
        } else {
            image::imageops::resize(
                &img.into_rgba8(),
                TILE,
                TILE,
                image::imageops::FilterType::Triangle,
            )
            .into_raw()
        };
        if rgba.len() != expected_rgba {
            return Err(format!("export frame {i} RGBA size mismatch"));
        }
        rgba_frames.push(rgba);
    }

    staging::set_animated_cursor_slot(AnimatedCursorSlotData {
        slot_id,
        loop_duration,
        frames: rgba_frames,
    });

    Ok(())
}
