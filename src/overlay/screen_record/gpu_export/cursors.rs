use std::sync::{Arc, Mutex, OnceLock};

use resvg::usvg::{Options, Tree};
use tiny_skia::{Pixmap, Transform};

use super::super::embedded_assets::{CURSOR_ATLAS_SLOT_COUNT, cursor_atlas_svg};

pub(super) const CURSOR_ATLAS_COLS: u32 = 9;
pub(super) const CURSOR_ATLAS_SLOTS: u32 = CURSOR_ATLAS_SLOT_COUNT;
pub(super) const CURSOR_ATLAS_ROWS: u32 = CURSOR_ATLAS_SLOTS.div_ceil(CURSOR_ATLAS_COLS);
pub(super) const CURSOR_TILE_SIZE: u32 = 512;
/// Mip levels per tile, down to 16 px. The shadow blur reads coarser levels
/// for wide kernels; stopping here keeps a level's bilinear footprint inside
/// the tile's transparent padding, so neighbouring tiles never bleed in.
pub(super) const CURSOR_MIP_LEVELS: u32 = 6;

/// wgpu's default `max_texture_dimension_2d`; the atlas must fit inside it.
const MAX_ATLAS_SIDE: u32 = 8192;
const _: () = assert!(
    CURSOR_ATLAS_COLS * CURSOR_TILE_SIZE <= MAX_ATLAS_SIDE
        && CURSOR_ATLAS_ROWS * CURSOR_TILE_SIZE <= MAX_ATLAS_SIDE,
    "cursor atlas exceeds the default texture limit; add columns or use an array texture"
);

/// A tile write must stay inside the atlas and cover exactly one tile.
pub(super) fn is_uploadable_cursor_tile(slot: u32, rgba_len: usize) -> bool {
    slot < CURSOR_ATLAS_SLOTS && rgba_len == (CURSOR_TILE_SIZE * CURSOR_TILE_SIZE * 4) as usize
}

type TileCache = Mutex<Vec<Option<Arc<Vec<u8>>>>>;

static CURSOR_TILE_CACHE: OnceLock<TileCache> = OnceLock::new();

fn cursor_tile_cache() -> &'static TileCache {
    CURSOR_TILE_CACHE.get_or_init(|| Mutex::new(vec![None; CURSOR_ATLAS_SLOTS as usize]))
}

fn render_cursor_tile_rgba(slot: u32) -> Option<Vec<u8>> {
    if slot >= CURSOR_ATLAS_SLOTS {
        return None;
    }

    let tile_size = CURSOR_TILE_SIZE;
    let center = tile_size as f32 / 2.0;
    let mut tile = Pixmap::new(tile_size, tile_size).unwrap();
    let target = tile_size as f32;

    let opt = Options::default();
    let tree = Tree::from_data(cursor_atlas_svg(slot)?, &opt).ok()?;
    let svg_size = tree.size();
    let svg_w = svg_size.width().max(1.0);
    let svg_h = svg_size.height().max(1.0);
    let base_scale = target / svg_w.max(svg_h);
    let hotspot_px_x = (svg_w * 0.5) * base_scale;
    let hotspot_px_y = (svg_h * 0.5) * base_scale;
    let x = center - hotspot_px_x;
    let y = center - hotspot_px_y;
    let ts = Transform::from_translate(x, y).pre_scale(base_scale, base_scale);
    resvg::render(&tree, ts, &mut tile.as_mut());

    Some(tile.data().to_vec())
}

pub(super) fn get_or_render_cursor_tile(slot: u32) -> Option<Arc<Vec<u8>>> {
    if slot >= CURSOR_ATLAS_SLOTS {
        return None;
    }

    {
        let cache = cursor_tile_cache().lock().unwrap();
        if let Some(bytes) = &cache[slot as usize] {
            return Some(Arc::clone(bytes));
        }
    }

    let rendered = Arc::new(render_cursor_tile_rgba(slot)?);
    let mut cache = cursor_tile_cache().lock().unwrap();
    if let Some(bytes) = &cache[slot as usize] {
        Some(Arc::clone(bytes))
    } else {
        cache[slot as usize] = Some(Arc::clone(&rendered));
        Some(rendered)
    }
}

/// Box-filters a tile into each smaller mip level (level 1 first). Alpha is
/// averaged exactly, which is all the shadow reads from these levels.
pub(super) fn cursor_tile_mip_levels(rgba: &[u8]) -> Vec<Vec<u8>> {
    let mut levels: Vec<Vec<u8>> = Vec::with_capacity(CURSOR_MIP_LEVELS as usize - 1);
    let mut side = CURSOR_TILE_SIZE as usize;
    for _ in 1..CURSOR_MIP_LEVELS {
        let source = levels.last().map_or(rgba, Vec::as_slice);
        let level = halve_square_rgba(source, side);
        levels.push(level);
        side /= 2;
    }
    levels
}

fn halve_square_rgba(source: &[u8], side: usize) -> Vec<u8> {
    let half = side / 2;
    let mut level = vec![0u8; half * half * 4];
    for y in 0..half {
        for x in 0..half {
            let top = (2 * y * side + 2 * x) * 4;
            let bottom = top + side * 4;
            for channel in 0..4 {
                let sum = u32::from(source[top + channel])
                    + u32::from(source[top + 4 + channel])
                    + u32::from(source[bottom + channel])
                    + u32::from(source[bottom + 4 + channel]);
                level[(y * half + x) * 4 + channel] = ((sum + 2) / 4) as u8;
            }
        }
    }
    level
}

pub(super) fn dedupe_valid_slots(slots: &[u32]) -> Vec<u32> {
    let mut seen = [false; CURSOR_ATLAS_SLOTS as usize];
    let mut out = Vec::with_capacity(slots.len().max(1));
    for slot in slots {
        let idx = *slot as usize;
        if idx >= CURSOR_ATLAS_SLOTS as usize || seen[idx] {
            continue;
        }
        seen[idx] = true;
        out.push(*slot);
    }
    if out.is_empty() {
        out.push(0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_in_atlas_whole_tiles_are_uploaded() {
        let tile = (CURSOR_TILE_SIZE * CURSOR_TILE_SIZE * 4) as usize;
        assert!(is_uploadable_cursor_tile(CURSOR_ATLAS_SLOTS - 1, tile));
        assert!(!is_uploadable_cursor_tile(CURSOR_ATLAS_SLOTS, tile));
        assert!(!is_uploadable_cursor_tile(255, tile));
        assert!(!is_uploadable_cursor_tile(0, tile - 4));
    }

    #[test]
    fn mip_levels_halve_the_tile_and_keep_its_coverage() {
        let side = CURSOR_TILE_SIZE as usize;
        // The left half of the tile is opaque white, the right half clear.
        let mut tile = vec![0u8; side * side * 4];
        for y in 0..side {
            for x in 0..side / 2 {
                tile[(y * side + x) * 4..][..4].copy_from_slice(&[255; 4]);
            }
        }
        let levels = cursor_tile_mip_levels(&tile);
        assert_eq!(levels.len(), CURSOR_MIP_LEVELS as usize - 1);
        for (index, level) in levels.iter().enumerate() {
            let level_side = side >> (index + 1);
            assert_eq!(level.len(), level_side * level_side * 4);
            let alpha: u64 = level.iter().skip(3).step_by(4).map(|&a| u64::from(a)).sum();
            assert_eq!(alpha, 255 * (level_side * level_side / 2) as u64);
        }
    }
}
