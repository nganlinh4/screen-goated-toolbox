use super::{EmbeddedAsset, lookup_asset};

macro_rules! asset {
    ($path:literal, $mime:literal) => {
        EmbeddedAsset {
            path: $path,
            mime: $mime,
            bytes: include_bytes!(concat!("../dist/", $path)),
        }
    };
}

pub(in crate::overlay::screen_record) const INDEX_HTML: &[u8] =
    include_bytes!("../dist/index.html");

const WEB_ASSETS: &[EmbeddedAsset] = &[
    asset!("assets/index.js", "application/javascript"),
    asset!("assets/index.css", "text/css"),
    asset!("assets/react-vendor.js", "application/javascript"),
    asset!("assets/vendor.js", "application/javascript"),
    asset!("vite.svg", "image/svg+xml"),
    asset!("tauri.svg", "image/svg+xml"),
    asset!("pointer.svg", "image/svg+xml"),
    asset!("bg-warm-abstract.svg", "image/svg+xml"),
    asset!("bg-cool-abstract.svg", "image/svg+xml"),
    asset!("bg-deep-abstract.svg", "image/svg+xml"),
    asset!("bg-vivid-abstract.svg", "image/svg+xml"),
    asset!("screenshot.png", "image/png"),
];

pub(in crate::overlay::screen_record) fn lookup_packaged_web_asset(
    path: &str,
) -> Option<(&'static [u8], &'static str)> {
    lookup_asset(WEB_ASSETS, path)
}
