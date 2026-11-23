//! Markdown to HTML conversion

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};

use super::css::{MARKDOWN_CSS, get_compositor_font_style, get_export_font_style, get_theme_css};
use super::html_utils::{
    escape_html_text, inject_gridjs, inject_render_diagnostics, inject_result_surface_css,
    inject_scrollbar_css, inject_storage_polyfill, normalize_html_content,
};

const INTERACTIVE_WORD_WRAP_CHAR_LIMIT: usize = 6000;
const INTERACTIVE_WORD_WRAP_WORD_LIMIT: usize = 900;

pub struct CompositorRender {
    pub body: String,
    pub isolated_document: Option<String>,
}

pub fn render_for_compositor(markdown: &str) -> CompositorRender {
    if let Some(raw_html) = normalize_html_content(markdown) {
        let document = markdown_to_html_for_compositor(&raw_html);
        return CompositorRender {
            body: document_body(&document),
            isolated_document: Some(document),
        };
    }
    CompositorRender {
        body: render_markdown_body(markdown).0,
        isolated_document: None,
    }
}

pub(super) fn should_enable_interactive_word_wrap(markdown: &str) -> bool {
    if markdown.len() > INTERACTIVE_WORD_WRAP_CHAR_LIMIT {
        return false;
    }

    let mut word_count = 0usize;
    for _ in markdown.split_whitespace() {
        word_count += 1;
        if word_count > INTERACTIVE_WORD_WRAP_WORD_LIMIT {
            return false;
        }
    }

    true
}

/// Build a portable styled document, preserving authored complete HTML.
pub fn markdown_to_html_for_export(markdown: &str) -> String {
    markdown_to_html_with_font_style(markdown, get_export_font_style)
}

pub fn markdown_to_html_for_compositor(markdown: &str) -> String {
    markdown_to_html_with_font_style(markdown, get_compositor_font_style)
}

fn markdown_to_html_with_font_style(markdown: &str, font_style: impl FnOnce() -> String) -> String {
    // If input is already HTML, inject localStorage polyfill, Grid.js, and hidden scrollbar styles
    if let Some(raw_html) = normalize_html_content(markdown) {
        let with_storage = inject_storage_polyfill(&raw_html);
        let with_grid = inject_gridjs(&with_storage);
        let with_scrollbar = inject_scrollbar_css(&with_grid);
        let with_surface = inject_result_surface_css(&with_scrollbar);
        return inject_render_diagnostics(
            &with_surface,
            markdown.len(),
            markdown.trim().len(),
            "raw_html",
        );
    }

    let theme_css = get_theme_css(crate::overlay::is_dark_mode());
    let font_style = font_style();
    let (html_output, has_table) = render_markdown_body(markdown);

    // Grid.js Integration
    let gridjs_head = if has_table {
        format!(
            r#"<style>{}{}</style><script>{}</script>"#,
            crate::overlay::html_components::grid_js::get_lib_css(),
            crate::overlay::html_components::grid_js::get_css(),
            crate::overlay::html_components::grid_js::get_lib_js()
        )
    } else {
        String::new()
    };

    let gridjs_body = if has_table {
        format!(
            r#"<script>{}</script>"#,
            crate::overlay::html_components::grid_js::get_init_script()
        )
    } else {
        String::new()
    };

    let html = format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <style id="sgt-theme-css">{}</style>
    {}
    <style>{}</style>
    {}
</head>
<body>
    {}
    {}
</body>
</html>"#,
        theme_css, font_style, MARKDOWN_CSS, gridjs_head, html_output, gridjs_body
    );

    inject_render_diagnostics(&html, markdown.len(), markdown.trim().len(), "markdown")
}

fn render_markdown_body(markdown: &str) -> (String, bool) {
    let html_output =
        render_markdown_segment(markdown, should_enable_interactive_word_wrap(markdown));
    let has_table = html_output.contains("<table");
    (html_output, has_table)
}

/// Render Markdown blocks; word wrapping is decided for the whole document.
pub(super) fn render_markdown_segment(
    markdown: &str,
    enable_interactive_word_wrap: bool,
) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);

    // Custom wrapper to enable word-level interaction
    // We map text events to HTML events containing wrapped words
    let mut in_code_block = false;
    let mut in_table = false;

    let wrapped_parser = parser.map(|event| match event {
        Event::Start(Tag::CodeBlock(_)) => {
            in_code_block = true;
            event
        }
        Event::End(TagEnd::CodeBlock) => {
            in_code_block = false;
            event
        }
        Event::Start(Tag::Table(_)) => {
            in_table = true;
            event
        }
        Event::End(TagEnd::Table) => {
            in_table = false;
            event
        }
        Event::Code(_) => {
            // Inline code event - return as is
            event
        }
        Event::Text(text) => {
            if enable_interactive_word_wrap && !in_code_block && !in_table {
                // Split text into words and wrap
                let mut output = String::with_capacity(text.len() * 2);
                let escaped = escape_html_text(&text);

                for (i, part) in escaped.split(' ').enumerate() {
                    if i > 0 {
                        output.push(' ');
                    }
                    if part.trim().is_empty() {
                        output.push_str(part);
                    } else {
                        output.push_str("<span class=\"word\">");
                        output.push_str(part);
                        output.push_str("</span>");
                    }
                }
                Event::Html(output.into())
            } else {
                Event::Text(text)
            }
        }
        Event::SoftBreak => Event::HardBreak,
        _ => event,
    });

    let mut html_output = String::new();
    html::push_html(&mut html_output, wrapped_parser);
    html_output
}

fn document_body(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let Some(body_start) = lower.find("<body") else {
        return html.to_string();
    };
    let Some(tag_end) = lower[body_start..].find('>') else {
        return html.to_string();
    };
    let content_start = body_start + tag_end + 1;
    let Some(content_end) = lower[content_start..].rfind("</body>") else {
        return html[content_start..].to_string();
    };
    html[content_start..content_start + content_end].to_string()
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;

    use super::{
        markdown_to_html_for_compositor, markdown_to_html_for_export,
        markdown_to_html_with_font_style, render_for_compositor,
    };

    #[test]
    fn exported_markdown_embeds_the_exact_font_without_a_session_resource() {
        let html = markdown_to_html_for_export("# Saved answer\n\n**Portable** text");
        let prefix = "data:font/woff2;base64,";
        assert_eq!(html.matches(prefix).count(), 1);
        let encoded = html
            .split_once(prefix)
            .unwrap()
            .1
            .split_once('\'')
            .unwrap()
            .0;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        assert_eq!(decoded, crate::assets::GOOGLE_SANS_FLEX_WEB);
        assert!(html.contains("<h1>"));
        let strong = html.split_once("<strong>").unwrap().1;
        assert!(
            strong
                .split_once("</strong>")
                .unwrap()
                .0
                .contains("Portable")
        );
        assert!(!html.contains("http://127.0.0.1:"));
        assert!(!html.contains("sgt-font-ready"));
    }

    #[test]
    fn exported_complete_html_preserves_authored_font_and_resource_choices() {
        let html = markdown_to_html_for_export(
            "<html><head><style>body{font-family:serif}</style></head><body><img src=\"image.png\"><script>window.ready=true</script></body></html>",
        );
        assert!(html.contains("body{font-family:serif}"));
        assert!(html.contains("src=\"image.png\""));
        assert!(html.contains("window.ready=true"));
        assert!(!html.contains("data:font/woff2;base64,"));
        assert!(!html.contains("http://127.0.0.1:"));
    }

    #[test]
    fn raw_html_does_not_build_unused_font_style() {
        let html = markdown_to_html_with_font_style("<html><body>hello</body></html>", || {
            panic!("raw HTML must not build font CSS")
        });
        assert!(html.contains("hello"));
        let html = markdown_to_html_with_font_style("hello", || "font marker".into());
        assert!(html.contains("font marker"));
    }

    #[test]
    fn html_injection_preserves_unicode_before_document_tags() {
        let input = "<html>İ<head><title>K</title></head><body><table><tr><td>ẞ</td></tr></table></body></html>";
        let document = markdown_to_html_for_compositor(input);
        assert!(document.contains("<title>K</title>"));
        assert!(document.contains("<td>ẞ</td>"));
        assert!(document.contains("</body></html>"));
        assert!(!document.contains("https://unpkg.com/gridjs"));
    }

    #[test]
    fn compositor_markdown_uses_the_visibility_gated_bundled_font() {
        let html = markdown_to_html_for_compositor("hello");

        assert!(html.contains("Google Sans Flex"));
        assert!(html.contains("html:not(.sgt-font-ready) body"));
        assert_eq!(html.matches("data:font/woff2;base64,").count(), 1);
        assert!(!html.contains("data:font/ttf;base64,"));
    }

    #[test]
    fn compositor_raw_html_preserves_authored_typography_and_scripts() {
        let html = markdown_to_html_for_compositor(
            "<html><head><style>body{font-family:serif}</style></head><body>hello<script>window.gameReady=true;</script></body></html>",
        );

        assert!(html.contains("body{font-family:serif}"));
        assert!(html.contains("window.gameReady=true"));
        assert!(html.contains("data-sgt-result-surface=\"1\""));
        assert!(html.contains(":where(html, body) { width: 100%; height: 100%; margin: 0; }"));
        assert!(!html.contains("Google Sans Flex"));
        assert!(!html.contains("html:not(.sgt-font-ready) body"));
    }

    #[test]
    fn ordinary_compositor_updates_only_build_replaceable_body_markup() {
        let rendered = render_for_compositor("hello **world**");

        assert!(rendered.isolated_document.is_none());
        assert!(rendered.body.contains("<strong>"));
        assert!(!rendered.body.contains("<!DOCTYPE html>"));
        assert!(!rendered.body.contains("/font.woff2"));
    }

    #[test]
    fn raw_html_keeps_an_isolated_compatibility_document() {
        let rendered = render_for_compositor("<html><head></head><body><p>hello</p></body></html>");

        assert_eq!(rendered.body.trim_start(), "<p>hello</p>");
        assert!(rendered.isolated_document.is_some());
    }

    #[test]
    fn fenced_html_is_normalized_before_becoming_an_isolated_document() {
        let rendered = render_for_compositor(
            "```html\n<!doctype html><html><body><button>Play</button></body></html>\n```",
        );

        let document = rendered.isolated_document.unwrap();
        assert!(document.trim_start().starts_with("<!doctype html>"));
        assert!(!document.contains("```"));
        assert_eq!(rendered.body.trim(), "<button>Play</button>");
    }

    #[test]
    fn shared_html_classification_preserves_markdown_examples() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../parity-fixtures/preset-system/result-overlay.json"
        ))
        .unwrap();
        for case in fixture["markdown_view"]["raw_html_classification_cases"]
            .as_array()
            .unwrap()
        {
            let name = case["name"].as_str().unwrap();
            let input = case["input"].as_str().unwrap();
            let rendered = render_for_compositor(input);
            assert_eq!(
                rendered.isolated_document.is_some(),
                case["raw"].as_bool().unwrap(),
                "{name}"
            );
            if name == "fenced_example_with_prose" {
                assert!(rendered.body.contains("Example:"));
                assert!(rendered.body.contains("Explanation."));
                assert!(rendered.body.contains("&lt;html&gt;"));
            }
            if name == "inline_script" || name == "inline_style" {
                assert!(rendered.body.contains("&lt;"), "{name}");
            }
        }
    }

    #[test]
    fn html_context_depth_is_bounded() {
        for (depth, raw) in [(64, true), (65, false)] {
            let document = format!(
                "<html>{}{}</html>",
                "<template>".repeat(depth),
                "</template>".repeat(depth)
            );
            assert_eq!(
                render_for_compositor(&document).isolated_document.is_some(),
                raw
            );
        }
    }

    #[test]
    fn deeply_nested_foreign_content_is_rejected() {
        let input = format!("<html>{}{}</html>", "<svg>".repeat(65), "</svg>".repeat(65));
        assert!(render_for_compositor(&input).isolated_document.is_none());
    }
}
