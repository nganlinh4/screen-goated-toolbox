use crate::config::Preset;
use crate::gui::settings_ui::get_localized_preset_name;

pub fn generate_panel_html(
    presets: &[Preset],
    lang: &str,
    is_dark: bool,
    keep_open: bool,
) -> String {
    let css = generate_panel_css(is_dark);
    let favorites_html = get_favorite_presets_html(presets, lang, is_dark);
    let keep_open_label = crate::gui::locale::LocaleText::get(lang).favorites_keep_open;
    let keep_open_js = if keep_open { "true" } else { "false" };
    let keep_open_class = if keep_open { " active" } else { "" };
    let js = get_js();

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>
{css}
</style>
</head>
<body>
<div class="container">
    <div class="keep-open-row visible" id="keepOpenRow">
        <span class="keep-open-label{keep_open_class}" id="keepOpenLabel" onclick="toggleKeepOpen()">{keep_open_label}</span>
        <div class="size-pill"><button class="size-btn" onclick="resizeBubble('desc')">-</button><button class="size-btn" onclick="resizeBubble('inc')">+</button></div>
    </div>
    <div class="list">{favorites}</div>
</div>
<script>
{js}
keepOpen = {keep_open_js};
</script>
</body>
</html>"#,
        css = css,
        favorites = favorites_html,
        keep_open_label = keep_open_label,
        keep_open_class = keep_open_class,
        keep_open_js = keep_open_js,
        js = js
    )
}

pub fn generate_panel_css(is_dark: bool) -> String {
    let font_css = crate::overlay::html_components::font_manager::get_font_css();

    // Theme-specific colors
    let (
        text_color,
        item_bg,
        item_hover_bg,
        item_shadow,
        item_hover_shadow,
        empty_text_color,
        empty_bg,
        empty_border,
        label_color,
        toggle_bg,
        toggle_active_bg,
        row_bg,
    ) = if is_dark {
        (
            "#eeeeee",
            "rgba(20, 20, 30, 0.85)",
            "rgba(40, 40, 55, 0.95)",
            "0 2px 8px rgba(0, 0, 0, 0.2)",
            "0 4px 12px rgba(0, 0, 0, 0.3)",
            "rgba(255, 255, 255, 0.6)",
            "rgba(20, 20, 30, 0.85)",
            "rgba(255, 255, 255, 0.1)",
            "rgba(255, 255, 255, 0.6)",
            "rgba(60, 60, 70, 0.8)",
            "rgba(64, 196, 255, 0.9)", // Blue (Light Blue A200)
            "rgba(20, 20, 30, 0.85)", // Match item_bg
        )
    } else {
        // Light mode colors
        (
            "#222222",
            "rgba(255, 255, 255, 0.92)",
            "rgba(240, 240, 245, 0.98)",
            "0 2px 8px rgba(0, 0, 0, 0.08)",
            "0 4px 12px rgba(0, 0, 0, 0.12)",
            "rgba(0, 0, 0, 0.5)",
            "rgba(255, 255, 255, 0.92)",
            "rgba(0, 0, 0, 0.08)",
            "rgba(0, 0, 0, 0.6)",
            "rgba(200, 200, 210, 0.8)",
            "rgba(33, 150, 243, 0.9)", // Blue (Material Blue 500)
            "rgba(255, 255, 255, 0.92)", // Match item_bg
        )
    };

    // Light mode needs adjusted border color for hover
    let item_hover_border = if is_dark {
        "rgba(255, 255, 255, 0.25)"
    } else {
        "rgba(0, 0, 0, 0.12)"
    };

    format!(
        r#"
{font_css}
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
html, body {{
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: transparent;
    font-family: 'Google Sans Flex', 'Segoe UI', system-ui, sans-serif;
    user-select: none;
}}

.container {{
    display: flex;
    flex-direction: column;
    padding: 30px 20px;
    /* Ensure container has height for hover detection even if items are small */
    min-height: 100px; 
}}
.container.side-right {{ padding-left: 30px; padding-right: 10px; }}
.container.side-left {{ padding-left: 10px; padding-right: 30px; }}

.list {{
    display: block;
    column-gap: 8px;
}}

/* THE PRESET ITEM */
.preset-item, .empty {{
    display: flex;
    align-items: center;
    padding: 8px 12px;
    border-radius: 12px;
    cursor: pointer;
    color: {text_color};
    font-size: 13px;
    font-variation-settings: 'wght' 500, 'wdth' 100, 'ROND' 100;
    background: {item_bg};
    backdrop-filter: blur(12px);
    box-shadow: {item_shadow};
    margin-bottom: 4px;
    break-inside: avoid;
    
    /* INITIAL STATE: Hidden */
    opacity: 0; 
    transform: scale(0.95);
    will-change: transform, opacity;
    
    /* Defaults */
    --dx: 0px;
    --dy: 0px;
}}

/* BLOOM ANIMATION (Enter) */
@keyframes bloom {{
    0% {{
        opacity: 0;
        transform: translate(var(--dx), var(--dy)) scale(0.1);
    }}
    60% {{
        opacity: 1;
    }}
    100% {{
        opacity: 1;
        transform: translate(0, 0) scale(1);
    }}
}}

/* RETREAT ANIMATION (Exit) */
@keyframes retreat {{
    0% {{
        opacity: 1;
        transform: translate(0, 0) scale(1);
    }}
    100% {{
        opacity: 0;
        transform: translate(var(--dx), var(--dy)) scale(0.1);
    }}
}}

.preset-item.blooming {{
    animation: bloom 0.4s cubic-bezier(0.2, 0.8, 0.2, 1) forwards;
}}

.preset-item.retreating {{
    /* 'both' is CRITICAL here: it makes the element stick to the 0% keyframe 
       (opacity: 1) during the animation-delay, preventing the blink */
    animation: retreat 0.35s cubic-bezier(0.4, 0, 1, 1) both;
}}

/* HOVER EFFECT */
.preset-item.animate-done:hover {{
    background: {item_hover_bg};
    border-color: {item_hover_border};
    box-shadow: {item_hover_shadow};
    font-variation-settings: 'wght' 650, 'wdth' 105, 'ROND' 100;
    transform: scale(1.03);
    transition: all 0.1s ease-out;
}}

.preset-item.animate-done:active {{
    transform: scale(0.98);
}}

/* Keep Open Row - HOVER VISIBILITY FIX */
.keep-open-row {{
    display: flex; align-items: center; justify-content: center; gap: 12px;
    padding: 8px 16px; margin-bottom: 12px; background: {row_bg};
    backdrop-filter: blur(12px); box-shadow: {item_shadow}; border-radius: 20px;
    width: fit-content; margin-left: auto; margin-right: auto;
    
    /* Initially hidden & offset */
    opacity: 0;
    transform: translateY(15px) scale(0.95);
    pointer-events: none;
    
    /* Smooth transition for hover state */
    transition: 
        opacity 0.3s cubic-bezier(0.2, 0.8, 0.2, 1),
        transform 0.3s cubic-bezier(0.2, 0.8, 0.2, 1);
}}

/* Only visible when hovering the container */
.container:hover .keep-open-row {{
    opacity: 1;
    transform: translateY(0) scale(1);
    pointer-events: auto;
}}

/* Hide keep-open-row during close animation (prevents flicker when hovering transparent window) */
.container.closing .keep-open-row,
.container.closing:hover .keep-open-row {{
    opacity: 0;
    transform: translateY(15px) scale(0.95);
    pointer-events: none;
}}

.preset-item {{ position: relative; overflow: hidden; }}
.progress-fill {{ position: absolute; top: 0; left: 0; width: 0%; height: 100%; background: rgba(64, 196, 255, 0.3); pointer-events: none; z-index: 0; transition: width 0.05s linear; }}
.preset-item .icon, .preset-item .name {{ position: relative; z-index: 1; }}
.icon {{ display: flex; align-items: center; margin-right: 10px; opacity: 0.9; }}
.name {{ flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }}
.empty {{ color: {empty_text_color}; text-align: center; padding: 12px; background: {empty_bg}; border: 1px solid {empty_border}; }}

.condense {{ letter-spacing: -0.5px; }}
.condense-more {{ letter-spacing: -1px; }}

.keep-open-label {{ color: {label_color}; font-size: 12px; font-variation-settings: 'wght' 500, 'wdth' 100; transition: all 0.2s; white-space: nowrap; cursor: pointer; padding: 4px 10px; border-radius: 10px; background: transparent; }}
.keep-open-label:hover {{ background: {toggle_bg}; }}
.keep-open-label.active {{ color: white; font-variation-settings: 'wght' 600, 'wdth' 105; background: {toggle_active_bg}; }}
.size-pill {{ display: flex; background: {item_bg}; border-radius: 10px; overflow: hidden; margin-left: 8px; }}
.size-btn {{ width: 22px; height: 20px; border: none; background: transparent; color: {text_color}; display: flex; align-items: center; justify-content: center; cursor: pointer; transition: background 0.2s; font-size: 14px; }}
.size-btn:hover {{ background: {item_hover_bg}; }}
"#,
        font_css = font_css,
        text_color = text_color,
        item_bg = item_bg,
        item_hover_bg = item_hover_bg,
        item_shadow = item_shadow,
        item_hover_shadow = item_hover_shadow,
        item_hover_border = item_hover_border,
        empty_text_color = empty_text_color,
        empty_bg = empty_bg,
        empty_border = empty_border,
        label_color = label_color,
        toggle_bg = toggle_bg,
        toggle_active_bg = toggle_active_bg,
        row_bg = row_bg
    )
}

pub fn get_favorite_presets_html(presets: &[Preset], lang: &str, is_dark: bool) -> String {
    let mut html_items = String::new();

    let icon_image = r#"<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M12 8.8a3.2 3.2 0 1 0 0 6.4 3.2 3.2 0 0 0 0-6.4z"/><path d="M9 2L7.17 4H4c-1.1 0-2 .9-2 2v12c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2h-3.17L15 2H9zm3 15c-2.76 0-5-2.24-5-5s2.24-5 5-5 5 2.24 5 5-2.24 5-5 5z"/></svg>"#;
    let icon_text_type = r#"<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M5 5h14v3h-2v-1h-3v10h2.5v2h-9v-2h2.5v-10h-3v1h-2z"/></svg>"#;
    let icon_text_select = r#"<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M4 7h11v1.5H4z M4 11h11v2.5H4z M4 15.5h11v1.5H4z M19 6h-2v1.5h0.5v9H17v1.5h2v-1.5h-0.5v-9H19z"/></svg>"#;
    let icon_mic = r#"<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M12 14c1.66 0 3-1.34 3-3V5c0-1.66-1.34-3-3-3S9 3.34 9 5v6c0 1.66 1.34 3 3 3zM17 11c0 2.76-2.24 5-5 5s-5-2.24-5-5H5c0 3.53 2.61 6.43 6 6.92V21h2v-3.08c3.39-.49 6-3.39 6-6.92h-2z"/></svg>"#;
    let icon_device = r#"<svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M3 9v6h4l5 5V4L7 9H3zm13.5 3c0-1.77-1.02-3.29-2.5-4.03v8.05c1.48-.73 2.5-2.25 2.5-4.02zM14 3.23v2.06c2.89.86 5 3.54 5 6.71s-2.11 5.85-5 6.71v2.06c4.01-.91 7-4.49 7-8.77s-2.99-7.86-7-8.77z"/></svg>"#;
    let icon_realtime = r#"<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12h3 l1.5-3 l2 10 l3.5-14 l3.5 10 l2-3 h4.5"/></svg>"#;

    for (idx, preset) in presets.iter().enumerate() {
        if preset.is_favorite && !preset.is_upcoming {
            let name = if preset.id.starts_with("preset_") {
                get_localized_preset_name(&preset.id, lang)
            } else {
                preset.name.clone()
            };

            let (icon_svg, color_hex) = match preset.preset_type.as_str() {
                "audio" => {
                    if preset.audio_processing_mode == "realtime" {
                        // Realtime/Live: Red
                        (icon_realtime, if is_dark { "#ff5555" } else { "#d32f2f" })
                    } else if preset.audio_source == "device" {
                        // Device/Speaker: Orange
                        (icon_device, if is_dark { "#ffaa33" } else { "#f57c00" })
                    } else {
                        // Mic: Orange
                        (icon_mic, if is_dark { "#ffaa33" } else { "#f57c00" })
                    }
                }
                "text" => {
                    // Text: Green
                    let c = if is_dark { "#55ff88" } else { "#388e3c" };
                    if preset.text_input_mode == "select" {
                        (icon_text_select, c)
                    } else {
                        (icon_text_type, c)
                    }
                }
                _ => (icon_image, if is_dark { "#44ccff" } else { "#1976d2" }), // Image: Blue
            };

            let item = format!(
                r#"<div class="preset-item" onmousedown="onMouseDown({})" onmouseup="onMouseUp({})" onmouseleave="onMouseLeave()"><div class="progress-fill"></div><span class="icon" style="color: {};">{}</span><span class="name">{}</span></div>"#,
                idx,
                idx,
                color_hex,
                icon_svg,
                html_escape(&name)
            );

            html_items.push_str(&item);
        }
    }

    if html_items.is_empty() {
        let locale = crate::gui::locale::LocaleText::get(lang);
        html_items = format!(
            r#"<div class="empty">{}</div>"#,
            html_escape(locale.favorites_empty)
        );
    }

    html_items
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn escape_js(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn get_js() -> &'static str {
    r#"
function select(idx) { window.ipc.postMessage('select:' + idx); }
function dismiss() { window.ipc.postMessage('dismiss'); }

function fitText() {
    requestAnimationFrame(() => {
        document.querySelectorAll('.name').forEach(el => {
            el.className = 'name';
            if (el.scrollWidth > el.clientWidth) {
                el.classList.add('condense');
                if (el.scrollWidth > el.clientWidth) {
                    el.classList.remove('condense');
                    el.classList.add('condense-more');
                }
            }
        });
        sendHeight();
    });
}
function resizeBubble(dir) {
    if (dir === 'inc') window.ipc.postMessage('increase_size');
    else window.ipc.postMessage('decrease_size');
}
window.onload = fitText;

function sendHeight() {
    const container = document.querySelector('.container');
    if (container) {
         window.ipc.postMessage('resize:' + Math.max(container.scrollHeight, container.offsetHeight));
    }
}

function startDrag(e) { if (e.button === 0) window.ipc.postMessage('drag'); }

// Re-assert bubble Z-order on any click interaction
document.addEventListener('mousedown', () => window.ipc.postMessage('focus_bubble'));

let keepOpen = false;
function toggleKeepOpen() {
    keepOpen = !keepOpen;
    const label = document.getElementById('keepOpenLabel');
    label.classList.toggle('active', keepOpen);
    window.ipc.postMessage('set_keep_open:' + (keepOpen ? '1' : '0'));
}

/* Mouse Logic */
let holdTimer = null;
const HOLD_THRESHOLD = 500;
function onMouseDown(idx) {
    const item = event.currentTarget;
    const fill = item.querySelector('.progress-fill');
    if (fill) {
        fill.style.width = '0%';
        fill.style.transition = 'width ' + HOLD_THRESHOLD + 'ms linear';
        requestAnimationFrame(() => fill.style.width = '100%');
    }
    holdTimer = setTimeout(() => {
        holdTimer = null;
        triggerContinuous(idx);
    }, HOLD_THRESHOLD);
}
function onMouseUp(idx) {
    if (holdTimer) {
        clearTimeout(holdTimer);
        holdTimer = null;
        triggerNormal(idx);
    }
    resetFill();
}
function onMouseLeave() {
    if (holdTimer) {
        clearTimeout(holdTimer);
        holdTimer = null;
    }
    resetFill();
}
function resetFill() {
    document.querySelectorAll('.progress-fill').forEach(f => {
        f.style.transition = 'none';
        f.style.width = '0%';
    });
}
function triggerNormal(idx) {
    if (keepOpen) window.ipc.postMessage('trigger_only:' + idx);
    else { closePanel(); window.ipc.postMessage('trigger:' + idx); }
}
function triggerContinuous(idx) {
    if (keepOpen) window.ipc.postMessage('trigger_continuous_only:' + idx);
    else { closePanel(); window.ipc.postMessage('trigger_continuous:' + idx); }
}

/* --- ANIMATION LOGIC --- */
let currentTimeout = null;
let currentSide = 'right';

function animateIn(bx, by) {
    if (currentTimeout) {
        clearTimeout(currentTimeout);
        currentTimeout = null;
    }

    // Remove closing class so keep-open-row can appear on hover
    const container = document.querySelector('.container');
    container.classList.remove('closing');

    const items = document.querySelectorAll('.preset-item, .empty');
    if (items.length === 0) return;

    // 1. BATCH READ: Calculate geometry
    const metrics = [];
    for(let i=0; i<items.length; i++) {
        const item = items[i];
        const rect = item.getBoundingClientRect();
        
        if (rect.width === 0) {
            metrics.push(null);
            continue;
        }

        const iy = rect.top + rect.height / 2;
        const ix = rect.left + rect.width / 2;
        const dx = bx - ix;
        const dy = by - iy;
        metrics.push({ dx, dy });
    }

    // 2. BATCH WRITE: Apply vars and animate
    requestAnimationFrame(() => {
        items.forEach((item, i) => {
            const m = metrics[i];
            if (!m) return;

            // Reset state
            item.classList.remove('retreating', 'animate-done');
            
            // Set variables for the shader
            item.style.setProperty('--dx', m.dx + 'px');
            item.style.setProperty('--dy', m.dy + 'px');
            
            // Stagger
            item.style.animationDelay = (i * 10) + 'ms';
            
            // Trigger
            item.classList.add('blooming');
            
            // Cleanup
            setTimeout(() => {
                item.classList.add('animate-done');
            }, 400 + (i * 10));
        });
        // Note: KeepOpenRow visibility handled purely by CSS hover now
    });
}

function closePanel() {
    if (currentTimeout) clearTimeout(currentTimeout);

    // Add closing class to prevent keep-open-row from appearing on hover
    const container = document.querySelector('.container');
    container.classList.add('closing');

    const items = Array.from(document.querySelectorAll('.preset-item, .empty'));

    items.forEach((item, i) => {
        // Reverse stagger
        item.style.animationDelay = ((items.length - 1 - i) * 6) + 'ms';
        
        // Remove 'animate-done' (which has hover effects) and 'blooming'
        item.classList.remove('blooming', 'animate-done');
        
        // Add retreating class. CSS 'animation-fill-mode: both' ensures 
        // it stays visible (opacity: 1) until the delay passes and animation starts.
        item.classList.add('retreating');
    });

    currentTimeout = setTimeout(() => {
        window.ipc.postMessage('close_now');
        currentTimeout = null;
    }, items.length * 6 + 350);
}

window.setSide = (side, bubbleOverlap) => {
    currentSide = side;
    const container = document.querySelector('.container');
    container.classList.remove('side-left', 'side-right');
    container.classList.add('side-' + side);

    // Set padding to account for bubble overlap area
    // Content should stay in the non-overlapping area
    if (side === 'right') {
        // Panel extends right behind bubble - add padding on right
        container.style.paddingLeft = '30px';
        container.style.paddingRight = (10 + bubbleOverlap) + 'px';
    } else {
        // Panel extends left behind bubble - add padding on left
        container.style.paddingLeft = (10 + bubbleOverlap) + 'px';
        container.style.paddingRight = '30px';
    }
};
"#
}
