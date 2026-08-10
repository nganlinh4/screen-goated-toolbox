//! Installed frontend bundle inlined into a single page.

use anyhow::{Context, Result};

use crate::component_registry::web_assets::{WebAssetComponent, WebAssetPack};

pub(super) fn build_inlined_html() -> Result<(String, WebAssetPack)> {
    let pack = crate::component_registry::web_assets::open(WebAssetComponent::Creation3d)?;
    let html = String::from_utf8(pack.read("index.html")?)
        .context("3D Creation index is not valid UTF-8")?;
    let css = String::from_utf8(pack.read("assets/index.css")?)
        .context("3D Creation stylesheet is not valid UTF-8")?;
    let js = String::from_utf8(pack.read("assets/index.js")?)
        .context("3D Creation script is not valid UTF-8")?;
    let font_css = crate::overlay::html_components::font_manager::get_font_css();

    let mut result = html;
    result = result.replace(
        r#"<link rel="stylesheet" crossorigin href="/assets/index.css">"#,
        &format!("<style>{font_css}\n{css}</style>"),
    );
    result = result.replace(
        r#"<script type="module" crossorigin src="/assets/index.js"></script>"#,
        &format!("<script type=\"module\">{js}</script>"),
    );
    Ok((result, pack))
}
