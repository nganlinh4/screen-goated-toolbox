//! CSS styles for markdown view

/// Get font CSS for markdown view (uses locally cached fonts)
pub fn get_font_style() -> String {
    format!(
        "<style>{}</style>",
        crate::overlay::html_components::font_manager::get_font_css()
    )
}

/// CSS styling for the markdown content
pub const MARKDOWN_CSS: &str = r#"
    :root {
        --bg: transparent;
    }
    * { box-sizing: border-box; }

    /* Animation definitions */
    @keyframes shimmer {
        0% { background-position: 100% 0; }
        100% { background-position: -100% 0; }
    }

    /* Appearing animation with blur dissolve - matches realtime overlay style */
    @keyframes content-appear {
        from {
            opacity: 0;
            filter: blur(8px);
            -webkit-backdrop-filter: blur(12px);
            backdrop-filter: blur(12px);
            transform: translateY(4px);
        }
        to {
            opacity: 1;
            filter: blur(0);
            -webkit-backdrop-filter: blur(0);
            backdrop-filter: blur(0);
            transform: translateY(0);
        }
    }

    body {
        font-family: 'Google Sans Flex', 'Segoe UI', -apple-system, sans-serif;
        font-optical-sizing: auto;
        /* wdth 90 for more compact text as requested */
        font-variation-settings: 'wght' 400, 'wdth' 90, 'slnt' 0, 'ROND' 100;
        /* Default size 14px - JavaScript fit_font_to_window handles dynamic scaling for short content */
        font-size: 14px;
        line-height: 1.5; /* Reduced line height for compactness */
        background: var(--bg);
        /* Removed min-height: 100vh to enable proper overflow detection for font scaling */
        color: var(--text-color);
        margin: 0;
        padding: 0; /* Padding now handled by WebView edge margin */
        overflow-x: hidden;
        word-wrap: break-word;
        /* Appearing animation */
        animation: content-appear 0.35s cubic-bezier(0.2, 0, 0.2, 1) forwards;
    }

    body > *:first-child { margin-top: 0; }

    h1 {
        font-size: 1.8em;
        color: var(--primary);
        margin-top: 0;
        margin-bottom: 12px; /* Reduced from 16px */
        padding: 0px;
        border-radius: 42px;
        backdrop-filter: blur(12px);
        -webkit-backdrop-filter: blur(12px);

        font-variation-settings: 'wght' 600, 'wdth' 110, 'slnt' 0, 'ROND' 100;
        text-align: center;
        position: relative;
        overflow: hidden;
    }

    h2 {
        font-size: 1.4em;
        color: var(--secondary);
        /* Removed border-bottom */
        padding-bottom: 4px;
        margin-top: 1.0em; /* Reduced from 1.2em */
        margin-bottom: 0.5em;
        font-variation-settings: 'wght' 550, 'wdth' 100, 'slnt' 0, 'ROND' 100;
    }

    h3 {
        font-size: 1.2em;
        color: var(--h3-color);
        margin-top: 0.8em; /* Reduced from 1.0em */
        margin-bottom: 0.4em;
        font-variation-settings: 'wght' 500, 'wdth' 100, 'slnt' 0, 'ROND' 100;
    }

    h4, h5, h6 {
        color: var(--h4-color);
        margin-top: 0.8em;
        margin-bottom: 0.4em;
        font-variation-settings: 'wght' 500, 'wdth' 100, 'slnt' 0, 'ROND' 100;
    }

    p { margin: 0 0; }

    /* Interactive Word Styling - COLOR ONLY, preserves font scaling */
    .word {
        display: inline;
        transition: color 0.2s ease, text-shadow 0.2s ease;
        cursor: text;
    }

    /* 1. Center (Hovered) - Bright cyan + glow */
    .word:hover {
        color: var(--primary);
        text-shadow: 0 0 12px var(--shadow-color);
    }

    /* 2. Immediate Neighbors (Distance: 1) - Light cyan */
    .word:hover + .word {
        color: var(--h4-color);
        text-shadow: 0 0 6px var(--shadow-weak);
    }
    .word:has(+ .word:hover) {
        color: var(--h4-color);
        text-shadow: 0 0 6px var(--shadow-weak);
    }

    /* 3. Secondary Neighbors (Distance: 2) - Lighter cyan */
    .word:hover + .word + .word {
        color: var(--h3-color);
    }
    .word:has(+ .word + .word:hover) {
        color: var(--h3-color);
    }

    /* Headers need specific overriding to ensure the fisheye works on top of their base styles */
    h1 .word:hover, h2 .word:hover, h3 .word:hover {
        color: var(--primary);
    }

    /* Ensure code blocks remain non-interactive */
    pre .word {
        display: inline;
        transition: none;
    }
    pre .word:hover,
    pre .word:hover + .word,
    pre .word:has(+ .word:hover) {
        color: inherit;
        text-shadow: none;
    }

    pre code {
        background: transparent;
        padding: 0;
        color: var(--code-color);
    }

    a { color: var(--link-color); text-decoration: none; transition: all 0.2s; cursor: pointer; }
    a .word { cursor: pointer; } /* Ensure link words show hand cursor */
    a:hover { color: var(--link-hover-color); text-shadow: 0 0 10px var(--link-shadow); text-decoration: none; }

    ul, ol { padding-left: 20px; margin: 0 0; }
    li { margin: 2px 0; } /* Reduced from 4px */

    table {
        width: 100%;
        border-collapse: separate;
        border-spacing: 0;
        margin: 12px 0; /* Reduced from 16px */
        border-radius: 8px;
        overflow: hidden;
        border: 1px solid var(--border-color);
        background: var(--table-bg);
    }
    th {
        background: var(--table-header-bg);
        padding: 8px 10px; /* Reduced from 10px */
        color: var(--primary);
        text-align: left;
        font-weight: 600;
        border-bottom: 1px solid var(--border-color);
        font-variation-settings: 'wght' 600, 'wdth' 100, 'slnt' 0, 'ROND' 100;
    }
    td {
        padding: 6px 10px; /* Reduced from 8px */
        border-top: 1px solid var(--border-color);
    }
    tr:first-child td { border-top: none; }
    tr:hover td { background: var(--glass); }

    hr { border: none; height: 1px; background: var(--border-color); margin: 16px 0; } /* Reduced from 24px */
    img { max-width: 100%; border-radius: 8px; box-shadow: 0 4px 12px rgba(0,0,0,0.3); }

    /* Streaming chunk animation - blur-dissolve for ONLY new content */
    @keyframes stream-chunk-in {
        from {
            opacity: 0;
            filter: blur(4px);
            transform: translateX(-2px);
        }
        to {
            opacity: 1;
            filter: blur(0);
            transform: translateX(0);
        }
    }

    /* Legacy chunk-appear kept for compatibility */
    @keyframes chunk-appear {
        from {
            opacity: 0;
            filter: blur(4px);
        }
        to {
            opacity: 1;
            filter: blur(0);
        }
    }

    /* Class for newly streamed text */
    .streaming-new {
        display: inline;
        animation: stream-chunk-in 0.25s ease-out forwards;
    }

    /* Smooth transition for all direct body children during updates */
    body > * {
        transition: opacity 0.15s ease-out, filter 0.15s ease-out;
    }

    ::-webkit-scrollbar { display: none; }
"#;

/// Get theme CSS variables based on mode
pub fn get_theme_css(is_dark: bool) -> String {
    if is_dark {
        r#"
        :root {
            --primary: #4fc3f7; /* Cyan 300 */
            --secondary: #81d4fa; /* Cyan 200 */
            --text-color: white;
            --h3-color: #b3e5fc; /* Cyan 100 */
            --h4-color: #e1f5fe; /* Cyan 50 */
            --code-color: #d4d4d4;
            --link-color: #82b1ff; /* Blue A100 */
            --link-hover-color: #448aff; /* Blue A200 */
            --link-shadow: rgba(68,138,255,0.4);
            --border-color: #333;
            --table-bg: rgba(0,0,0,0.2);
            --table-header-bg: #222;
            --glass: rgba(255, 255, 255, 0.03);
            --shadow-color: rgba(79, 195, 247, 0.6);
            --shadow-weak: rgba(79, 195, 247, 0.3);
            --sort-icon-filter: invert(1) brightness(200%) grayscale(100%);
            --bg: transparent;
        }
        "#
        .to_string()
    } else {
        r#"
        :root {
            --primary: #0288d1; /* Light Blue 700 */
            --secondary: #0277bd; /* Light Blue 800 */
            --text-color: #222;
            --h3-color: #01579b; /* Light Blue 900 */
            --h4-color: #0277bd;
            --code-color: #444;
            --link-color: #1976d2; /* Blue 700 */
            --link-hover-color: #0d47a1; /* Blue 900 */
            --link-shadow: rgba(13, 71, 161, 0.25);
            --border-color: #ddd;
            --table-bg: rgba(255,255,255,0.4);
            --table-header-bg: rgba(240,240,240,0.8);
            --glass: rgba(0, 0, 0, 0.03);
            --shadow-color: rgba(2, 136, 209, 0.4);
            --shadow-weak: rgba(2, 136, 209, 0.2);
            --sort-icon-filter: none;
            --bg: transparent;
        }
        "#
        .to_string()
    }
}
