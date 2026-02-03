//! HTML manipulation utilities for markdown view

/// Minimal HTML escaping for text content
pub fn escape_html_text(text: &str) -> String {
    text.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace("\"", "&quot;")
        .replace("'", "&#39;")
}

/// Check if content is already HTML (rather than Markdown)
pub fn is_html_content(content: &str) -> bool {
    let trimmed = content.trim();
    // Check for HTML doctype or opening html tag
    trimmed.starts_with("<!DOCTYPE")
        || trimmed.starts_with("<!doctype")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<HTML")
        // Check for common HTML structure patterns
        || (trimmed.contains("<html") && trimmed.contains("</html>"))
        || (trimmed.contains("<head") && trimmed.contains("</head>"))
        // Also detect HTML fragments (has script/style but no html wrapper)
        || is_html_fragment(content)
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

/// Check if HTML content contains scripts that need full browser capabilities
/// (localStorage, sessionStorage, IndexedDB, etc.)
pub fn content_needs_recreation(html: &str) -> bool {
    let lower = html.to_lowercase();
    // If content has <script> tags that might use storage APIs, it needs recreation
    // to get a proper origin instead of the sandboxed document.write context
    lower.contains("<script")
        && (lower.contains("localstorage")
            || lower.contains("sessionstorage")
            || lower.contains("indexeddb")
            || lower.contains("const ") // Variable declarations can conflict
            || lower.contains("let ")
            || lower.contains("var "))
}
