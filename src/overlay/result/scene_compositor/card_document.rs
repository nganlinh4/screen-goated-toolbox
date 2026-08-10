use std::sync::LazyLock;

use super::html::DOCUMENT;

static COMPOSITOR_DOCUMENT: LazyLock<String> = LazyLock::new(build_compositor_document);

pub(super) fn compositor_document(isolated_origin: &str) -> String {
    COMPOSITOR_DOCUMENT.replace(
        "__SGT_ISOLATED_ORIGIN_JSON__",
        &serde_json::to_string(isolated_origin).expect("isolated origin must serialize"),
    )
}

pub(super) fn with_fit(mut html: String) -> String {
    let script = crate::overlay::result::markdown_view::fit::runtime_fit_script();
    let injected = format!(
        "<script>window.__SGT_STREAMING__=false;window.__SGT_RUN_FIT__=function(streaming){{{script}}};</script>"
    );
    if let Some(position) = html.to_ascii_lowercase().rfind("</body>") {
        html.insert_str(position, &injected);
    } else {
        html.push_str(&injected);
    }
    html
}

fn build_compositor_document() -> String {
    let card_css = format!(
        r#"
:host{{display:block;width:100%;height:100%;overflow:hidden}}
.result-body,.result-body *{{font-family:'Google Sans Flex' !important}}
.result-body{{width:100%;height:100%;min-height:0;overflow:hidden}}
.result-body[data-sgt-mode="refining"]{{display:flex;align-items:center;justify-content:center;
text-align:center;padding:12px;font-style:italic;color:#aaa;font-size:16px}}
{}
{}
"#,
        crate::overlay::result::markdown_view::css::MARKDOWN_CSS
            .replace(":root", ":host")
            .replace("body", ".result-body"),
        crate::overlay::html_components::grid_js::get_css(),
    );
    let card_css_json = serde_json::to_string(&card_css).expect("card CSS must serialize");
    let (grid_css_url, grid_js_url) = crate::overlay::html_components::grid_js::get_lib_urls();
    let direct_runtime = include_str!("direct_runtime.js")
        .replace("__SGT_GRID_CSS_URL__", grid_css_url)
        .replace("__SGT_GRID_JS_URL__", grid_js_url);

    DOCUMENT
        .replace("__SGT_FONT_FACE__", &super::font::face_css("/font.ttf"))
        .replace("__SGT_CARD_CSS_JSON__", &card_css_json)
        .replace("__SGT_DIRECT_RUNTIME__", &direct_runtime)
        .replace(
            "__SGT_SETTLED_REVEAL_RUNTIME__",
            include_str!("settled_reveal_runtime.js"),
        )
        .replace(
            "__SGT_RENDERER_BOOTSTRAP__",
            include_str!("renderer_bootstrap.js"),
        )
        .replace(
            "__SGT_FIT_RUNTIME__",
            &crate::overlay::result::markdown_view::fit::runtime_fit_script(),
        )
}

#[cfg(test)]
mod tests {
    use super::compositor_document;

    #[test]
    fn compositor_document_owns_one_font_and_one_shared_card_runtime() {
        let document = compositor_document("http://127.0.0.1:32123");

        assert!(document.contains("Google Sans Flex"));
        assert!(document.contains("window.__SGT_CREATE_DIRECT_RUNTIME__"));
        assert!(document.contains("window.__SGT_RUN_FIT__"));
        assert!(document.contains("attachShadow"));
        assert!(document.contains("src:url('/font.ttf')"));
        assert!(!document.contains("Segoe UI"));
        assert!(!document.contains("__SGT_FONT_FACE__"));
        assert!(!document.contains("__SGT_CARD_CSS_JSON__"));
        assert!(!document.contains("__SGT_DIRECT_RUNTIME__"));
        assert!(!document.contains("__SGT_SETTLED_REVEAL_RUNTIME__"));
        assert!(!document.contains("__SGT_FIT_RUNTIME__"));
        assert!(!document.contains("__SGT_RENDERER_BOOTSTRAP__"));
        assert!(!document.contains("__SGT_ISOLATED_ORIGIN_JSON__"));
    }

    #[test]
    fn ordinary_card_css_is_scoped_to_each_shadow_root() {
        let document = compositor_document("http://127.0.0.1:32123");

        assert!(document.contains(".result-body"));
        assert!(document.contains(":host"));
        assert!(document.contains(".result-body > *:first-child"));
    }

    #[test]
    fn non_streaming_content_is_revealed_only_after_its_final_fit() {
        let document = compositor_document("http://127.0.0.1:32123");

        assert!(document.contains("type === 'finalize' && !entry.streamingEnabled"));
        assert!(document.contains("prepareSettledReveal(entry, entry.contentRevision)"));
        assert!(document.contains("revealSettledContent(entry, completed.contentRevision)"));
        assert!(document.contains("setSettledSurfaceVisibility(entry, false)"));
        assert!(document.contains("entry.contentRevision !== contentRevision"));
        assert!(document.contains("if (!String(entry.body || '').trim()) return false"));
        assert!(document.contains("settle_before_reveal: settleBeforeReveal"));
        assert!(document.contains("settleBeforeReveal: Boolean(message.settle_before_reveal)"));
        assert!(document.contains("finishBodyPresentation()"));
        assert!(document.contains("body.style.setProperty('animation', 'none', 'important')"));
        assert!(document.contains("body.style.setProperty('opacity', '1', 'important')"));
        assert!(
            document.contains("body.style.setProperty('transform', 'translateY(0)', 'important')")
        );
    }
}
