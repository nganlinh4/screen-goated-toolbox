use std::sync::OnceLock;

use super::read_asset;

macro_rules! cursor_assets {
    ($($slug:literal),* $(,)?) => {
        &[
            $(
                concat!("cursor-default-", $slug, ".svg"),
                concat!("cursor-text-", $slug, ".svg"),
                concat!("cursor-pointer-", $slug, ".svg"),
                concat!("cursor-openhand-", $slug, ".svg"),
                concat!("cursor-closehand-", $slug, ".svg"),
                concat!("cursor-wait-", $slug, ".svg"),
                concat!("cursor-appstarting-", $slug, ".svg"),
                concat!("cursor-crosshair-", $slug, ".svg"),
                concat!("cursor-resize-ns-", $slug, ".svg"),
                concat!("cursor-resize-we-", $slug, ".svg"),
                concat!("cursor-resize-nwse-", $slug, ".svg"),
                concat!("cursor-resize-nesw-", $slug, ".svg"),
            )*
        ]
    };
}

// Keep this order aligned with the frontend cursor atlas mapping.
const CURSOR_ATLAS_PATHS: &[&str] = cursor_assets!(
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
);

pub(in crate::overlay::screen_record) const CURSOR_ATLAS_SLOT_COUNT: u32 =
    CURSOR_ATLAS_PATHS.len() as u32;

static CURSOR_ATLAS_ASSETS: OnceLock<Option<Vec<Vec<u8>>>> = OnceLock::new();

fn cursor_assets() -> Option<&'static [Vec<u8>]> {
    CURSOR_ATLAS_ASSETS
        .get_or_init(|| {
            CURSOR_ATLAS_PATHS
                .iter()
                .map(|path| read_asset(path))
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
    CURSOR_ATLAS_PATHS
        .iter()
        .position(|candidate| *candidate == relative)
        .and_then(|index| cursor_assets()?.get(index).cloned())
        .map(|bytes| (bytes, "image/svg+xml"))
}
