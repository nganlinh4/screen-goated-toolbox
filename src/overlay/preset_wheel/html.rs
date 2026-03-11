// Preset Wheel HTML - Apple Watch fisheye with center-out ripple animation

use super::script::get_js;
use super::state::WheelEntry;
use super::styles::generate_css;

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn calculate_row_distribution(n: usize) -> Vec<usize> {
    if n == 0 {
        return vec![];
    }

    if n == 1 {
        return vec![1];
    }

    let squish_factor = 1.5;
    let cols = ((n as f64 / squish_factor).sqrt().ceil() as usize).max(1);
    let num_rows = n.div_ceil(cols);
    let base = n / num_rows;
    let remainder = n % num_rows;

    let mut rows = Vec::with_capacity(num_rows);
    for i in 0..num_rows {
        rows.push(if i < remainder { base + 1 } else { base });
    }

    rows
}

pub(crate) fn generate_items_html(entries: &[WheelEntry]) -> String {
    let row_distribution = calculate_row_distribution(entries.len());
    let mut html = String::new();
    let mut item_idx = 0usize;

    for (row_idx, &items_in_row) in row_distribution.iter().enumerate() {
        html.push_str(&format!(
            r#"<div class="preset-row" data-row="{}">"#,
            row_idx
        ));

        for _ in 0..items_in_row {
            if let Some(entry) = entries.get(item_idx) {
                let color_class = format!("color-{}", item_idx % 12);
                html.push_str(&format!(
                    r#"<div class="preset-item {}" data-idx="{}" data-item="{}" onclick="select({})">{}</div>"#,
                    color_class,
                    entry.selection_id,
                    item_idx,
                    entry.selection_id,
                    escape_html(&entry.label)
                ));
                item_idx += 1;
            }
        }

        html.push_str("</div>");
    }

    html
}

pub(crate) fn get_wheel_template(is_dark: bool) -> String {
    let font_css = crate::overlay::html_components::font_manager::get_font_css();
    let css = generate_css(is_dark);
    let js = get_js();

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style id="font-style">
{font_css}
</style>
<style id="theme-style">
{css}
</style>
</head>
<body>
<div class="container">
    <div class="dismiss-btn" onclick="dismiss()">CANCEL</div>
    <div class="presets-grid" id="grid"></div>
</div>
<script>
{js}
</script>
</body>
</html>"#
    )
}
