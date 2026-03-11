pub(crate) fn generate_css(is_dark: bool) -> String {
    let (
        text_color,
        dismiss_bg,
        dismiss_border,
        dismiss_color,
        dismiss_hover_bg,
        dismiss_hover_border,
        item_border,
        item_hover_border,
        item_shadow,
    ) = if is_dark {
        (
            "#ffffff",
            "rgba(20, 20, 25, 0.75)",
            "rgba(255, 255, 255, 0.12)",
            "rgba(255, 180, 180, 0.85)",
            "rgba(60, 30, 30, 0.85)",
            "rgba(255, 150, 150, 0.4)",
            "rgba(255, 255, 255, 0.15)",
            "rgba(255, 255, 255, 0.5)",
            "0 5px 18px rgba(0, 0, 0, 0.35)",
        )
    } else {
        (
            "#222222",
            "rgba(255, 255, 255, 0.85)",
            "rgba(0, 0, 0, 0.1)",
            "rgba(180, 60, 60, 0.9)",
            "rgba(255, 220, 220, 0.95)",
            "rgba(200, 100, 100, 0.4)",
            "rgba(0, 0, 0, 0.12)",
            "rgba(0, 0, 0, 0.3)",
            "0 5px 18px rgba(0, 0, 0, 0.15)",
        )
    };

    let color_palette = if is_dark {
        r#"
.color-0  { background: rgba(30, 60, 110, 0.85); border-color: rgba(100, 150, 255, 0.3); }
.color-1  { background: rgba(35, 80, 45, 0.85);  border-color: rgba(100, 255, 120, 0.3); }
.color-2  { background: rgba(90, 30, 35, 0.85);  border-color: rgba(255, 100, 110, 0.3); }
.color-3  { background: rgba(70, 35, 90, 0.85);  border-color: rgba(200, 120, 255, 0.3); }
.color-4  { background: rgba(90, 60, 20, 0.85);  border-color: rgba(255, 180, 80, 0.3); }
.color-5  { background: rgba(20, 75, 85, 0.85);  border-color: rgba(80, 230, 255, 0.3); }
.color-6  { background: rgba(85, 30, 85, 0.85);  border-color: rgba(255, 100, 255, 0.3); }
.color-7  { background: rgba(30, 70, 100, 0.85); border-color: rgba(100, 200, 255, 0.3); }
.color-8  { background: rgba(65, 80, 20, 0.85);  border-color: rgba(200, 255, 80, 0.3); }
.color-9  { background: rgba(90, 20, 60, 0.85);  border-color: rgba(255, 80, 150, 0.3); }
.color-10 { background: rgba(20, 80, 70, 0.85);  border-color: rgba(80, 255, 200, 0.3); }
.color-11 { background: rgba(90, 50, 30, 0.85);  border-color: rgba(255, 140, 80, 0.3); }

.color-0.hovered  { background: rgba(50, 100, 180, 0.95); box-shadow: 0 0 15px rgba(60, 120, 255, 0.4); }
.color-1.hovered  { background: rgba(50, 140, 70, 0.95);  box-shadow: 0 0 15px rgba(80, 255, 100, 0.4); }
.color-2.hovered  { background: rgba(160, 50, 60, 0.95);  box-shadow: 0 0 15px rgba(255, 80, 90, 0.4); }
.color-3.hovered  { background: rgba(120, 60, 160, 0.95); box-shadow: 0 0 15px rgba(180, 100, 255, 0.4); }
.color-4.hovered  { background: rgba(160, 100, 40, 0.95); box-shadow: 0 0 15px rgba(255, 160, 60, 0.4); }
.color-5.hovered  { background: rgba(40, 130, 150, 0.95); box-shadow: 0 0 15px rgba(60, 220, 255, 0.4); }
.color-6.hovered  { background: rgba(150, 50, 150, 0.95); box-shadow: 0 0 15px rgba(255, 80, 255, 0.4); }
.color-7.hovered  { background: rgba(50, 120, 170, 0.95); box-shadow: 0 0 15px rgba(80, 180, 255, 0.4); }
.color-8.hovered  { background: rgba(110, 140, 40, 0.95); box-shadow: 0 0 15px rgba(180, 255, 60, 0.4); }
.color-9.hovered  { background: rgba(160, 40, 100, 0.95); box-shadow: 0 0 15px rgba(255, 60, 140, 0.4); }
.color-10.hovered { background: rgba(40, 140, 120, 0.95); box-shadow: 0 0 15px rgba(60, 255, 200, 0.4); }
.color-11.hovered { background: rgba(160, 80, 50, 0.95);  box-shadow: 0 0 15px rgba(255, 120, 60, 0.4); }"#
    } else {
        r#"
.color-0  { background: rgba(200, 220, 255, 0.95); }
.color-1  { background: rgba(200, 235, 200, 0.95); }
.color-2  { background: rgba(255, 210, 210, 0.95); }
.color-3  { background: rgba(230, 210, 255, 0.95); }
.color-4  { background: rgba(255, 230, 200, 0.95); }
.color-5  { background: rgba(200, 240, 240, 0.95); }
.color-6  { background: rgba(240, 210, 245, 0.95); }
.color-7  { background: rgba(210, 230, 250, 0.95); }
.color-8  { background: rgba(235, 235, 200, 0.95); }
.color-9  { background: rgba(255, 210, 235, 0.95); }
.color-10 { background: rgba(200, 245, 240, 0.95); }
.color-11 { background: rgba(255, 225, 210, 0.95); }

.color-0.hovered  { background: rgba(130, 180, 255, 0.98); }
.color-1.hovered  { background: rgba(130, 200, 130, 0.98); }
.color-2.hovered  { background: rgba(255, 150, 150, 0.98); }
.color-3.hovered  { background: rgba(190, 150, 255, 0.98); }
.color-4.hovered  { background: rgba(255, 190, 120, 0.98); }
.color-5.hovered  { background: rgba(100, 220, 220, 0.98); }
.color-6.hovered  { background: rgba(220, 150, 230, 0.98); }
.color-7.hovered  { background: rgba(140, 190, 255, 0.98); }
.color-8.hovered  { background: rgba(200, 200, 120, 0.98); }
.color-9.hovered  { background: rgba(255, 150, 200, 0.98); }
.color-10.hovered { background: rgba(80, 210, 200, 0.98); }
.color-11.hovered { background: rgba(255, 170, 130, 0.98); }"#
    };

    format!(
        r#"
* {{ margin: 0; padding: 0; box-sizing: border-box; }}
html, body {{
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: transparent;
    font-family: 'Google Sans Flex', 'Segoe UI Variable Text', 'Segoe UI', system-ui, sans-serif;
    font-variation-settings: 'wght' 500, 'wdth' 100, 'ROND' 100;
    user-select: none;
    color: {text_color};
}}

.container {{
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 100%;
    padding: 40px;
    gap: 10px;
}}

.dismiss-btn {{
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 12px 36px;
    margin-bottom: 16px;
    background: {dismiss_bg};
    backdrop-filter: blur(16px);
    border: 1px solid {dismiss_border};
    border-radius: 24px;
    cursor: pointer;
    font-size: 14px;
    letter-spacing: 3px;
    text-transform: uppercase;
    font-variation-settings: 'wght' 600, 'wdth' 125, 'ROND' 100;
    color: {dismiss_color};
    opacity: 0;
    transform: scale(0.5);
    transition:
        transform 0.2s cubic-bezier(0.22, 1, 0.36, 1),
        opacity 0.15s ease-out,
        background 0.1s ease,
        border-color 0.1s ease,
        box-shadow 0.1s ease,
        color 0.1s ease,
        font-variation-settings 0.15s ease;
}}

.dismiss-btn.visible {{
    opacity: 1;
    transform: scale(1);
}}

.dismiss-btn:hover {{
    background: {dismiss_hover_bg};
    border-color: {dismiss_hover_border};
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.2);
    color: {text_color};
    font-variation-settings: 'wght' 700, 'wdth' 105, 'ROND' 100;
}}

.dismiss-btn:active {{
    transform: scale(0.92) !important;
}}

.presets-grid {{
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 20px;
}}

.preset-row {{
    display: flex;
    flex-direction: row;
    justify-content: center;
    align-items: center;
    gap: 10px;
    min-height: 40px;
}}

.preset-item {{
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 9px 14px;
    min-width: 85px;
    backdrop-filter: blur(12px);
    border: 1px solid {item_border};
    border-radius: 15px;
    cursor: pointer;
    font-size: 12px;
    white-space: nowrap;
    letter-spacing: 0;
    color: {text_color};
    opacity: 0;
    transform: scale(0.8);
    transition:
        transform 0.15s cubic-bezier(0.22, 1, 0.36, 1),
        opacity 0.15s ease-out,
        background 0.1s ease,
        box-shadow 0.1s ease,
        border-color 0.1s ease,
        font-variation-settings 0.1s ease,
        letter-spacing 0.1s ease;
}}

.preset-item.visible {{
    opacity: 1;
    transform: scale(1);
}}

{color_palette}

.preset-item.hovered {{
    border-color: {item_hover_border};
    box-shadow: {item_shadow};
    font-variation-settings: 'wght' 650, 'wdth' 90, 'ROND' 100;
    letter-spacing: 0.5px;
}}

.preset-item:active {{
    transform: scale(0.88) !important;
    transition: transform 0.05s ease !important;
}}
"#,
        text_color = text_color,
        dismiss_bg = dismiss_bg,
        dismiss_border = dismiss_border,
        dismiss_color = dismiss_color,
        dismiss_hover_bg = dismiss_hover_bg,
        dismiss_hover_border = dismiss_hover_border,
        item_border = item_border,
        item_hover_border = item_hover_border,
        item_shadow = item_shadow,
        color_palette = color_palette
    )
}
