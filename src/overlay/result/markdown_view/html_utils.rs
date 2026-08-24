//! HTML manipulation utilities for markdown view

/// Minimal HTML escaping for text content
pub fn escape_html_text(text: &str) -> String {
    crate::overlay::utils::escape_html(text)
}

/// Return the executable HTML payload, removing only transport decoration that
/// models commonly add around an otherwise complete document.
pub fn normalize_html_content(content: &str) -> Option<String> {
    let trimmed = content.trim().trim_start_matches('\u{feff}').trim_start();
    let candidate = strip_outer_html_fence(trimmed).unwrap_or(trimmed).trim();
    let lower = candidate.to_ascii_lowercase();

    if lower.starts_with("<!doctype") || lower.starts_with("<html") {
        return Some(candidate.to_string());
    }

    if let Some(html_start) = lower.find("<html")
        && let Some(html_end) = lower.rfind("</html>")
    {
        let doctype_start = lower[..html_start].rfind("<!doctype").unwrap_or(html_start);
        return Some(candidate[doctype_start..html_end + "</html>".len()].to_string());
    }

    if (lower.contains("<head") && lower.contains("</head>")) || is_html_fragment(candidate) {
        return Some(candidate.to_string());
    }

    None
}

fn strip_outer_html_fence(content: &str) -> Option<&str> {
    let first = content.as_bytes().first().copied()?;
    if first != b'`' && first != b'~' {
        return None;
    }
    let marker_len = content.bytes().take_while(|byte| *byte == first).count();
    if marker_len < 3 {
        return None;
    }
    let header_end = content.find('\n')?;
    let label = content[marker_len..header_end].trim();
    if !label.is_empty()
        && !label.eq_ignore_ascii_case("html")
        && !label.eq_ignore_ascii_case("htm")
    {
        return None;
    }
    let closing = std::str::from_utf8(&vec![first; marker_len])
        .ok()?
        .to_string();
    let body_and_close = &content[header_end + 1..];
    let close_start = body_and_close.rfind('\n')?;
    if body_and_close[close_start + 1..].trim() != closing {
        return None;
    }
    Some(&body_and_close[..close_start])
}

/// Check if content is an HTML fragment (has HTML-like content but no document wrapper)
/// Examples: <div><style>...</style><script>...</script></div>
pub fn is_html_fragment(content: &str) -> bool {
    let lower = content.to_lowercase();
    // Has script or style tags but no html/doctype wrapper
    (lower.contains("<script") || lower.contains("<style"))
        && !lower.contains("<!doctype")
        && !lower.contains("<html")
}

/// Wrap an HTML fragment in a proper document structure
/// This ensures WebView2 can properly parse the DOM
pub fn wrap_html_fragment(fragment: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
</head>
<body>
{}
</body>
</html>"#,
        fragment
    )
}

/// Inject localStorage/sessionStorage polyfill into HTML for WebView2 compatibility
/// WebView2's with_html() runs in a sandboxed context that denies storage access
/// This provides an in-memory fallback so scripts don't crash
pub fn inject_storage_polyfill(html: &str) -> String {
    // First, wrap HTML fragments in a proper document structure
    // This ensures WebView2 can properly parse the DOM (fixes "null" getElementById errors)
    let html = if is_html_fragment(html) {
        wrap_html_fragment(html)
    } else {
        html.to_string()
    };

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

    // Find the best place to inject the polyfill (before any other scripts)
    // Priority: after <head>, after <html>, or at the very start
    let lower = html.to_lowercase();

    if let Some(pos) = lower.find("<head>") {
        // Inject right after <head>
        let insert_pos = pos + 6; // length of "<head>"
        let mut result = html[..insert_pos].to_string();
        result.push_str(polyfill);
        result.push_str(&html[insert_pos..]);
        result
    } else if let Some(pos) = lower.find("<head ") {
        // <head with attributes
        if let Some(end) = html[pos..].find('>') {
            let insert_pos = pos + end + 1;
            let mut result = html[..insert_pos].to_string();
            result.push_str(polyfill);
            result.push_str(&html[insert_pos..]);
            result
        } else {
            format!("{}{}", polyfill, html)
        }
    } else if let Some(pos) = lower.find("<html>") {
        let insert_pos = pos + 6;
        let mut result = html[..insert_pos].to_string();
        result.push_str(polyfill);
        result.push_str(&html[insert_pos..]);
        result
    } else if let Some(pos) = lower.find("<html ") {
        if let Some(end) = html[pos..].find('>') {
            let insert_pos = pos + end + 1;
            let mut result = html[..insert_pos].to_string();
            result.push_str(polyfill);
            result.push_str(&html[insert_pos..]);
            result
        } else {
            format!("{}{}", polyfill, html)
        }
    } else {
        // No head or html tag found, prepend polyfill
        format!("{}{}", polyfill, html)
    }
}

/// Inject Grid.js into raw HTML if tables are present
pub fn inject_gridjs(html: &str) -> String {
    if !html.contains("<table") {
        return html.to_string();
    }

    let (css_url, js_url) = crate::overlay::html_components::grid_js::get_lib_urls();
    let gridjs_head = format!(
        r#"<link href="{}" rel="stylesheet" />
        <script src="{}"></script>
        <style>{}</style>"#,
        css_url,
        js_url,
        crate::overlay::html_components::grid_js::get_css()
    );
    let gridjs_body = format!(
        r#"<script>{}</script>"#,
        crate::overlay::html_components::grid_js::get_init_script()
    );

    let lower = html.to_lowercase();
    let mut result = html.to_string();

    // Inject CSS/JS into <head>
    if let Some(pos) = lower.find("</head>") {
        result.insert_str(pos, &gridjs_head);
    } else if let Some(pos) = lower.find("<body>") {
        result.insert_str(pos, &gridjs_head);
    } else {
        result.insert_str(0, &gridjs_head);
    }

    // Inject init script into <body>
    let lower_updated = result.to_lowercase();
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
    let lower = html.to_lowercase();
    let mut result = html.to_string();

    if let Some(pos) = lower.find("</head>") {
        result.insert_str(pos, css);
    } else if let Some(pos) = lower.find("<body>") {
        result.insert_str(pos, css);
    } else {
        result.insert_str(0, css);
    }
    result
}

/// Remove only browser-default page gutters for authored result documents.
/// `:where` keeps zero specificity, so any authored margin or sizing still wins.
pub fn inject_result_surface_css(html: &str) -> String {
    let css = r#"<style data-sgt-result-surface="1">
:where(html, body) { width: 100%; height: 100%; margin: 0; }
:where(body) { box-sizing: border-box; }
</style>"#;
    let lower = html.to_ascii_lowercase();
    let mut result = html.to_string();

    if let Some(position) = lower.find("</head>") {
        result.insert_str(position, css);
    } else if let Some(position) = lower.find("<body>") {
        result.insert_str(position, css);
    } else {
        result.insert_str(0, css);
    }
    result
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
    let lower = html.to_lowercase();
    let mut result = html.to_string();

    if let Some(pos) = lower.find("</head>") {
        result.insert_str(pos, &script);
    } else if let Some(pos) = lower.find("<body>") {
        result.insert_str(pos, &script);
    } else if let Some(pos) = lower.find("</body>") {
        result.insert_str(pos, &script);
    } else {
        result.push_str(&script);
    }

    result
}
