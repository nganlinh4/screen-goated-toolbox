// --- TEXT INPUT STYLES ---
// CSS and HTML generation for the text input WebView.

use super::state::*;
use raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};
use std::num::NonZeroIsize;
use windows::Win32::Foundation::HWND;

/// Wrapper for HWND to implement HasWindowHandle
pub struct HwndWrapper(pub HWND);

impl HasWindowHandle for HwndWrapper {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let hwnd = self.0 .0 as isize;
        if let Some(non_zero) = NonZeroIsize::new(hwnd) {
            let mut handle = Win32WindowHandle::new(non_zero);
            handle.hinstance = None;
            let raw = RawWindowHandle::Win32(handle);
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        } else {
            Err(HandleError::Unavailable)
        }
    }
}

/// CSS for the modern text input editor
pub fn get_editor_css(is_dark: bool) -> String {
    let vars = if is_dark {
        r#"
        :root {
            /* Premium Dark Mode (Google UI Inspired) */
            --bg-color: rgba(32, 33, 36, 0.8);
            --text-color: #e8eaed;
            --header-text: #9aa0a6;
            --footer-text: #9aa0a6;
            --placeholder-color: #9aa0a6;
            --scrollbar-thumb: #5f6368;
            --scrollbar-thumb-hover: #80868b;
            --btn-bg: #3c4043; /* Elevated surface */
            --btn-border: rgba(255, 255, 255, 0.1);
            --mic-fill: #8ab4f8;
            --mic-border: transparent;
            --mic-hover-bg: rgba(138, 180, 248, 0.12);
            --send-fill: #8ab4f8;
            --send-border: transparent;
            --send-hover-bg: rgba(138, 180, 248, 0.12);
            --hint-color: #9aa0a6;
            --close-hover-bg: rgba(232, 234, 237, 0.08);
            --container-border: 1px solid #3c4043;
            --container-shadow: 0 0px 16px rgba(0,0,0,0.25);
            --input-bg: #303134; /* Base surface */
            --input-border: 1px solid transparent;
            --wave-color: #8ab4f8;
        }
        "#
    } else {
        r#"
        :root {
            /* Premium Light Mode (Google UI Inspired) */
            --bg-color: rgba(255, 255, 255, 0.75);
            --text-color: #202124;
            --header-text: #5f6368;
            --footer-text: #5f6368;
            --wave-color: #1a73e8;
            --placeholder-color: #5f6368;
            --scrollbar-thumb: #dadce0;
            --scrollbar-thumb-hover: #bdc1c6;
            --btn-bg: #ffffff; /* Elevated action button */
            --btn-border: #dadce0;
            --mic-fill: #1a73e8;
            --mic-border: transparent;
            --mic-hover-bg: rgba(26, 115, 232, 0.06);
            --send-fill: #1a73e8;
            --send-border: transparent;
            --send-hover-bg: rgba(26, 115, 232, 0.06);
            --hint-color: #5f6368;
            --close-hover-bg: rgba(32, 33, 36, 0.04);
            --container-border: 1px solid #dadce0;
            --container-shadow: 0 0px 16px rgba(0,0,0,0.25);
            --input-bg: #f1f3f4; /* Base surface */
            --input-border: 1px solid transparent;
        }
        "#
    };

    format!(
        r#"
    {vars}

    html, body {{
        width: 100%;
        height: 100%;
        overflow: hidden;
        background: transparent;
        padding: 10px; /* Reduced to fit calc(100% - 20px) better */
        font-family: 'Google Sans Flex', sans-serif;
        font-variation-settings: 'ROND' 100;
    }}

    * {{
        box-sizing: border-box;
        margin: 0;
        padding: 0;
        user-select: none;
        font-variation-settings: 'ROND' 100;
    }}

    *::-webkit-scrollbar {{
        width: 10px;
        height: 10px;
        background: transparent;
    }}
    *::-webkit-scrollbar-thumb {{
        background: var(--scrollbar-thumb);
        border-radius: 5px;
        border: 2px solid transparent;
        background-clip: content-box;
    }}
    *::-webkit-scrollbar-thumb:hover {{
        background: var(--scrollbar-thumb-hover);
        border: 2px solid transparent;
        background-clip: content-box;
    }}

    .editor-container {{
        width: calc(100% - 20px);
        height: calc(100% - 20px);
        margin: 10px;
        display: flex;
        flex-direction: column;
        overflow: hidden;
        background: var(--bg-color);
        position: relative;
        border-radius: 20px;
        border: var(--container-border);
        box-shadow: var(--container-shadow);

        /* Initial State for Animation */
        opacity: 0;
        transform: scale(0.95);
        transition: background 0.2s, border-color 0.2s;
    }}

    .editor-container.entering {{
        animation: inputFadeIn 0.2s cubic-bezier(0.2, 0, 0, 1) forwards;
    }}

    .editor-container.exiting {{
        animation: inputFadeOut 0.15s cubic-bezier(0.2, 0, 0, 1) forwards;
    }}

    @keyframes inputFadeIn {{
        to {{ opacity: 1; transform: scale(1); }}
    }}

    @keyframes inputFadeOut {{
        from {{ opacity: 1; transform: scale(1); }}
        to {{ opacity: 0; transform: scale(0.95); }}
    }}

    /* Header (Draggable) */
    .header {{
        height: 32px;
        background: transparent;
        display: flex;
        align-items: center;
        padding: 0 10px;
        cursor: default;
        /* No border for header to seamless blend */
    }}

    .header-title {{
        flex: 1;
        font-size: 14px;
        font-weight: 600;
        text-transform: uppercase;
        font-stretch: 151%;
        letter-spacing: 0.15em;
        line-height: 24px;
        padding-top: 4px; /* Visual centering */
        color: var(--header-text);
        padding-left: 14px;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
        font-family: 'Google Sans Flex', sans-serif;
    }}

    .header-title span {{
        display: inline-block;
        transition: color 0.2s;
    }}

    @keyframes waveColor {{
        0%, 100% {{
            color: var(--header-text);
            font-variation-settings: 'GRAD' 0, 'wght' 600, 'ROND' 100;
        }}
        50% {{
            color: var(--wave-color);
            font-variation-settings: 'GRAD' 200, 'wght' 1000, 'ROND' 100;
        }}
    }}

    .close-btn {{
        width: 32px;
        height: 32px;
        display: flex;
        align-items: center;
        justify-content: center;
        border-radius: 50%;
        cursor: pointer;
        color: var(--header-text);
        transition: background 0.1s;
        margin-right: 6px;
    }}

    .close-btn svg {{
        width: 20px;
        height: 20px;
        fill: currentColor;
    }}

    .mic-btn svg, .send-btn svg {{
        width: 22px;
        height: 22px;
    }}

    .mic-btn svg {{ fill: var(--mic-fill); }}
    .send-btn svg {{ fill: var(--send-fill); }}

    .close-btn:hover {{
        background: var(--close-hover-bg);
    }}

    #editor {{
        flex: 1;
        width: 100%;
        margin: 0px 8px;
        background: var(--input-bg);
        border-radius: 22px; /* Ultra rounded pill look */
        padding: 12px 14px;
        padding-right: 68px; /* Space for mic + send buttons to prevent overlap */
        border: var(--input-border);
        outline: none;
        resize: none;
        font-family: 'Google Sans Flex', sans-serif;
        font-size: 15px;
        line-height: 1.55;
        color: var(--text-color);
        overflow-y: auto;
        user-select: text;
        width: calc(100% - 16px);
    }}

    #editor::placeholder {{
        color: var(--placeholder-color);
        opacity: 1;
    }}

    /* Footer */
    .footer {{
        height: 28px;
        background: transparent;
        /* No border for seamless blend */
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 11px;
        color: var(--footer-text);
        font-variation-settings: 'ROND' 100, 'slnt' -10;
        cursor: default;
    }}

    /* Floating Buttons */
    /* Floating Buttons - Vertical Stack */
    .btn-container {{
        position: absolute;
        bottom: 40px; /* Above footer */
        right: 20px;
        display: flex;
        flex-direction: column;
        gap: 12px;
        z-index: 100;
    }}

    .mic-btn, .send-btn {{
        width: 48px;
        height: 48px; /* Big buttons */
        border-radius: 50%;
        display: flex;
        align-items: center;
        justify-content: center;
        cursor: pointer;
        background: var(--btn-bg);
        border: 1px solid var(--btn-border);
        box-shadow: 0 2px 8px rgba(0,0,0,0.1);
        transition: all 0.2s cubic-bezier(0.2, 0.0, 0.2, 1);
        backdrop-filter: blur(8px);
        -webkit-backdrop-filter: blur(8px);
    }}

    .mic-btn svg, .send-btn svg {{
        width: 28px; /* Bigger icons */
        height: 28px;
        transition: transform 0.2s, fill 0.2s;
    }}

    .mic-btn:active, .send-btn:active {{
        transform: scale(0.95);
    }}



    .mic-btn svg {{ fill: var(--mic-fill); }}
    .send-btn svg {{ fill: var(--send-fill); }}

    .mic-btn:hover {{
        background: var(--mic-hover-bg);
        border-color: var(--mic-fill);
    }}

    .send-btn:hover {{
        background: var(--send-hover-bg);
        border-color: var(--send-fill);
    }}
"#,
        vars = vars
    )
}

/// Generate HTML for the text input webview
pub fn get_editor_html(placeholder: &str, is_dark: bool) -> String {
    let css = get_editor_css(is_dark);
    let theme_attr = if is_dark {
        "data-theme=\"dark\""
    } else {
        "data-theme=\"light\""
    };
    let font_css = crate::overlay::html_components::font_manager::get_font_css();
    let escaped_placeholder = placeholder
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");

    // Locale text
    let (submit_txt, newline_txt, cancel_txt) = {
        let lang = CFG_LANG.lock().unwrap().clone();
        let locale = crate::gui::locale::LocaleText::get(&lang);
        (
            locale.text_input_footer_submit.to_string(),
            locale.text_input_footer_newline.to_string(),
            locale.text_input_footer_cancel.to_string(),
        )
    };
    let cancel_hint = {
        let sub = CFG_CANCEL.lock().unwrap();
        if sub.is_empty() {
            "Esc".to_string()
        } else {
            format!("Esc / {}", sub)
        }
    };
    let title_text = {
        let t = CFG_TITLE.lock().unwrap();
        if t.is_empty() {
            let lang = CFG_LANG.lock().unwrap().clone();
            let locale = crate::gui::locale::LocaleText::get(&lang);
            locale.text_input_placeholder.to_string()
        } else {
            t.clone()
        }
    };

    format!(
        r#"<!DOCTYPE html>
<html {theme_attr}>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <style>{font_css}</style>
    <style id="theme-style">{css}</style>
</head>
<body>
    <div class="editor-container">
        <div class="header" id="headerRegion">
            <span class="header-title" id="headerTitle">{title_text}</span>
            <div class="close-btn" id="closeBtn" title="Close">
                {close_svg}
            </div>
        </div>

        <textarea id="editor" placeholder="{placeholder}" autofocus></textarea>

        <div class="btn-container">
            <button class="mic-btn" id="micBtn" title="Speech to text">
                {mic_svg}
            </button>
            <button class="send-btn" id="sendBtn" title="Send">
                {send_svg}
            </button>
        </div>

        <div class="footer" id="footerRegion">
            {submit_txt}  |  {newline_txt}  |  {cancel_hint} {cancel_txt}
        </div>
    </div>
    <script>
        const container = document.querySelector('.editor-container');
        const editor = document.getElementById('editor');
        const closeBtn = document.getElementById('closeBtn');
        const micBtn = document.getElementById('micBtn');
        const sendBtn = document.getElementById('sendBtn');

        // Drag window logic - Entire container except interactive elements
        container.addEventListener('mousedown', (e) => {{
            const isInteractive = e.target.closest('#editor') ||
                                e.target.closest('.close-btn') ||
                                e.target.closest('.mic-btn') ||
                                e.target.closest('.send-btn');
            if (isInteractive) return;

            // Only left click
            if (e.button === 0) {{
                window.ipc.postMessage('drag_window');
            }}
        }});

        // Close button
        closeBtn.addEventListener('click', (e) => {{
            window.ipc.postMessage('close_window');
        }});

        window.onload = () => {{
            setTimeout(() => editor.focus(), 50);
        }};

        // ... keydown handles ...
        editor.addEventListener('keydown', (e) => {{
            if (e.key === 'Enter' && !e.shiftKey) {{
                e.preventDefault();
                const text = editor.value.trim();
                if (text) {{
                    window.ipc.postMessage('submit:' + text);
                }}
            }}

            if (e.key === 'Escape') {{
                e.preventDefault();
                window.ipc.postMessage('cancel');
            }}

            if (e.key === 'ArrowUp') {{
                const isSingleLine = !editor.value.includes('\n');
                if ((isSingleLine || editor.selectionStart === 0) && !e.shiftKey) {{
                    e.preventDefault();
                    window.ipc.postMessage('history_up:' + editor.value);
                }}
            }}

            if (e.key === 'ArrowDown') {{
                const isSingleLine = !editor.value.includes('\n');
                if ((isSingleLine || editor.selectionStart === editor.value.length) && !e.shiftKey) {{
                    e.preventDefault();
                    window.ipc.postMessage('history_down:' + editor.value);
                }}
            }}
        }});

        micBtn.addEventListener('click', (e) => {{
            e.preventDefault();
            window.ipc.postMessage('mic');
        }});

        sendBtn.addEventListener('click', (e) => {{
            e.preventDefault();
            const text = editor.value.trim();
            if (text) {{
                window.ipc.postMessage('submit:' + text);
            }}
        }});

        document.addEventListener('contextmenu', e => e.preventDefault());

        window.setEditorText = (text) => {{
            editor.value = text;
            editor.selectionStart = editor.selectionEnd = text.length;
            editor.focus();
        }};

        window.updateTheme = (isDark) => {{
            document.documentElement.setAttribute('data-theme', isDark ? 'dark' : 'light');
        }};

        window.playEntry = () => {{
            const el = document.querySelector('.editor-container');
            if(el) {{
                el.classList.remove('exiting');
                el.classList.add('entering');

                // Trigger wave animation on title characters
                const title = document.getElementById('headerTitle');
                if (title && !title.hasAttribute('data-wrapped')) {{
                    const text = title.innerText;
                    title.innerHTML = text.split('').map((char, i) =>
                        `<span style="animation: waveColor 0.6s ease forwards ${{0.2 + i * 0.05}}s">${{char === ' ' ? '&nbsp;' : char}}</span>`
                    ).join('');
                    title.setAttribute('data-wrapped', 'true');
                }} else if (title) {{
                    // Re-trigger animation by removing/adding spans or class
                     const text = title.innerText; // Get raw text back from spans
                     title.innerHTML = text.split('').map((char, i) =>
                        `<span style="animation: waveColor 0.6s ease forwards ${{0.2 + i * 0.05}}s">${{char === ' ' ? '&nbsp;' : char}}</span>`
                    ).join('');
                }}
            }}
        }};

        window.playExit = () => {{
            const el = document.querySelector('.editor-container');
            if(el) {{
                el.classList.remove('entering');
                el.classList.add('exiting');
            }}
        }};

    </script>
</body>
</html>"#,
        theme_attr = theme_attr,
        font_css = font_css,
        css = css,
        title_text = title_text,
        placeholder = escaped_placeholder,
        submit_txt = submit_txt,
        newline_txt = newline_txt,
        cancel_hint = cancel_hint,
        cancel_txt = cancel_txt,
        close_svg = crate::overlay::html_components::icons::get_icon_svg("close"),
        mic_svg = crate::overlay::html_components::icons::get_icon_svg("mic"),
        send_svg = crate::overlay::html_components::icons::get_icon_svg("send")
    )
}
