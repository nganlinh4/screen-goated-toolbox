//! Markdown to HTML conversion

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};

use super::css::{MARKDOWN_CSS, get_compositor_font_style, get_font_style, get_theme_css};
use super::html_utils::{
    escape_html_text, inject_gridjs, inject_render_diagnostics, inject_scrollbar_css,
    inject_storage_polyfill, is_html_content,
};

const INTERACTIVE_WORD_WRAP_CHAR_LIMIT: usize = 6000;
const INTERACTIVE_WORD_WRAP_WORD_LIMIT: usize = 900;

pub struct CompositorRender {
    pub body: String,
    pub isolated_document: Option<String>,
}

pub fn render_for_compositor(
    markdown: &str,
    is_refining: bool,
    preset_prompt: &str,
    input_text: &str,
) -> CompositorRender {
    if is_refining && crate::overlay::utils::SHOW_REFINING_CONTEXT_QUOTE {
        let combined = refining_context(preset_prompt, input_text);
        return CompositorRender {
            body: crate::overlay::utils::get_context_quote(&combined),
            isolated_document: None,
        };
    }
    if is_html_content(markdown) {
        let document =
            markdown_to_html_for_compositor(markdown, is_refining, preset_prompt, input_text);
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

fn should_enable_interactive_word_wrap(markdown: &str) -> bool {
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

/// Convert markdown text to styled HTML, or pass through raw HTML
pub fn markdown_to_html(
    markdown: &str,
    is_refining: bool,
    preset_prompt: &str,
    input_text: &str,
) -> String {
    markdown_to_html_with_font_style(
        markdown,
        is_refining,
        preset_prompt,
        input_text,
        &get_font_style(),
        false,
    )
}

pub fn markdown_to_html_for_compositor(
    markdown: &str,
    is_refining: bool,
    preset_prompt: &str,
    input_text: &str,
) -> String {
    markdown_to_html_with_font_style(
        markdown,
        is_refining,
        preset_prompt,
        input_text,
        &get_compositor_font_style(),
        true,
    )
}

fn markdown_to_html_with_font_style(
    markdown: &str,
    is_refining: bool,
    preset_prompt: &str,
    input_text: &str,
    font_style: &str,
    inject_raw_font: bool,
) -> String {
    let is_dark = crate::overlay::is_dark_mode();
    let theme_css = get_theme_css(is_dark);

    if is_refining && crate::overlay::utils::SHOW_REFINING_CONTEXT_QUOTE {
        let combined = refining_context(preset_prompt, input_text);
        let quote = crate::overlay::utils::get_context_quote(&combined);
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <style id="sgt-theme-css">{}</style>
    {}
    <style>
        {}
        body {{
            display: flex;
            align-items: center;
            justify-content: center;
            text-align: center;
            height: 100vh;
            margin: 0;
            padding: 12px;
            font-style: italic;
            color: #aaa;
            font-size: 16px;
        }}
    </style>
</head>
<body>
    {}
</body>
</html>"#,
            theme_css, font_style, MARKDOWN_CSS, quote
        );

        return inject_render_diagnostics(
            &html,
            combined.len(),
            combined.trim().len(),
            "refining_context",
        );
    }

    // If input is already HTML, inject localStorage polyfill, Grid.js, and hidden scrollbar styles
    if is_html_content(markdown) {
        let with_storage = inject_storage_polyfill(markdown);
        let with_font = if inject_raw_font {
            inject_style_into_document(&with_storage, font_style)
        } else {
            with_storage
        };
        let with_grid = inject_gridjs(&with_font);
        let with_scrollbar = inject_scrollbar_css(&with_grid);
        return inject_render_diagnostics(
            &with_scrollbar,
            markdown.len(),
            markdown.trim().len(),
            "raw_html",
        );
    }

    let (html_output, has_table) = render_markdown_body(markdown);

    // Grid.js Integration
    let gridjs_head = if has_table {
        let (css_url, js_url) = crate::overlay::html_components::grid_js::get_lib_urls();
        format!(
            r#"<link href="{}" rel="stylesheet" />
            <script src="{}"></script>
            <style>{}</style>"#,
            css_url,
            js_url,
            crate::overlay::html_components::grid_js::get_css()
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

fn refining_context(preset_prompt: &str, input_text: &str) -> String {
    if input_text.is_empty() {
        preset_prompt.to_string()
    } else {
        format!("{preset_prompt}\n\n{input_text}")
    }
}

fn render_markdown_body(markdown: &str) -> (String, bool) {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let enable_interactive_word_wrap = should_enable_interactive_word_wrap(markdown);

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

    let has_table = html_output.contains("<table");
    (html_output, has_table)
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

fn inject_style_into_document(html: &str, style: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut result = html.to_string();
    if let Some(position) = lower.find("</head>") {
        result.insert_str(position, style);
    } else if let Some(position) = lower.find("<body") {
        result.insert_str(position, style);
    } else {
        result.insert_str(0, style);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{markdown_to_html_for_compositor, render_for_compositor};

    #[test]
    fn compositor_markdown_uses_the_visibility_gated_bundled_font() {
        let html = markdown_to_html_for_compositor("hello", false, "", "");

        assert!(html.contains("Google Sans Flex"));
        assert!(html.contains("html:not(.sgt-font-ready) body"));
        assert_eq!(html.matches("data:font/ttf;base64,").count(), 1);
        assert!(!html.contains("data:font/woff;base64,"));
    }

    #[test]
    fn compositor_raw_html_receives_the_same_font_gate() {
        let html = markdown_to_html_for_compositor(
            "<html><head></head><body>hello</body></html>",
            false,
            "",
            "",
        );

        assert!(html.contains("Google Sans Flex"));
        assert!(html.contains("html:not(.sgt-font-ready) body"));
        assert_eq!(html.matches("data:font/ttf;base64,").count(), 1);
    }

    #[test]
    fn ordinary_compositor_updates_only_build_replaceable_body_markup() {
        let rendered = render_for_compositor("hello **world**", false, "", "");

        assert!(rendered.isolated_document.is_none());
        assert!(rendered.body.contains("<strong>"));
        assert!(!rendered.body.contains("<!DOCTYPE html>"));
        assert!(!rendered.body.contains("/font.ttf"));
    }

    #[test]
    fn raw_html_keeps_an_isolated_compatibility_document() {
        let rendered = render_for_compositor(
            "<html><head></head><body><p>hello</p></body></html>",
            false,
            "",
            "",
        );

        assert_eq!(rendered.body.trim_start(), "<p>hello</p>");
        assert!(rendered.isolated_document.is_some());
    }
}
