// HTML template generation for the tray popup WebView

use crate::APP;

use super::html::{
    get_popup_labels, get_restore_options, render_restore_options_html, restore_flyout_top_logical,
};
use super::{BASE_POPUP_WIDTH, POPUP_SURFACE_INSET, RESTORE_FLYOUT_GAP, RESTORE_FLYOUT_WIDTH};

pub(super) fn generate_popup_html() -> String {
    use crate::config::ThemeMode;

    let mut ui_language = String::from("en");
    let (
        settings_text,
        bubble_text,
        stop_tts_text,
        restore_overlay_text,
        quit_text,
        bubble_checked,
        is_dark_mode,
    ) = if let Ok(app) = APP.lock() {
        ui_language = app.config.ui_language.clone();
        let (settings, bubble, stop_tts, restore_overlay, quit) =
            get_popup_labels(&app.config.ui_language);
        let checked = app.config.show_favorite_bubble;

        // Theme detection
        let is_dark = match app.config.theme_mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
        };

        (
            settings,
            bubble,
            stop_tts,
            restore_overlay,
            quit,
            checked,
            is_dark,
        )
    } else {
        (
            "Settings",
            "Favorite Bubble",
            "Stop All TTS",
            "Restore Last Closed Overlay",
            "Quit",
            false,
            true,
        )
    };

    // Check if TTS has pending audio
    let has_tts_pending = crate::api::tts::TTS_MANAGER.has_pending_audio();

    // Define Colors based on theme
    let (bg_color, text_color, hover_color, border_color, separator_color) = if is_dark_mode {
        (
            "#2c2c2c",
            "#ffffff",
            "#3c3c3c",
            "#454545",
            "rgba(255,255,255,0.08)",
        )
    } else {
        (
            "#f9f9f9",
            "#1a1a1a",
            "#eaeaea",
            "#dcdcdc",
            "rgba(0,0,0,0.06)",
        )
    };

    let check_mark = if bubble_checked {
        r#"<svg class="check-icon" viewBox="0 0 16 16" fill="currentColor"><path d="M13.86 3.66a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.6 9.03a.75.75 0 1 1 1.06-1.06l2.42 2.42 6.72-6.72a.75.75 0 0 1 1.06 0z"/></svg>"#
    } else {
        ""
    };

    let active_class = if bubble_checked { "active" } else { "" };

    let stop_tts_disabled_class = if has_tts_pending { "" } else { "disabled" };
    let restore_overlay_disabled_class = if crate::overlay::result::can_restore_last_closed() {
        ""
    } else {
        "disabled"
    };
    let restore_options = get_restore_options(&ui_language);
    let restore_options_html = render_restore_options_html(&restore_options);
    let restore_flyout_top = restore_flyout_top_logical(restore_options.len());

    // Get font CSS to preload fonts into WebView2 cache (tray popup warms up first)
    let font_css = crate::overlay::html_components::font_manager::get_font_css();

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>
{font_css}
:root {{
    --bg-color: {bg};
    --text-color: {text};
    --hover-bg: {hover};
    --border-color: {border};
    --separator-color: {separator};
}}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
html, body {{
    width: 100%;
    height: 100%;
    overflow: visible;
    background: transparent;
    font-family: 'Google Sans Flex', 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif;
    font-variation-settings: 'ROND' 100;
    user-select: none;
    color: var(--text-color);
}}

.container {{
    position: relative;
    width: 100%;
    height: 100%;
    padding: {popup_surface_inset}px;
    overflow: visible;
}}

.menu-panel {{
    display: flex;
    flex-direction: column;
    width: {base_popup_width}px;
    height: 186px;
    padding: 4px;
    background: var(--bg-color);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    overflow: visible;
}}

.menu-item {{
    display: flex;
    align-items: center;
    padding: 6px 10px;
    border-radius: 4px;
    cursor: default;
    font-size: 13px;
    margin-bottom: 2px;
    background: transparent;
    transition: background 0.1s ease;
    height: 32px;
}}

.menu-item:hover {{
    background: var(--hover-bg);
}}

.icon {{
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    margin-right: 12px;
    opacity: 0.8;
}}

.label {{
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    padding-bottom: 1px; /* Visual alignment */
}}

.check {{
    display: flex;
    align-items: center;
    justify-content: center;
    width: 0;
    flex: 0 0 0;
    margin-left: 0;
}}

.separator {{
    height: 1px;
    background: var(--separator-color);
    margin: 4px 10px;
}}

svg {{
    width: 16px;
    height: 16px;
}}


.bubble-item .label {{
    transition: font-variation-settings 0.4s cubic-bezier(0.33, 1, 0.68, 1);
    font-variation-settings: 'wght' 400, 'wdth' 100, 'ROND' 100;
}}
.bubble-item .check {{
    width: 16px;
    flex: 0 0 16px;
    margin-left: 8px;
}}
.bubble-item.active .label {{
    font-variation-settings: 'wght' 700, 'wdth' 96, 'ROND' 100;
    color: var(--text-color);
}}

.restore-item {{
    position: relative;
    margin-bottom: 0;
}}

.restore-item .label {{
    font-variation-settings: 'wght' 420, 'wdth' 78, 'ROND' 100;
}}

.restore-chevron {{
    display: flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    flex: 0 0 14px;
    margin-left: 8px;
    opacity: 0.64;
    transition: transform 0.12s ease, opacity 0.12s ease;
}}

.restore-chevron svg {{
    width: 14px;
    height: 14px;
}}

.restore-item.expanded {{
    background: var(--hover-bg);
}}

.restore-item.expanded .restore-chevron {{
    transform: translateX(1px);
    opacity: 0.92;
}}

.restore-flyout {{
    display: none;
    position: absolute;
    left: {restore_flyout_left}px;
    flex-direction: column;
    width: {restore_flyout_width}px;
    padding: 4px;
    background: var(--bg-color);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    box-shadow: 0 14px 28px rgba(0, 0, 0, 0.18);
    z-index: 5;
}}

.restore-flyout.visible {{
    display: flex;
}}

.restore-option {{
    display: flex;
    align-items: center;
    min-height: 28px;
    padding: 5px 12px;
    border-radius: 4px;
    cursor: default;
    transition: background 0.1s ease;
}}

.restore-option:hover {{
    background: var(--hover-bg);
}}

.restore-option-label {{
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12px;
    font-variation-settings: 'wght' 420, 'wdth' 82, 'ROND' 100;
    padding-bottom: 1px;
}}

.menu-item.disabled {{
    opacity: 0.4;
    pointer-events: none;
}}

.restore-flyout.disabled {{
    opacity: 0.4;
    pointer-events: none;
}}
</style>
</head>
<body>
<div class="container">
    <div class="menu-panel" id="menu-panel">
    <div class="menu-item" onclick="action('settings')">
        <div class="icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.09a2 2 0 0 1-1-1.74v-.47a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.39a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"></path>
                <circle cx="12" cy="12" r="3"></circle>
            </svg>
        </div>
        <div class="label">{settings}</div>
        <div class="check"></div>
    </div>

    <div class="menu-item bubble-item {active_class}" data-state="{active_class}" onclick="action('bubble')">
        <div class="icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>
        </div>
        <div class="label">{bubble}</div>
        <div class="check" id="bubble-check-container">{check}</div>
    </div>

    <div class="menu-item {stop_tts_disabled}" id="stop-tts-item" onclick="action('stop_tts')">
        <div class="icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5L6 9H2v6h4l5 4V5z"/><line x1="23" y1="9" x2="17" y2="15"/><line x1="17" y1="9" x2="23" y2="15"/></svg>
        </div>
        <div class="label">{stop_tts}</div>
        <div class="check"></div>
    </div>

        <div class="menu-item restore-item {restore_overlay_disabled}" id="restore-overlay-item" onmouseenter="setRestoreMenuExpanded(true)" onmouseleave="setRestoreMenuExpanded(false)">
            <div class="icon">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7"/><polyline points="3 3 3 9 9 9"/></svg>
            </div>
            <div class="label">{restore_overlay}</div>
            <div class="restore-chevron" id="restore-chevron">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.25" stroke-linecap="round" stroke-linejoin="round"><polyline points="9 6 15 12 9 18"/></svg>
            </div>
        </div>

    <div class="separator"></div>

    <div class="menu-item" onclick="action('quit')">
        <div class="icon">
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/><polyline points="16 17 21 12 16 7"/><line x1="21" y1="12" x2="9" y2="12"/></svg>
        </div>
        <div class="label">{quit}</div>
        <div class="check"></div>
    </div>
    </div>
    <div class="restore-flyout {restore_overlay_disabled}" id="restore-flyout" style="top: {restore_flyout_top}px" onmouseenter="setRestoreMenuExpanded(true)" onmouseleave="setRestoreMenuExpanded(false)">{restore_options_html}</div>
</div>
<script>
window.ignoreBlur = false;
window.restoreExpanded = false;
window.restoreHideTimer = null;

function collapseRestoreMenu() {{
    window.restoreExpanded = false;
    if (window.restoreHideTimer) {{
        clearTimeout(window.restoreHideTimer);
        window.restoreHideTimer = null;
    }}
    const item = document.getElementById('restore-overlay-item');
    const flyout = document.getElementById('restore-flyout');
    if (item) {{
        item.classList.remove('expanded');
    }}
    if (flyout) {{
        flyout.classList.remove('visible');
    }}
}}

function positionRestoreMenu(topPx) {{
    const flyout = document.getElementById('restore-flyout');
    if (!flyout) {{
        return;
    }}
    flyout.style.top = topPx + 'px';
}}

function renderRestoreOptions(options) {{
    const menu = document.getElementById('restore-flyout');
    if (!menu) {{
        return;
    }}

    menu.innerHTML = '';
    for (const option of options) {{
        const item = document.createElement('div');
        item.className = 'restore-option';
        item.onclick = function() {{
            action('restore_recent:' + option.batch_count);
        }};

        const label = document.createElement('div');
        label.className = 'restore-option-label';
        label.textContent = option.label;
        item.appendChild(label);
        menu.appendChild(item);
    }}
}}

function setRestoreMenuExpanded(expanded) {{
    const item = document.getElementById('restore-overlay-item');
    const flyout = document.getElementById('restore-flyout');
    if (!item || !flyout) {{
        return;
    }}
    if (expanded && item.classList.contains('disabled')) {{
        return;
    }}
    if (expanded) {{
        if (window.restoreHideTimer) {{
            clearTimeout(window.restoreHideTimer);
            window.restoreHideTimer = null;
        }}
        if (!window.restoreExpanded) {{
            window.restoreExpanded = true;
            item.classList.add('expanded');
            flyout.classList.add('visible');
        }}
    }} else {{
        if (window.restoreHideTimer) {{
            clearTimeout(window.restoreHideTimer);
        }}
        window.restoreHideTimer = setTimeout(function() {{
            window.restoreHideTimer = null;
            if (!window.restoreExpanded) {{
                return;
            }}
            window.restoreExpanded = false;
            item.classList.remove('expanded');
            flyout.classList.remove('visible');
        }}, 90);
    }}
}}

function action(cmd) {{
    if (cmd === 'bubble') {{
        window.ignoreBlur = true;
        setTimeout(function() {{ window.ignoreBlur = false; }}, 1200);
        const el = document.querySelector('.bubble-item');
        if (el) {{
            if (el.classList.contains('active')) {{
                el.classList.remove('active');
            }} else {{
                el.classList.add('active');
            }}
        }}
    }} else if (cmd.startsWith('restore_recent:')) {{
        collapseRestoreMenu();
    }}
    window.ipc.postMessage(cmd);
}}

// Update popup state without reloading (preserves font cache)
window.updatePopupState = function(config) {{
    // Update CSS variables for theme
    document.documentElement.style.setProperty('--bg-color', config.bgColor);
    document.documentElement.style.setProperty('--text-color', config.textColor);
    document.documentElement.style.setProperty('--hover-bg', config.hoverColor);
    document.documentElement.style.setProperty('--border-color', config.borderColor);
    document.documentElement.style.setProperty('--separator-color', config.separatorColor);

    // Update label texts for language changes
    const labels = document.querySelectorAll('.menu-item .label');
    if (labels.length >= 5) {{
        labels[0].textContent = config.settingsText;
        labels[1].textContent = config.bubbleText;
        labels[2].textContent = config.stopTtsText;
        labels[3].textContent = config.restoreOverlayText;
        labels[4].textContent = config.quitText;
    }}

    // Update bubble active state
    const bubbleItem = document.querySelector('.bubble-item');
    if (bubbleItem) {{
        if (config.bubbleActive) {{
            bubbleItem.classList.add('active');
            document.getElementById('bubble-check-container').innerHTML = '<svg class="check-icon" viewBox="0 0 16 16" fill="currentColor"><path d="M13.86 3.66a.75.75 0 0 1 0 1.06l-7.25 7.25a.75.75 0 0 1-1.06 0L2.6 9.03a.75.75 0 1 1 1.06-1.06l2.42 2.42 6.72-6.72a.75.75 0 0 1 1.06 0z"/></svg>';
        }} else {{
            bubbleItem.classList.remove('active');
            document.getElementById('bubble-check-container').innerHTML = '';
        }}
    }}

    // Update stop TTS disabled state
    const stopTtsItem = document.getElementById('stop-tts-item');
    if (stopTtsItem) {{
        if (config.ttsDisabled) {{
            stopTtsItem.classList.add('disabled');
        }} else {{
            stopTtsItem.classList.remove('disabled');
        }}
    }}

    renderRestoreOptions(config.restoreOptions);
    positionRestoreMenu(config.restoreFlyoutTop);
    collapseRestoreMenu();

    const restoreOverlayItem = document.getElementById('restore-overlay-item');
    const restoreFlyout = document.getElementById('restore-flyout');
    if (restoreOverlayItem) {{
        if (config.restoreDisabled) {{
            restoreOverlayItem.classList.add('disabled');
        }} else {{
            restoreOverlayItem.classList.remove('disabled');
        }}
    }}
    if (restoreFlyout) {{
        if (config.restoreDisabled) {{
            restoreFlyout.classList.add('disabled');
        }} else {{
            restoreFlyout.classList.remove('disabled');
        }}
    }}
}};

window.addEventListener('blur', function() {{
    if (window.ignoreBlur) return;
    collapseRestoreMenu();
    window.ipc.postMessage('close');
}});
</script>
</body>
</html>"#,
        bg = bg_color,
        text = text_color,
        hover = hover_color,
        border = border_color,
        separator = separator_color,
        base_popup_width = BASE_POPUP_WIDTH,
        settings = settings_text,
        bubble = bubble_text,
        stop_tts = stop_tts_text,
        stop_tts_disabled = stop_tts_disabled_class,
        restore_overlay = restore_overlay_text,
        restore_overlay_disabled = restore_overlay_disabled_class,
        restore_flyout_left = BASE_POPUP_WIDTH + RESTORE_FLYOUT_GAP,
        restore_flyout_top = restore_flyout_top,
        restore_flyout_width = RESTORE_FLYOUT_WIDTH,
        restore_options_html = restore_options_html,
        popup_surface_inset = POPUP_SURFACE_INSET,
        quit = quit_text,
        check = check_mark
    )
}
