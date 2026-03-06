use std::sync::{Arc, Mutex, OnceLock};

use resvg::usvg::{Options, Tree};
use tiny_skia::{Pixmap, Transform};

use super::super::embedded_assets::{CURSOR_ATLAS_SLOT_COUNT, cursor_atlas_svg};

pub(super) const CURSOR_ATLAS_COLS: u32 = 9;
pub(super) const CURSOR_ATLAS_SLOTS: u32 = CURSOR_ATLAS_SLOT_COUNT;
pub(super) const CURSOR_ATLAS_ROWS: u32 = CURSOR_ATLAS_SLOTS.div_ceil(CURSOR_ATLAS_COLS);
pub(super) const CURSOR_TILE_SIZE: u32 = 512;

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
