use super::read_asset;

const WEB_ASSETS: &[(&str, &str)] = &[
    ("assets/index.js", "application/javascript"),
    ("assets/index.css", "text/css"),
    ("assets/vendor.js", "application/javascript"),
    // Rolldown (Vite 8) emits a separate runtime chunk that index.js imports;
    // without serving it the whole bundle fails to load and the WebView is blank.
    ("assets/rolldown-runtime.js", "application/javascript"),
    ("bg-warm-abstract.jpg", "image/jpeg"),
    ("bg-cool-abstract.jpg", "image/jpeg"),
    ("bg-deep-abstract.jpg", "image/jpeg"),
    ("bg-vivid-abstract.jpg", "image/jpeg"),
    ("bg-macos-tahoe.jpg", "image/jpeg"),
    ("bg-gdrive-2.jpg", "image/jpeg"),
    ("bg-gdrive-3.jpg", "image/jpeg"),
    ("bg-mojave-dunes.jpg", "image/jpeg"),
    ("bg-catalina.jpg", "image/jpeg"),
    ("bg-big-sur.jpg", "image/jpeg"),
    ("bg-el-capitan.jpg", "image/jpeg"),
    ("bg-beach-aerial.jpg", "image/jpeg"),
    ("bg-sierra-sunset.jpg", "image/jpeg"),
    ("bg-windows-11-3d.jpg", "image/jpeg"),
    ("bg-cerro-torre.jpg", "image/jpeg"),
    ("bg-ipados-orange.jpg", "image/jpeg"),
    ("bg-ipados-blue.jpg", "image/jpeg"),
    ("bg-blue-waves.jpg", "image/jpeg"),
    ("bg-windows-xp.jpg", "image/jpeg"),
    ("bg-antelope-canyon.jpg", "image/jpeg"),
    ("bg-windows-7.jpg", "image/jpeg"),
    ("bg-windows-11-colorful.jpg", "image/jpeg"),
    ("bg-big-sur-iridescence.jpg", "image/jpeg"),
    ("bg-landscape-rocks.jpg", "image/jpeg"),
    ("bg-lake-mountains.jpg", "image/jpeg"),
    ("bg-big-sur-rocks.jpg", "image/jpeg"),
    ("bg-big-sur-waves.jpg", "image/jpeg"),
    ("bg-sierra-glacier.jpg", "image/jpeg"),
    ("bg-monterey-dark.jpg", "image/jpeg"),
];

pub(in crate::overlay::screen_record) fn index_html() -> Option<Vec<u8>> {
    read_asset("index.html")
}

pub(in crate::overlay::screen_record) fn lookup_packaged_web_asset(
    path: &str,
) -> Option<(Vec<u8>, &'static str)> {
    let relative = path.strip_prefix('/').unwrap_or(path);
    WEB_ASSETS
        .iter()
        .find(|(candidate, _)| *candidate == relative)
        .and_then(|(_, mime)| read_asset(relative).map(|bytes| (bytes, *mime)))
}
