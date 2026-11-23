use crate::config::Preset;
use crate::gui::settings_ui::get_localized_preset_name;
use crate::overlay::html_components::icons::OverlayIcon;

mod css;
pub use css::generate_panel_css;

pub fn generate_panel_html(
    presets: &[Preset],
    lang: &str,
    is_dark: bool,
    keep_open: bool,
) -> String {
    let css = generate_panel_css(is_dark);
    let favorites_html = get_favorite_presets_html(presets, lang, is_dark);
    let keep_open_label = crate::gui::locale::LocaleText::get(lang)
        .shell
        .favorites_keep_open;
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

/// Preset-type icons in the favorites list (the Android bubble uses the same size).
const FAVORITE_ICON_PX: u32 = 20;

pub fn get_favorite_presets_html(presets: &[Preset], lang: &str, is_dark: bool) -> String {
    let mut html_items = String::new();

    // Typing preset -> "keyboard"; text-selection preset -> "format_italic"
    // (matches the desktop sidebar + the Android app's icon choices).
    let icon_image = OverlayIcon::Image.svg_at(FAVORITE_ICON_PX);
    let icon_text_type = OverlayIcon::Keyboard.svg_at(FAVORITE_ICON_PX);
    let icon_text_select = OverlayIcon::FormatItalic.svg_at(FAVORITE_ICON_PX);
    let icon_mic = OverlayIcon::Mic.svg_at(FAVORITE_ICON_PX);
    let icon_device = OverlayIcon::SpeakerGroup.svg_at(FAVORITE_ICON_PX);
    for (idx, preset) in presets.iter().enumerate() {
        if preset.is_favorite && !preset.is_upcoming {
            let name = if preset.id.starts_with("preset_") {
                get_localized_preset_name(&preset.id, lang)
            } else {
                preset.name.clone()
            };

            let (icon_svg, color_hex) = match preset.preset_type.as_str() {
                "audio" => {
                    if preset.audio_source == "device" {
                        // Device/Speaker: Orange
                        (&icon_device, if is_dark { "#ffaa33" } else { "#f57c00" })
                    } else {
                        // Mic: Orange
                        (&icon_mic, if is_dark { "#ffaa33" } else { "#f57c00" })
                    }
                }
                "text" => {
                    // Text: Green
                    let c = if is_dark { "#55ff88" } else { "#388e3c" };
                    if preset.text_input_mode == "select" {
                        (&icon_text_select, c)
                    } else {
                        (&icon_text_type, c)
                    }
                }
                _ => (&icon_image, if is_dark { "#44ccff" } else { "#1976d2" }), // Image: Blue
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
            html_escape(locale.shell.favorites_empty)
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
    crate::overlay::utils::escape_js_double_quoted(text)
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
let bubbleCX = 0;
let bubbleCY = 0;

window.updateBubbleCenter = function(bx, by) {
    bubbleCX = bx;
    bubbleCY = by;
};

function animateIn(bx, by) {
    bubbleCX = bx;
    bubbleCY = by;

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

    // Recalculate --dx/--dy toward the CURRENT bubble center
    items.forEach((item, i) => {
        const rect = item.getBoundingClientRect();
        if (rect.width > 0) {
            const ix = rect.left + rect.width / 2;
            const iy = rect.top + rect.height / 2;
            item.style.setProperty('--dx', (bubbleCX - ix) + 'px');
            item.style.setProperty('--dy', (bubbleCY - iy) + 'px');
        }

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

#[cfg(test)]
mod tests {
    use super::{FAVORITE_ICON_PX, get_favorite_presets_html};
    use crate::config::Preset;
    use crate::overlay::html_components::icons::{OverlayIcon, assert_only_catalog_icons};

    fn favorite(preset_type: &str, audio_source: &str, text_input_mode: &str) -> Preset {
        Preset {
            name: format!("{preset_type} {audio_source}{text_input_mode}"),
            preset_type: preset_type.to_owned(),
            audio_source: audio_source.to_owned(),
            text_input_mode: text_input_mode.to_owned(),
            is_favorite: true,
            ..Preset::default()
        }
    }

    #[test]
    fn each_preset_type_shows_its_catalog_symbol() {
        for (preset, icon) in [
            (favorite("image", "", ""), OverlayIcon::Image),
            (favorite("text", "", "type"), OverlayIcon::Keyboard),
            (favorite("text", "", "select"), OverlayIcon::FormatItalic),
            (favorite("audio", "mic", ""), OverlayIcon::Mic),
            (favorite("audio", "device", ""), OverlayIcon::SpeakerGroup),
        ] {
            let html = get_favorite_presets_html(&[preset], "en", true);
            assert_only_catalog_icons(&html);
            assert!(html.contains(&icon.svg_at(FAVORITE_ICON_PX)), "{icon:?}");
        }
    }
}
