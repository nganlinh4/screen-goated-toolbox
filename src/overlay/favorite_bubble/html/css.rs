//! Theme stylesheet for the favorites panel.

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
            "rgba(64, 196, 255, 0.9)", // Bright cyan
            "rgba(20, 20, 30, 0.85)",  // Match item_bg
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
            "rgba(33, 100, 200, 0.9)",   // Deeper blue for light mode
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

.preset-item.blooming, .empty.blooming {{
    animation: bloom 0.4s cubic-bezier(0.2, 0.8, 0.2, 1) forwards;
}}

.preset-item.retreating, .empty.retreating {{
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
