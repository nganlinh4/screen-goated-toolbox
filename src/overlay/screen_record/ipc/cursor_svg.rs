// --- CURSOR SVG MANIPULATION ---
// Functions for adjusting cursor SVG geometry: scaling, offsetting,
// and normalizing cursor pack SVG files in the repo.

use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn fmt_num(v: f32) -> String {
    let s = format!("{:.2}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn is_repo_root(path: &Path) -> bool {
    path.join("Cargo.toml").exists()
        && path.join("screen-record").exists()
        && path.join("src").exists()
}

fn find_repo_root() -> Result<PathBuf, String> {
    let mut dir = std::env::current_dir().map_err(|e| format!("current_dir failed: {}", e))?;
    for _ in 0..6 {
        if is_repo_root(&dir) {
            return Ok(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    Err("Could not locate repository root".to_string())
}

fn sanitize_svg_rel_path(src: &str) -> Result<String, String> {
    if !src.ends_with(".svg") {
        return Err("Only .svg files are allowed".to_string());
    }
    let rel = src.trim_start_matches('/');
    if rel.is_empty() || rel.contains("..") || rel.contains('\\') {
        return Err("Invalid svg path".to_string());
    }
    if !(rel.starts_with("cursor-") || rel.starts_with("cursors/")) {
        return Err("Path outside cursor assets".to_string());
    }
    Ok(rel.to_string())
}

pub(super) fn apply_cursor_svg_adjustment(
    src: &str,
    scale: f32,
    offset_x_lab: f32,
    offset_y_lab: f32,
) -> Result<usize, String> {
    let rel = sanitize_svg_rel_path(src)?;
    let repo_root = find_repo_root()?;

    let targets = [
        repo_root.join("screen-record").join("public").join(&rel),
        repo_root
            .join("src")
            .join("overlay")
            .join("screen_record")
            .join("dist")
            .join(&rel),
    ];

    let scale = scale.clamp(0.2, 4.0);
    let offset_x = offset_x_lab;
    let offset_y = offset_y_lab;
    let draw_w = 44.0 * scale;
    let draw_h = 43.0 * scale;
    let x = offset_x + (44.0 - draw_w) * 0.5;
    let y = offset_y + (43.0 - draw_h) * 0.5;

    let mut found = 0usize;
    let mut updated = 0usize;
    for path in targets {
        if !path.exists() {
            continue;
        }
        found += 1;
        let content =
            fs::read_to_string(&path).map_err(|e| format!("read {:?} failed: {}", path, e))?;
        let replaced = replace_cursor_svg_geometry(&content, x, y, draw_w, draw_h, scale)?;
        if replaced != content {
            let next = normalize_sgt_offset_transform(replaced);
            if next != content {
                fs::write(&path, next).map_err(|e| format!("write {:?} failed: {}", path, e))?;
                updated += 1;
            }
        }
    }

    if found == 0 {
        return Err(format!("No target files found for {}", rel));
    }
    Ok(updated)
}

fn replace_cursor_svg_geometry(
    content: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    scale: f32,
) -> Result<String, String> {
    if let Ok(next) = replace_nested_svg_geometry(content, x, y, width, height) {
        return Ok(next);
    }
    replace_group_transform_geometry(content, x, y, scale)
}

fn replace_nested_svg_geometry(
    content: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
) -> Result<String, String> {
    let mut cursor = 0usize;
    let mut svg_index = 0usize;
    let mut target: Option<(usize, usize)> = None;

    while let Some(rel) = content[cursor..].find("<svg") {
        let start = cursor + rel;
        let end_rel = content[start..]
            .find('>')
            .ok_or("Could not locate end of nested <svg> tag")?;
        let end = start + end_rel;
        let tag = &content[start..=end];

        if svg_index > 0 && tag.contains("viewBox=") {
            target = Some((start, end));
            break;
        }

        svg_index += 1;
        cursor = end + 1;
    }

    let (start, end) = target.ok_or("Could not locate nested <svg ... viewBox=...> block")?;
    let tag = &content[start..=end];
    let tag = set_or_insert_svg_attr(tag, "x", &fmt_num(x));
    let tag = set_or_insert_svg_attr(&tag, "y", &fmt_num(y));
    let tag = set_or_insert_svg_attr(&tag, "width", &fmt_num(width));
    let tag = set_or_insert_svg_attr(&tag, "height", &fmt_num(height));

    Ok(format!(
        "{}{}{}",
        &content[..start],
        tag,
        &content[end + 1..]
    ))
}

fn set_or_insert_svg_attr(tag: &str, name: &str, value: &str) -> String {
    let double_pat = format!(r#"{}=""#, name);
    if let Some(pos) = tag.find(&double_pat) {
        let value_start = pos + double_pat.len();
        if let Some(end_rel) = tag[value_start..].find('"') {
            let value_end = value_start + end_rel;
            return format!("{}{}{}", &tag[..value_start], value, &tag[value_end..]);
        }
    }

    let single_pat = format!(r#"{}='"#, name);
    if let Some(pos) = tag.find(&single_pat) {
        let value_start = pos + single_pat.len();
        if let Some(end_rel) = tag[value_start..].find('\'') {
            let value_end = value_start + end_rel;
            return format!("{}{}{}", &tag[..value_start], value, &tag[value_end..]);
        }
    }

    if let Some(gt) = tag.rfind('>') {
        return format!(r#"{} {}="{}"{}"#, &tag[..gt], name, value, &tag[gt..]);
    }

    tag.to_string()
}

fn replace_group_transform_geometry(
    content: &str,
    x: f32,
    y: f32,
    scale: f32,
) -> Result<String, String> {
    let marker = r#"<g transform="translate("#;
    let start = content
        .find(marker)
        .ok_or("Could not locate group transform for cursor geometry")?;
    let rest = &content[start..];
    let end_rel = rest
        .find(")\">")
        .ok_or("Could not locate end of group transform")?;
    let end = start + end_rel + 3;

    let replacement = format!(
        r#"<g transform="translate({} {}) scale({})">"#,
        fmt_num(x),
        fmt_num(y),
        fmt_num(scale)
    );

    Ok(format!(
        "{}{}{}",
        &content[..start],
        replacement,
        &content[end..]
    ))
}

pub(super) fn normalize_sgt_offset_transform(mut content: String) -> String {
    let marker = r#"data-sgt-offset="1""#;
    let transform_prefix = r#"transform="translate("#;
    let transform_replacement = r#"transform="translate(0 0)""#;

    let mut search_from = 0usize;
    while let Some(marker_rel) = content[search_from..].find(marker) {
        let marker_idx = search_from + marker_rel;
        let before = &content[..marker_idx];
        if let Some(ts) = before.rfind(transform_prefix) {
            let after_ts = &content[ts..];
            if let Some(end_rel) = after_ts.find(")\"") {
                let end = ts + end_rel + 2; // include )"
                content.replace_range(ts..end, transform_replacement);
                search_from = marker_idx + marker.len();
                continue;
            }
        }
        search_from = marker_idx + marker.len();
    }
    content
}
