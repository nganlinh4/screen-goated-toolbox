// HTML template generation for the tray popup WebView

use crate::APP;

use super::html::{
    get_popup_labels, get_restore_options, render_restore_options_html, restore_flyout_top_logical,
};
use super::{BASE_POPUP_WIDTH, POPUP_SURFACE_INSET, RESTORE_FLYOUT_GAP, RESTORE_FLYOUT_WIDTH};

pub(super) fn generate_popup_html() -> String {
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
        let is_dark = app.config.theme_mode.is_dark();

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
        super::BUBBLE_CHECK_SVG
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
            <svg viewBox="0 0 24 24" fill="currentColor"><path d="m9.25 22l-.4-3.2q-.325-.125-.612-.3t-.563-.375L4.7 19.375l-2.75-4.75l2.575-1.95Q4.5 12.5 4.5 12.338v-.675q0-.163.025-.338L1.95 9.375l2.75-4.75l2.975 1.25q.275-.2.575-.375t.6-.3l.4-3.2h5.5l.4 3.2q.325.125.613.3t.562.375l2.975-1.25l2.75 4.75l-2.575 1.95q.025.175.025.338v.674q0 .163-.05.338l2.575 1.95l-2.75 4.75l-2.95-1.25q-.275.2-.575.375t-.6.3l-.4 3.2zm2.8-6.5q1.45 0 2.475-1.025T15.55 12t-1.025-2.475T12.05 8.5q-1.475 0-2.488 1.025T8.55 12t1.013 2.475T12.05 15.5"/></svg>
        </div>
        <div class="label">{settings}</div>
        <div class="check"></div>
    </div>

    <div class="menu-item bubble-item {active_class}" data-state="{active_class}" onclick="action('bubble')">
        <div class="icon">
            <svg viewBox="0 0 24 24" fill="currentColor"><path d="m5.825 21l1.625-7.025L2 9.25l7.2-.625L12 2l2.8 6.625l7.2.625l-5.45 4.725L18.175 21L12 17.275z"/></svg>
        </div>
        <div class="label">{bubble}</div>
        <div class="check" id="bubble-check-container">{check}</div>
    </div>

    <div class="menu-item {stop_tts_disabled}" id="stop-tts-item" onclick="action('stop_tts')">
        <div class="icon">
            <svg viewBox="0 0 24 24" fill="currentColor"><path d="m19.8 22.6l-3.025-3.025q-.625.4-1.325.688t-1.45.462v-2.05q.35-.125.688-.25t.637-.3L12 14.8V20l-5-5H3V9h3.2L1.4 4.2l1.4-1.4l18.4 18.4zm-.2-5.8l-1.45-1.45q.425-.775.638-1.625t.212-1.75q0-2.35-1.375-4.2T14 5.275v-2.05q3.1.7 5.05 3.138T21 11.975q0 1.325-.363 2.55T19.6 16.8m-3.35-3.35L14 11.2V7.95q1.175.55 1.838 1.65T16.5 12q0 .375-.062.738t-.188.712M12 9.2L9.4 6.6L12 4z"/></svg>
        </div>
        <div class="label">{stop_tts}</div>
        <div class="check"></div>
    </div>

        <div class="menu-item restore-item {restore_overlay_disabled}" id="restore-overlay-item" onmouseenter="setRestoreMenuExpanded(true)" onmouseleave="setRestoreMenuExpanded(false)">
            <div class="icon">
                <svg viewBox="0 0 24 24" fill="currentColor"><path d="M11 20.95q-3.025-.375-5.012-2.637T4 13q0-1.65.65-3.162T6.5 7.2l1.425 1.425q-.95.85-1.437 1.975T6 13q0 2.2 1.4 3.888T11 18.95zm2 0v-2q2.175-.4 3.588-2.075T18 13q0-2.5-1.75-4.25T12 7h-.075l1.1 1.1l-1.4 1.4l-3.5-3.5l3.5-3.5l1.4 1.4l-1.1 1.1H12q3.35 0 5.675 2.325T20 13q0 3.025-1.987 5.288T13 20.95"/></svg>
            </div>
            <div class="label">{restore_overlay}</div>
            <div class="restore-chevron" id="restore-chevron">
                <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12.6 12L8 7.4L9.4 6l6 6l-6 6L8 16.6z"/></svg>
            </div>
        </div>

    <div class="separator"></div>

    <div class="menu-item" onclick="action('quit')">
        <div class="icon">
            <svg viewBox="0 0 24 24" fill="currentColor"><path d="M5 21q-.825 0-1.412-.587T3 19V5q0-.825.588-1.412T5 3h7v2H5v14h7v2zm11-4l-1.375-1.45l2.55-2.55H9v-2h8.175l-2.55-2.55L16 7l5 5z"/></svg>
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
            document.getElementById('bubble-check-container').innerHTML = '{bubble_check_svg}';
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
        check = check_mark,
        bubble_check_svg = super::BUBBLE_CHECK_SVG
    )
}
