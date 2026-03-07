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
    asset!("bg-warm-abstract.jpg", "image/jpeg"),
    asset!("bg-cool-abstract.jpg", "image/jpeg"),
    asset!("bg-deep-abstract.jpg", "image/jpeg"),
    asset!("bg-vivid-abstract.jpg", "image/jpeg"),
    asset!("bg-macos-tahoe.jpg", "image/jpeg"),
    asset!("bg-gdrive-2.jpg", "image/jpeg"),
    asset!("bg-gdrive-3.jpg", "image/jpeg"),
    asset!("bg-mojave-dunes.jpg", "image/jpeg"),
    asset!("bg-catalina.jpg", "image/jpeg"),
    asset!("bg-big-sur.jpg", "image/jpeg"),
    asset!("bg-el-capitan.jpg", "image/jpeg"),
    asset!("bg-beach-aerial.jpg", "image/jpeg"),
    asset!("bg-sierra-sunset.jpg", "image/jpeg"),
    asset!("bg-windows-11-3d.jpg", "image/jpeg"),
    asset!("bg-cerro-torre.jpg", "image/jpeg"),
];

pub(in crate::overlay::screen_record) fn lookup_packaged_web_asset(
    path: &str,
) -> Option<(&'static [u8], &'static str)> {
    lookup_asset(WEB_ASSETS, path)
}
