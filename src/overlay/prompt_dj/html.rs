use anyhow::{Context, Result};

use crate::component_registry::web_assets::{WebAssetComponent, WebAssetPack};

/// Build a self-contained HTML with CSS/JS/font inlined.
/// Served via the shared font server so all WebViews share one browser process.
pub fn build_inlined_html() -> Result<(String, WebAssetPack)> {
    let pack = crate::component_registry::web_assets::open(WebAssetComponent::PromptDj)?;
    let html =
        String::from_utf8(pack.read("index.html")?).context("PromptDJ index is not valid UTF-8")?;
    let css = String::from_utf8(pack.read("assets/index.css")?)
        .context("PromptDJ stylesheet is not valid UTF-8")?;
    let js = String::from_utf8(pack.read("assets/index.js")?)
        .context("PromptDJ script is not valid UTF-8")?;
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
