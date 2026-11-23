//! HTML manipulation utilities for markdown view

/// Minimal HTML escaping for text content
pub fn escape_html_text(text: &str) -> String {
    crate::overlay::utils::escape_html(text)
}

pub use super::html_document::normalize_html_content;

/// Inject localStorage/sessionStorage polyfill into HTML for WebView2 compatibility
/// WebView2's with_html() runs in a sandboxed context that denies storage access
/// This provides an in-memory fallback so scripts don't crash
pub fn inject_storage_polyfill(html: &str) -> String {
    // Polyfill script that provides in-memory storage when real storage is blocked
    let polyfill = r#"<script>
(function() {
    // Check if localStorage is accessible
    try {
        var test = '__storage_test__';
        localStorage.setItem(test, test);
        localStorage.removeItem(test);
        // localStorage works, no polyfill needed
    } catch (e) {
        // localStorage blocked, create in-memory polyfill
        var memoryStorage = {};
        var createStorage = function() {
            return {
                _data: {},
                length: 0,
                getItem: function(key) { return this._data.hasOwnProperty(key) ? this._data[key] : null; },
                setItem: function(key, value) { this._data[key] = String(value); this.length = Object.keys(this._data).length; },
                removeItem: function(key) { delete this._data[key]; this.length = Object.keys(this._data).length; },
                clear: function() { this._data = {}; this.length = 0; },
                key: function(i) { var keys = Object.keys(this._data); return keys[i] || null; }
            };
        };
        try {
            Object.defineProperty(window, 'localStorage', { value: createStorage(), writable: false });
            Object.defineProperty(window, 'sessionStorage', { value: createStorage(), writable: false });
        } catch (e2) {
            // If defineProperty fails, try direct assignment
            window.localStorage = createStorage();
            window.sessionStorage = createStorage();
        }
    }
})();
</script>"#;

    // Before any authored script: right after the <head>, <html> or doctype opener.
    let position = open_tag_end(html, "head")
        .or_else(|| open_tag_end(html, "html"))
        .or_else(|| leading_doctype_end(html))
        .unwrap_or(0);
    insert_at(html, position, polyfill)
}

/// Inject Grid.js into raw HTML if tables are present
pub fn inject_gridjs(html: &str) -> String {
    if !html.contains("<table") {
        return html.to_string();
    }

    let gridjs_head = format!(
        r#"<style>{}{}</style><script>{}</script>"#,
        crate::overlay::html_components::grid_js::get_lib_css(),
        crate::overlay::html_components::grid_js::get_css(),
        crate::overlay::html_components::grid_js::get_lib_js(),
    );
    let gridjs_body = format!(
        r#"<script>{}</script>"#,
        crate::overlay::html_components::grid_js::get_init_script()
    );

    let mut result = insert_at(html, head_insertion_point(html), &gridjs_head);

    // Inject init script into <body>
    let lower_updated = result.to_ascii_lowercase();
    if let Some(pos) = lower_updated.find("</body>") {
        result.insert_str(pos, &gridjs_body);
    } else {
        result.push_str(&gridjs_body);
    }

    result
}

/// Inject CSS to hide scrollbars while preserving scrolling functionality
pub fn inject_scrollbar_css(html: &str) -> String {
    let css = "<style>::-webkit-scrollbar { display: none; }</style>";
    insert_at(html, head_insertion_point(html), css)
}

/// Remove only browser-default page gutters for authored result documents.
/// `:where` keeps zero specificity, so any authored margin or sizing still wins.
pub fn inject_result_surface_css(html: &str) -> String {
    let css = r#"<style data-sgt-result-surface="1">
:where(html, body) { width: 100%; height: 100%; margin: 0; }
:where(body) { box-sizing: border-box; }
</style>"#;
    insert_at(html, head_insertion_point(html), css)
}

/// Inject runtime diagnostics that report suspicious blank render states via IPC.
pub fn inject_render_diagnostics(
    html: &str,
    source_len: usize,
    source_trimmed_len: usize,
    render_mode: &str,
) -> String {
    let render_mode_json =
        serde_json::to_string(render_mode).unwrap_or_else(|_| "\"unknown\"".to_string());
    let script = format!(
        r#"<script data-sgt-render-diag="1">
(function() {{
    const defaultSourceTextLen = {source_len};
    const defaultSourceTrimmedLen = {source_trimmed_len};
    const renderMode = {render_mode_json};

    function report(meta) {{
        try {{
            const body = document.body;
            const renderedText = body ? ((body.innerText || body.textContent || '').trim()) : '';
            const renderedTextLen = renderedText.length;
            const bodyHtmlLen = body ? body.innerHTML.length : 0;
            const bodyChildCount = body ? body.children.length : 0;
            const hasRenderableMedia = !!document.querySelector('img, svg, canvas, video, audio, iframe, table');
            const sourceTextLen = Number.isFinite(meta && meta.sourceTextLen)
                ? meta.sourceTextLen
                : defaultSourceTextLen;
            const sourceTrimmedLen = Number.isFinite(meta && meta.sourceTrimmedLen)
                ? meta.sourceTrimmedLen
                : defaultSourceTrimmedLen;
            const phase = (meta && meta.phase) ? meta.phase : 'page_load';
            const reason = (meta && meta.reason)
                ? meta.reason
                : (sourceTrimmedLen === 0 ? 'blank_source' : 'blank_render');
            const shouldReport = sourceTrimmedLen === 0
                || (sourceTrimmedLen > 0 && renderedTextLen === 0 && !hasRenderableMedia);

            if (!shouldReport || !window.ipc || typeof window.ipc.postMessage !== 'function') {{
                return;
            }}

            const cacheKey = [
                phase,
                reason,
                renderMode,
                sourceTrimmedLen,
                renderedTextLen,
                bodyChildCount,
                bodyHtmlLen
            ].join(':');
            window.__SGT_REPORTED_RENDER_DIAGNOSTICS__ =
                window.__SGT_REPORTED_RENDER_DIAGNOSTICS__ || {{}};
            if (window.__SGT_REPORTED_RENDER_DIAGNOSTICS__[cacheKey]) {{
                return;
            }}
            window.__SGT_REPORTED_RENDER_DIAGNOSTICS__[cacheKey] = true;

            window.ipc.postMessage(JSON.stringify({{
                action: 'render_diagnostics',
                phase,
                reason,
                renderMode,
                sourceTextLen,
                sourceTrimmedLen,
                renderedTextLen,
                bodyHtmlLen,
                bodyChildCount,
                hasRenderableMedia,
                readyState: document.readyState,
                url: location.href
            }}));
        }} catch (err) {{
            if (window.ipc && typeof window.ipc.postMessage === 'function') {{
                window.ipc.postMessage(JSON.stringify({{
                    action: 'render_diagnostics',
                    phase: (meta && meta.phase) ? meta.phase : 'page_load',
                    reason: 'diagnostic_script_error',
                    renderMode,
                    sourceTextLen: defaultSourceTextLen,
                    sourceTrimmedLen: defaultSourceTrimmedLen,
                    error: err && err.message ? err.message : String(err)
                }}));
            }}
        }}
    }}

    window.__SGT_REPORT_RENDER_DIAGNOSTICS__ = report;

    document.addEventListener('DOMContentLoaded', function() {{
        requestAnimationFrame(function() {{
            report({{ phase: 'dom_content_loaded' }});
        }});
    }});

    window.addEventListener('load', function() {{
        setTimeout(function() {{
            report({{ phase: 'window_load' }});
        }}, 0);
    }});
}})();
</script>"#
    );
    insert_at(html, head_insertion_point(html), &script)
}

/// Where head content belongs: before `</head>`, else before the `<body>` tag
/// (with or without attributes), else after the `<html>` or doctype opener.
/// Never ahead of a doctype, which would switch the page into quirks mode.
fn head_insertion_point(html: &str) -> usize {
    let lower = html.to_ascii_lowercase();
    lower
        .find("</head>")
        .or_else(|| open_tag_start(&lower, "body"))
        .or_else(|| open_tag_end(html, "html"))
        .or_else(|| leading_doctype_end(html))
        .unwrap_or(0)
}

/// End of a leading, well-formed doctype. A quoted or malformed doctype can
/// end elsewhere for an HTML parser, so it yields None (insert at byte 0).
fn leading_doctype_end(html: &str) -> Option<usize> {
    let start = html.len() - html.trim_start_matches('\u{feff}').trim_start().len();
    let rest = &html[start..];
    if !rest.get(..9)?.eq_ignore_ascii_case("<!doctype") {
        return None;
    }
    let end = rest.find('>')?;
    let declaration = &rest[9..end];
    (!declaration.contains(['"', '\'', '<'])).then_some(start + end + 1)
}

/// Byte offset of `<name` when followed by `>`, `/` or whitespace.
fn open_tag_start(lower: &str, name: &str) -> Option<usize> {
    let needle = format!("<{name}");
    let mut from = 0;
    while let Some(found) = lower[from..].find(&needle) {
        let start = from + found;
        let after = start + needle.len();
        match lower.as_bytes().get(after) {
            Some(b'>' | b'/') => return Some(start),
            Some(byte) if byte.is_ascii_whitespace() => return Some(start),
            _ => from = after,
        }
    }
    None
}

/// Byte offset just past the `>` that closes the first `<name ...>` tag.
fn open_tag_end(html: &str, name: &str) -> Option<usize> {
    let lower = html.to_ascii_lowercase();
    let start = open_tag_start(&lower, name)?;
    lower[start..].find('>').map(|end| start + end + 1)
}

fn insert_at(html: &str, position: usize, snippet: &str) -> String {
    let mut result = String::with_capacity(html.len() + snippet.len());
    result.push_str(&html[..position]);
    result.push_str(snippet);
    result.push_str(&html[position..]);
    result
}

#[cfg(test)]
mod tests {
    use super::{
        inject_gridjs, inject_render_diagnostics, inject_result_surface_css, inject_scrollbar_css,
        inject_storage_polyfill,
    };

    type Injector = (&'static str, fn(&str) -> String);

    fn injectors() -> [Injector; 5] {
        [
            ("storage", inject_storage_polyfill),
            ("gridjs", inject_gridjs),
            ("scrollbar", inject_scrollbar_css),
            ("surface", inject_result_surface_css),
            ("diagnostics", |html| {
                inject_render_diagnostics(html, 1, 1, "raw_html")
            }),
        ]
    }

    #[test]
    fn injected_markup_never_precedes_the_doctype() {
        let documents = [
            "<!DOCTYPE html><html><body class=\"page\"><table><tr><td>İ</td></tr></table></body></html>",
            "<!doctype html>
<HTML lang=\"tr\"><BODY style=\"margin:0\"><table></table></BODY></HTML>",
            "<!DOCTYPE html><table><tr><td>ẞ</td></tr></table>",
        ];
        for document in documents {
            for (name, inject) in injectors() {
                let output = inject(document);
                assert!(
                    output.to_ascii_lowercase().starts_with("<!doctype html>"),
                    "{name}: {output:.80}"
                );
            }
        }
    }

    #[test]
    fn head_content_lands_inside_the_head_and_keeps_unicode_intact() {
        let document = "<html><head><title>İ</title></head><body><table></table></body></html>";
        for (name, inject) in injectors() {
            let output = inject(document);
            let first_change = output
                .bytes()
                .zip(document.bytes())
                .position(|(left, right)| left != right)
                .unwrap();
            assert!(first_change <= output.find("</head>").unwrap(), "{name}");
            assert!(first_change >= "<html><head>".len(), "{name}");
            assert!(output.contains("<title>İ</title>"), "{name}");
        }
    }

    #[test]
    fn quoted_doctypes_are_never_split() {
        let document = "<!DOCTYPE html \"a>b\"><table></table>";
        let output = inject_scrollbar_css(document);
        assert!(output.starts_with("<style>"), "{output}");
    }

    #[test]
    fn body_tag_prefixes_are_not_mistaken_for_the_body() {
        let document = "<!DOCTYPE html><bodyguard>x</bodyguard>";
        let output = inject_scrollbar_css(document);
        assert!(output.starts_with("<!DOCTYPE html><style>"), "{output}");
    }
}
