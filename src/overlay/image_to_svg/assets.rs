//! Frontend bundle (`image-to-svg-ui/dist/`) inlined into a single page.

const INDEX_HTML: &[u8] = include_bytes!("dist/index.html");
const ASSET_INDEX_JS: &[u8] = include_bytes!("dist/assets/index.js");
const ASSET_INDEX_CSS: &[u8] = include_bytes!("dist/assets/index.css");

pub(super) fn build_inlined_html() -> String {
    let html = String::from_utf8_lossy(INDEX_HTML);
    let css = String::from_utf8_lossy(ASSET_INDEX_CSS);
    let js = String::from_utf8_lossy(ASSET_INDEX_JS);
    let font_css = crate::overlay::html_components::font_manager::get_font_css();

    let mut result = html.to_string();
    result = result.replace(
        r#"<link rel="stylesheet" crossorigin href="/assets/index.css">"#,
        &format!("<style>{font_css}\n{css}</style>"),
    );
    result = result.replace(
        r#"<script type="module" crossorigin src="/assets/index.js"></script>"#,
        &format!("<script type=\"module\">{js}</script>"),
    );
    result
}
