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
use std::sync::Mutex;

use super::config::AnimatedCursorSlotData;
use super::staging;

const MAGIC: &[u8; 8] = b"SGT_ANIM";
const FORMAT_VERSION: u32 = 2;
const TILE: u32 = 512;
const MAX_FRAMES: usize = 240;
const MAX_CACHE_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
const MAX_CACHE_FILES: usize = 64;
const MAX_CACHE_TREE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
static CACHE_WRITE_LOCK: Mutex<()> = Mutex::new(());

fn cache_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| base.join("screen-goated-toolbox").join("cursor-anim-cache"))
}

fn cache_path(slot_id: u32, svg_hash: &str) -> Option<PathBuf> {
    if slot_id >= super::super::embedded_assets::CURSOR_ATLAS_SLOT_COUNT
        || svg_hash.is_empty()
        || svg_hash.len() > 16
        || !svg_hash
            .bytes()
            .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit())
    {
        return None;
    }
    cache_dir().map(|dir| dir.join(format!("slot_{slot_id}_{svg_hash}.bin")))
}

/// Try to load cached animation data for a slot.
/// On success, populates the persistent ANIMATED_CURSORS store with export
/// frames and returns the preview PNG bytes for JS to reconstruct canvases.
pub fn load_cache(slot_id: u32, svg_hash: &str) -> Option<CacheLoadResult> {
    let path = cache_path(slot_id, svg_hash)?;
    let metadata = fs::metadata(&path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_CACHE_FILE_BYTES {
        return None;
    }
    let data = fs::read(&path).ok()?;
    let result = parse_cache_file(&data, slot_id)?;

    // Populate the persistent export store immediately.
    staging::set_animated_cursor_slot(AnimatedCursorSlotData {
        slot_id,
        loop_duration: result.loop_duration,
        frames: result.export_rgba_frames,
    })
    .ok()?;

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
        if *p + 4 > data.len() {
            return None;
        }
        let v = u32::from_le_bytes(data[*p..*p + 4].try_into().ok()?);
        *p += 4;
        Some(v)
    };
    let read_f64 = |p: &mut usize| -> Option<f64> {
        if *p + 8 > data.len() {
            return None;
        }
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
    if export_count == 0
        || export_count > MAX_FRAMES
        || preview_count > MAX_FRAMES
        || export_count != preview_count
        || !loop_duration.is_finite()
        || loop_duration <= 0.0
        || natural_width == 0
        || natural_height == 0
        || natural_width > 16_384
        || natural_height > 16_384
    {
        return None;
    }

    let expected_rgba = (TILE * TILE * 4) as usize;

    // Read export frames (PNG → decode to RGBA)
    let mut export_rgba_frames = Vec::with_capacity(export_count);
    for _ in 0..export_count {
        let png_len = read_u32(&mut pos)? as usize;
        if png_len == 0 || png_len > MAX_FRAME_BYTES || pos.checked_add(png_len)? > data.len() {
            return None;
        }
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
        if png_len == 0 || png_len > MAX_FRAME_BYTES || pos.checked_add(png_len)? > data.len() {
            return None;
        }
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
    let path = cache_path(slot_id, svg_hash).ok_or("invalid cursor animation cache key")?;
    if !loop_duration.is_finite()
        || loop_duration <= 0.0
        || natural_width == 0
        || natural_height == 0
        || natural_width > 16_384
        || natural_height > 16_384
        || export_png_bytes.is_empty()
        || export_png_bytes.len() > MAX_FRAMES
        || export_png_bytes.len() != preview_png_bytes.len()
        || export_png_bytes
            .iter()
            .chain(preview_png_bytes)
            .any(|frame| frame.is_empty() || frame.len() > MAX_FRAME_BYTES)
    {
        return Err("cursor animation cache data exceeds supported limits".to_string());
    }
    let encoded_bytes = export_png_bytes
        .iter()
        .chain(preview_png_bytes)
        .try_fold(40usize, |total, frame| {
            total.checked_add(4)?.checked_add(frame.len())
        })
        .ok_or("cursor animation cache byte length overflowed")?;
    if encoded_bytes as u64 > MAX_CACHE_FILE_BYTES {
        return Err("cursor animation cache exceeds the 256 MiB limit".to_string());
    }
    let _write_guard = CACHE_WRITE_LOCK.lock().unwrap();
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

    let mut buf: Vec<u8> = Vec::with_capacity(encoded_bytes);
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

    let temp_path = dir.join(format!(
        ".cursor-cache-{}-{}-{}.part",
        std::process::id(),
        slot_id,
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let mut temp = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|e| format!("create cursor cache temp: {e}"))?;
    use std::io::Write as _;
    if let Err(error) = temp.write_all(&buf).and_then(|_| temp.sync_all()) {
        drop(temp);
        let _ = fs::remove_file(&temp_path);
        return Err(format!("write cursor cache temp: {error}"));
    }
    drop(temp);
    let _ = fs::remove_file(&path);
    if let Err(error) = fs::rename(&temp_path, &path) {
        let _ = fs::remove_file(&temp_path);
        return Err(format!("publish cursor cache: {error}"));
    }
    prune_cache_dir(&dir, &path);

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
    })?;

    Ok(())
}

fn prune_cache_dir(dir: &std::path::Path, keep: &std::path::Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            (metadata.is_file()
                && entry.path() != keep
                && entry.file_name().to_string_lossy().starts_with("slot_")
                && entry.file_name().to_string_lossy().ends_with(".bin"))
            .then(|| (metadata.modified().ok(), metadata.len(), entry.path()))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(modified, _, _)| *modified);
    let mut total = files
        .iter()
        .map(|(_, bytes, _)| *bytes)
        .sum::<u64>()
        .saturating_add(fs::metadata(keep).map(|meta| meta.len()).unwrap_or(0));
    let mut count = files.len().saturating_add(1);
    for (_, bytes, path) in files {
        if count <= MAX_CACHE_FILES && total <= MAX_CACHE_TREE_BYTES {
            break;
        }
        if fs::remove_file(path).is_ok() {
            count = count.saturating_sub(1);
            total = total.saturating_sub(bytes);
        }
    }
}

#[cfg(test)]
mod security_tests {
    use super::cache_path;

    #[test]
    fn cache_keys_cannot_escape_the_owned_directory() {
        assert!(cache_path(0, "abc123").is_some());
        for hash in ["../outside", r"..\outside", "hash.bin", "UPPER"] {
            assert!(cache_path(0, hash).is_none());
        }
        assert!(cache_path(u32::MAX, "abc123").is_none());
    }
}
