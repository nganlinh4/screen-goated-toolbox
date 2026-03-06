use super::{EmbeddedAsset, lookup_asset};

macro_rules! svg_asset {
    ($path:expr) => {
        EmbeddedAsset {
            path: $path,
            mime: "image/svg+xml",
            bytes: include_bytes!(concat!("../dist/", $path)),
        }
    };
}

macro_rules! cursor_assets {
    ($($slug:literal),* $(,)?) => {
        &[
            $(
                svg_asset!(concat!("cursor-default-", $slug, ".svg")),
                svg_asset!(concat!("cursor-text-", $slug, ".svg")),
                svg_asset!(concat!("cursor-pointer-", $slug, ".svg")),
                svg_asset!(concat!("cursor-openhand-", $slug, ".svg")),
                svg_asset!(concat!("cursor-closehand-", $slug, ".svg")),
                svg_asset!(concat!("cursor-wait-", $slug, ".svg")),
                svg_asset!(concat!("cursor-appstarting-", $slug, ".svg")),
                svg_asset!(concat!("cursor-crosshair-", $slug, ".svg")),
                svg_asset!(concat!("cursor-resize-ns-", $slug, ".svg")),
                svg_asset!(concat!("cursor-resize-we-", $slug, ".svg")),
                svg_asset!(concat!("cursor-resize-nwse-", $slug, ".svg")),
                svg_asset!(concat!("cursor-resize-nesw-", $slug, ".svg")),
            )*
        ]
    };
}

// Keep this order aligned with the frontend cursor atlas mapping.
const CURSOR_ATLAS_ASSETS: &[EmbeddedAsset] = cursor_assets!(
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
    CURSOR_ATLAS_ASSETS.len() as u32;

pub(in crate::overlay::screen_record) fn cursor_atlas_svg(slot: u32) -> Option<&'static [u8]> {
    CURSOR_ATLAS_ASSETS
        .get(slot as usize)
        .map(|asset| asset.bytes)
}

pub(in crate::overlay::screen_record) fn lookup_packaged_cursor_asset(
    path: &str,
) -> Option<(&'static [u8], &'static str)> {
    lookup_asset(CURSOR_ATLAS_ASSETS, path)
}
