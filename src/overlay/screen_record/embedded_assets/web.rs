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

fn dynamic_web_asset_mime(relative: &str) -> Option<&'static str> {
    let file_name = relative.strip_prefix("assets/")?;
    if file_name.is_empty()
        || file_name.contains(['/', '\\'])
        || file_name == "."
        || file_name == ".."
    {
        return None;
    }
    match std::path::Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some("js") => Some("application/javascript"),
        Some("css") => Some("text/css"),
        _ => None,
    }
}

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
        .map(|(_, mime)| *mime)
        .or_else(|| dynamic_web_asset_mime(relative))
        .and_then(|mime| read_asset(relative).map(|bytes| (bytes, mime)))
}

#[cfg(test)]
mod tests {
    use super::dynamic_web_asset_mime;

    #[test]
    fn lazy_script_and_style_chunks_are_authorized() {
        assert_eq!(
            dynamic_web_asset_mime("assets/editor-C0FFEE.js"),
            Some("application/javascript")
        );
        assert_eq!(
            dynamic_web_asset_mime("assets/crop-panel-A11CE.css"),
            Some("text/css")
        );
    }

    #[test]
    fn dynamic_assets_reject_traversal_nested_and_non_web_files() {
        for path in [
            "../assets/editor.js",
            "assets/../editor.js",
            r"assets\editor.js",
            "assets/nested/editor.js",
            "assets/editor.js.map",
            "assets/worker.wasm",
            "assets/secrets.json",
            "assets/",
        ] {
            assert_eq!(dynamic_web_asset_mime(path), None, "{path}");
        }
    }
}
