use std::sync::OnceLock;

use super::read_asset;

/// Cursor packs in atlas order; "screenstudio" is the default pack. Keep this
/// order aligned with the frontend cursor atlas mapping.
pub(in crate::overlay::screen_record) const CURSOR_PACKS: [&str; 12] = [
    "screenstudio",
    "macos26",
    "sgtcute",
    "sgtcool",
    "sgtai",
    "sgtpixel",
    "jepriwin11",
    "sgtwatermelon",
    "sgtfastfood",
    "sgtveggie",
    "sgtvietnam",
    "sgtkorea",
];

/// Cursor types in per-pack slot order. Slot = pack index * types + type index.
pub(in crate::overlay::screen_record) const CURSOR_TYPES: [&str; 12] = [
    "default",
    "text",
    "pointer",
    "openhand",
    "closehand",
    "wait",
    "appstarting",
    "crosshair",
    "resize-ns",
    "resize-we",
    "resize-nwse",
    "resize-nesw",
];

pub(in crate::overlay::screen_record) const CURSOR_ATLAS_SLOT_COUNT: u32 =
    (CURSOR_PACKS.len() * CURSOR_TYPES.len()) as u32;

/// The `<type>-<pack>` atlas slot, or `None` for an unknown name.
pub(in crate::overlay::screen_record) fn cursor_atlas_slot(name: &str) -> Option<u32> {
    let (kind, pack) = name.rsplit_once('-')?;
    let pack = CURSOR_PACKS
        .iter()
        .position(|candidate| *candidate == pack)?;
    let kind = CURSOR_TYPES
        .iter()
        .position(|candidate| *candidate == kind)?;
    Some((pack * CURSOR_TYPES.len() + kind) as u32)
}

fn cursor_atlas_path(slot: usize) -> String {
    let pack = CURSOR_PACKS[slot / CURSOR_TYPES.len()];
    let kind = CURSOR_TYPES[slot % CURSOR_TYPES.len()];
    format!("cursor-{kind}-{pack}.svg")
}

static CURSOR_ATLAS_ASSETS: OnceLock<Option<Vec<Vec<u8>>>> = OnceLock::new();

fn cursor_assets() -> Option<&'static [Vec<u8>]> {
    CURSOR_ATLAS_ASSETS
        .get_or_init(|| {
            (0..CURSOR_ATLAS_SLOT_COUNT as usize)
                .map(|slot| read_asset(&cursor_atlas_path(slot)))
                .collect()
        })
        .as_deref()
}

pub(in crate::overlay::screen_record) fn cursor_atlas_svg(slot: u32) -> Option<&'static [u8]> {
    cursor_assets()?.get(slot as usize).map(Vec::as_slice)
}

pub(in crate::overlay::screen_record) fn lookup_packaged_cursor_asset(
    path: &str,
) -> Option<(Vec<u8>, &'static str)> {
    let relative = path.strip_prefix('/').unwrap_or(path);
    let name = relative.strip_prefix("cursor-")?.strip_suffix(".svg")?;
    cursor_atlas_slot(name)
        .and_then(|slot| cursor_assets()?.get(slot as usize).cloned())
        .map(|bytes| (bytes, "image/svg+xml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_atlas_slot_round_trips_through_its_file_name() {
        for slot in 0..CURSOR_ATLAS_SLOT_COUNT as usize {
            let path = cursor_atlas_path(slot);
            let name = path
                .strip_prefix("cursor-")
                .and_then(|rest| rest.strip_suffix(".svg"))
                .unwrap();
            assert_eq!(cursor_atlas_slot(name), Some(slot as u32), "{path}");
        }
        assert_eq!(cursor_atlas_path(0), "cursor-default-screenstudio.svg");
        assert_eq!(cursor_atlas_slot("resize-nesw-sgtkorea"), Some(143));
        assert_eq!(cursor_atlas_slot("default-unknownpack"), None);
        assert_eq!(cursor_atlas_slot("default"), None);
    }
}
