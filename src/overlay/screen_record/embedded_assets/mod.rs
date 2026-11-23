mod cursors;
mod web;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static PACKAGED_ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();

fn packaged_root() -> Option<&'static Path> {
    PACKAGED_ROOT
        .get_or_init(|| selected_packaged_root(std::env::var_os("SGT_RECORDER_WEB_ROOT")))
        .as_deref()
}

fn selected_packaged_root(configured: Option<std::ffi::OsString>) -> Option<PathBuf> {
    configured
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn read_asset(path: &str) -> Option<Vec<u8>> {
    let relative = Path::new(path.strip_prefix('/').unwrap_or(path));
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
    {
        return None;
    }
    std::fs::read(packaged_root()?.join(relative)).ok()
}

pub(in crate::overlay::screen_record) use cursors::{
    CURSOR_ATLAS_SLOT_COUNT, CURSOR_PACKS, cursor_atlas_slot, cursor_atlas_svg,
    lookup_packaged_cursor_asset,
};
pub(in crate::overlay::screen_record) use web::{index_html, lookup_packaged_web_asset};

pub(in crate::overlay::screen_record) fn lookup_packaged_asset(
    path: &str,
) -> Option<(Vec<u8>, &'static str)> {
    lookup_packaged_web_asset(path).or_else(|| lookup_packaged_cursor_asset(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn web_assets_use_only_the_selected_root() {
        assert_eq!(selected_packaged_root(None), None);
        assert_eq!(selected_packaged_root(Some("".into())), None);
        let configured = PathBuf::from("selected-web-root");
        assert_eq!(
            selected_packaged_root(Some(configured.clone().into())),
            Some(configured)
        );
    }
}
