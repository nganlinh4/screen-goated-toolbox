//! Streaming content functions for markdown view

mod fit_impl;

use windows::Win32::Foundation::*;

use super::WEBVIEWS;
use super::conversion::markdown_to_html;
use super::webview::create_markdown_webview_ex;

pub use fit_impl::{fit_font_to_window, init_gridjs};

#[derive(Clone, Copy)]
struct StreamingUpdateOptions {
    run_inline_sizing: bool,
    animate_new_words: bool,
    smooth_scroll: bool,
}

/// Stream markdown content - optimized for rapid updates during streaming.
/// Uses innerHTML instead of document recreation.
pub fn stream_markdown_content(parent_hwnd: HWND, markdown_text: &str) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;
    let (is_refining, preset_prompt, input_text) = {
        let states = crate::overlay::result::state::WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get(&hwnd_key) {
            (
                state.is_refining,
                state.preset_prompt.clone(),
                state.input_text.clone(),
            )
        } else {
            (false, String::new(), String::new())
        }
    };

    stream_markdown_content_ex(
        parent_hwnd,
        markdown_text,
        is_refining,
        &preset_prompt,
        &input_text,
    )
}

/// Finalize streamed markdown content without the live-stream tail effects.
pub fn finalize_stream_markdown_content(parent_hwnd: HWND, markdown_text: &str) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;
    let (is_refining, preset_prompt, input_text) = {
        let states = crate::overlay::result::state::WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get(&hwnd_key) {
            (
                state.is_refining,
                state.preset_prompt.clone(),
                state.input_text.clone(),
            )
        } else {
            (false, String::new(), String::new())
        }
    };

    finalize_stream_markdown_content_ex(
        parent_hwnd,
        markdown_text,
        is_refining,
        &preset_prompt,
        &input_text,
    )
}

/// Stream markdown content - internal version for rapid streaming updates.
pub fn stream_markdown_content_ex(
    parent_hwnd: HWND,
    markdown_text: &str,
    is_refining: bool,
    preset_prompt: &str,
    input_text: &str,
) -> bool {
    update_stream_markdown_content_ex(
        parent_hwnd,
        markdown_text,
        is_refining,
        preset_prompt,
        input_text,
        StreamingUpdateOptions {
            run_inline_sizing: true,
            animate_new_words: true,
            smooth_scroll: true,
        },
    )
}

/// Finalize streamed markdown content for the last flush after streaming stops.
pub fn finalize_stream_markdown_content_ex(
    parent_hwnd: HWND,
    markdown_text: &str,
    is_refining: bool,
    preset_prompt: &str,
    input_text: &str,
) -> bool {
    update_stream_markdown_content_ex(
        parent_hwnd,
        markdown_text,
        is_refining,
        preset_prompt,
        input_text,
        StreamingUpdateOptions {
            run_inline_sizing: false,
            animate_new_words: false,
            smooth_scroll: false,
        },
    )
}

fn update_stream_markdown_content_ex(
    parent_hwnd: HWND,
    markdown_text: &str,
    is_refining: bool,
    preset_prompt: &str,
    input_text: &str,
    options: StreamingUpdateOptions,
) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;

    if !WEBVIEWS.with(|webviews| webviews.borrow().contains_key(&hwnd_key)) {
        return create_markdown_webview_ex(
            parent_hwnd,
            markdown_text,
            false,
            is_refining,
            preset_prompt,
            input_text,
        );
    }

    let html = markdown_to_html(markdown_text, is_refining, preset_prompt, input_text);
    let body_content = if let Some(body_start) = html.find("<body>") {
        let after_body = &html[body_start + 6..];
        if let Some(body_end) = after_body.find("</body>") {
            &after_body[..body_end]
        } else {
            &html[..]
        }
    } else {
        &html[..]
    };

    let escaped_content = body_content
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${");
    let scroll_behavior = if options.smooth_scroll {
        "smooth"
    } else {
        "auto"
    };
    let source_len = markdown_text.len();
    let source_trimmed_len = markdown_text.trim().len();

    let script = format!(
        r#"(function() {{
    const newContent = `{}`;
    const sourceTextLen = {};
    const sourceTrimmedLen = {};
    const prevWordCount = window._streamWordCount || 0;
    const prevRenderCount = window._streamRenderCount || 0;
    const shouldRunInlineSizing = {};
    const shouldAnimateNewWords = {};
    const scrollBehavior = '{}';

    document.body.innerHTML = newContent;

    var body = document.body;
    var doc = document.documentElement;
    if (!body || !doc) {{
        if (window.__SGT_REPORT_RENDER_DIAGNOSTICS__) {{
            window.__SGT_REPORT_RENDER_DIAGNOSTICS__({{
                phase: 'stream_update',
                reason: 'missing_body_after_stream_update',
                sourceTextLen: sourceTextLen,
                sourceTrimmedLen: sourceTrimmedLen
            }});
        }}
        return;
    }}

    var winH = window.innerHeight;
    var winW = window.innerWidth;
    var isConstrainedWindow = (winH < 260 || winW < 420);
    var text = (body.innerText || body.textContent || '').trim();
    var textLen = text.length;
    var isNewSession = (prevRenderCount === 0 || (prevWordCount < 5 && textLen < 50));
    var isConstrainedShortContent = isConstrainedWindow && textLen < 450;

    function currentLineHeightPx() {{
        var computed = window.getComputedStyle(body);
        var fontSize = parseFloat(computed.fontSize) || parseFloat(body.style.fontSize) || 14;
        var lineHeight = parseFloat(computed.lineHeight);
        if (!Number.isFinite(lineHeight)) {{
            var inlineLineHeight = parseFloat(body.style.lineHeight);
            lineHeight = fontSize * (Number.isFinite(inlineLineHeight) ? inlineLineHeight : 1.5);
        }}
        return Math.max(1, lineHeight);
    }}

    function hasPathologicalWrap() {{
        if (textLen < 8) {{
            return false;
        }}

        var tokens = text.split(/\s+/).filter(Boolean);
        var wordCount = tokens.length;
        var longestToken = 0;
        for (var i = 0; i < tokens.length; i++) {{
            longestToken = Math.max(longestToken, tokens[i].length);
        }}

        var approxLineCount = Math.max(1, Math.round(doc.scrollHeight / currentLineHeightPx()));
        var avgCharsPerLine = textLen / approxLineCount;

        return avgCharsPerLine < 3.5
            && approxLineCount > Math.max(3, wordCount + 1)
            && (wordCount <= 12 || longestToken >= 4);
    }}

    function fitsVertically() {{
        void body.offsetHeight;
        return doc.scrollHeight <= (winH + 2) && !hasPathologicalWrap();
    }}

    var minSize = (textLen < 200) ? 6 : 14;

    if (shouldRunInlineSizing) {{
        if (isNewSession) {{
            var maxPossible = Math.min(isConstrainedWindow ? 84 : 110, winH);
            var estimated = Math.sqrt((winW * winH) / (textLen + 1));
            var low = Math.max(minSize, Math.floor(estimated * 0.5));
            var high = Math.min(maxPossible, Math.ceil(estimated * 1.15));
            if (low > high) low = high;

            body.style.fontVariationSettings = "'wght' 400, 'wdth' 90, 'slnt' 0, 'ROND' 100";
            body.style.letterSpacing = '0px';
            body.style.wordSpacing = '0px';
            body.style.lineHeight = '1.5';
            body.style.paddingTop = '0';
            body.style.paddingBottom = '0';

            var blocks = body.querySelectorAll('p, h1, h2, h3, li, blockquote');
            for (var i = 0; i < blocks.length; i++) {{
                blocks[i].style.marginBottom = '0.5em';
                blocks[i].style.paddingBottom = '0';
            }}

            void body.offsetHeight;
            var best = low;
            while (low <= high) {{
                var mid = Math.floor((low + high) / 2);
                body.style.fontSize = mid + 'px';
                if (fitsVertically()) {{
                    best = mid;
                    low = mid + 1;
                }} else {{
                    high = mid - 1;
                }}
            }}
            if (best < minSize) best = minSize;
            body.style.fontSize = best + 'px';

            if (isConstrainedShortContent) {{
                void body.offsetHeight;
                var settleLow = minSize;
                var settleHigh = best;
                var settleBest = minSize;
                while (settleLow <= settleHigh) {{
                    var settleMid = Math.floor((settleLow + settleHigh) / 2);
                    body.style.fontSize = settleMid + 'px';
                    if (fitsVertically()) {{
                        settleBest = settleMid;
                        settleLow = settleMid + 1;
                    }} else {{
                        settleHigh = settleMid - 1;
                    }}
                }}
                body.style.fontSize = settleBest + 'px';
            }}
        }} else {{
            var hasOverflow = !fitsVertically();
            if (hasOverflow) {{
                var currentSize = parseFloat(body.style.fontSize) || 14;
                if (currentSize > minSize) {{
                    var low = minSize;
                    var high = currentSize;
                    var best = minSize;
                    while (low <= high) {{
                        var mid = Math.floor((low + high) / 2);
                        body.style.fontSize = mid + 'px';
                        if (fitsVertically()) {{
                            best = mid;
                            low = mid + 1;
                        }} else {{
                            high = mid - 1;
                        }}
                    }}
                    body.style.fontSize = best + 'px';
                }}
            }}
        }}
    }}

    const words = document.querySelectorAll('.word');
    const newWordCount = words.length;

    if (shouldAnimateNewWords && !isNewSession) {{
        let newWords = [];
        for (let i = prevWordCount; i < newWordCount; i++) {{
            newWords.push(words[i]);
        }}

        if (newWords.length > 0) {{
            newWords.forEach(w => {{
                w.style.opacity = '0';
                w.style.filter = 'blur(2px)';
            }});

            requestAnimationFrame(() => {{
                newWords.forEach(w => {{
                    w.style.transition = 'opacity 0.35s ease-out, filter 0.35s ease-out';
                    w.style.opacity = '1';
                    w.style.filter = 'blur(0)';
                }});
            }});
        }}
    }}

    if (window.__SGT_REPORT_RENDER_DIAGNOSTICS__) {{
        window.__SGT_REPORT_RENDER_DIAGNOSTICS__({{
            phase: 'stream_update',
            sourceTextLen: sourceTextLen,
            sourceTrimmedLen: sourceTrimmedLen
        }});
    }}

    if (body.style.opacity === '0') {{
        body.style.opacity = '1';
    }}

    window._streamWordCount = newWordCount;
    window._streamRenderCount = prevRenderCount + 1;
    window.scrollTo({{ top: document.body.scrollHeight, behavior: scrollBehavior }});
}})()"#,
        escaped_content,
        source_len,
        source_trimmed_len,
        options.run_inline_sizing,
        options.animate_new_words,
        scroll_behavior
    );

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            match webview.evaluate_script(&script) {
                Ok(()) => true,
                Err(err) => {
                    crate::log_info!(
                        "[MarkdownDiag] stream_evaluate_script_failed hwnd={:?} source_len={} source_trimmed_len={} html_len={} err={:?}",
                        parent_hwnd,
                        source_len,
                        source_trimmed_len,
                        body_content.len(),
                        err
                    );
                    false
                }
            }
        } else {
            crate::log_info!(
                "[MarkdownDiag] stream_update_missing_webview hwnd={:?} source_len={} source_trimmed_len={} html_len={}",
                parent_hwnd,
                source_len,
                source_trimmed_len,
                body_content.len()
            );
            false
        }
    })
}

/// Reset the stream content tracker.
pub fn reset_stream_counter(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key)
            && let Err(err) = webview.evaluate_script(
                "window._streamPrevLen = 0; window._streamPrevContent = ''; window._streamWordCount = 0; window._streamRenderCount = 0;",
            )
        {
            crate::log_info!(
                "[MarkdownDiag] reset_stream_counter_failed hwnd={:?} err={:?}",
                parent_hwnd,
                err
            );
        }
    });
}

/// Set body opacity (hide before fitting, reveal after).
pub fn set_body_opacity(parent_hwnd: HWND, visible: bool) {
    let hwnd_key = parent_hwnd.0 as isize;
    let opacity = if visible { "1" } else { "0" };
    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key)
            && let Err(err) = webview.evaluate_script(&format!(
                "if(document.body) document.body.style.opacity = '{}';",
                opacity
            ))
        {
            crate::log_info!(
                "[MarkdownDiag] set_body_opacity_failed hwnd={:?} visible={} err={:?}",
                parent_hwnd,
                visible,
                err
            );
        }
    });
}
