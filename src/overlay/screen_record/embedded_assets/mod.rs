mod cursors;
mod web;

struct EmbeddedAsset {
    path: &'static str,
    mime: &'static str,
    bytes: &'static [u8],
}

fn lookup_asset(
    assets: &'static [EmbeddedAsset],
    path: &str,
) -> Option<(&'static [u8], &'static str)> {
    let rel = path.strip_prefix('/').unwrap_or(path);
    assets
        .iter()
        .find(|asset| asset.path == rel)
        .map(|asset| (asset.bytes, asset.mime))
}

pub(in crate::overlay::screen_record) use cursors::{
    CURSOR_ATLAS_SLOT_COUNT, cursor_atlas_svg, lookup_packaged_cursor_asset,
};
pub(in crate::overlay::screen_record) use web::{INDEX_HTML, lookup_packaged_web_asset};

pub(in crate::overlay::screen_record) fn lookup_packaged_asset(
    path: &str,
) -> Option<(&'static [u8], &'static str)> {
    lookup_packaged_web_asset(path).or_else(|| lookup_packaged_cursor_asset(path))
}
