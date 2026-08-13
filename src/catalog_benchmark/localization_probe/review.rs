use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::scoring::{Evaluation, Metrics};
use crate::catalog_benchmark::manifest::LocalizationCase;
use crate::overlay::screen_translate::geometry::PixelRegion;

pub(super) struct ReviewEntry {
    pub model_id: String,
    pub case_id: String,
    pub difficulty: u8,
    pub variant: String,
    pub raw_image: PathBuf,
    pub painted_image: PathBuf,
    pub metrics: Metrics,
}

pub(super) fn write_overlays(
    output: &Path,
    model_id: &str,
    variant: &str,
    case: &LocalizationCase,
    source: &image::RgbaImage,
    evaluation: &Evaluation,
) -> Result<(PathBuf, PathBuf)> {
    let directory = output.join("overlays").join(model_id);
    std::fs::create_dir_all(&directory)
        .with_context(|| format!("create {}", directory.display()))?;
    let stem = format!("{}-{variant}", case.id);
    let raw_path = directory.join(format!("{stem}-raw.png"));
    let painted_path = directory.join(format!("{stem}-painted.png"));
    let gold = case
        .regions
        .iter()
        .map(|region| pixel_region(region.box_px))
        .collect::<Vec<_>>();
    save_overlay(
        source,
        &gold,
        &evaluation.raw,
        [0, 255, 112, 255],
        [0, 210, 255, 255],
        &raw_path,
    )?;
    save_overlay(
        source,
        &gold,
        &evaluation.painted,
        [0, 255, 112, 255],
        [255, 42, 187, 255],
        &painted_path,
    )?;
    Ok((raw_path, painted_path))
}

pub(super) fn write_review(output: &Path, entries: &[ReviewEntry]) -> Result<()> {
    let mut html = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>Screen-text localization probe</title><style>body{font:14px system-ui,sans-serif;background:#15171b;color:#edf1f5;margin:24px}h1{margin:0 0 8px}.note{color:#b7c0ca;max-width:90ch}.grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(420px,1fr));gap:20px;margin-top:22px}.card{background:#22262d;border:1px solid #3a414c;border-radius:12px;padding:14px}.pair{display:grid;grid-template-columns:1fr 1fr;gap:10px}.pair img{width:100%;height:auto}.label{color:#aeb8c4;margin:6px 0}.metrics{line-height:1.55}.good{color:#48e692}.raw{color:#00d2ff}.paint{color:#ff4ac4}code{color:#dce5ef}</style></head><body><h1>Screen-text localization diagnostic</h1><p class=\"note\"><span class=\"good\">Green</span> is human-reviewed source text ground truth. <span class=\"raw\">Cyan</span> is the raw model location. <span class=\"paint\">Magenta</span> is the source-only paint region; translated-text layout expansion is intentionally excluded. This probe is separate from catalog history.</p><div class=\"grid\">",
    );
    for entry in entries {
        let raw = relative_path(output, &entry.raw_image);
        let painted = relative_path(output, &entry.painted_image);
        let metrics = &entry.metrics;
        write!(
            html,
            "<article class=\"card\"><h2>Level {} · {} · {}</h2><div class=\"pair\"><div><p class=\"label\">Raw location</p><img src=\"{}\"></div><div><p class=\"label\">Current painted rectangle</p><img src=\"{}\"></div></div><p class=\"metrics\"><code>{}</code><br>matched {}/{} expected, {} predicted · recall {:.1}% · precision {:.1}%<br>raw IoU {:.1}% · raw coverage {:.1}% · raw overpaint {:.2}×<br>painted IoU {:.1}% · painted coverage {:.1}% · painted overpaint {:.2}× · expansion delta {:+.2}×</p></article>",
            entry.difficulty,
            escape(&entry.variant),
            escape(&entry.case_id),
            escape(&raw),
            escape(&painted),
            escape(&entry.model_id),
            metrics.matched_regions,
            metrics.expected_regions,
            metrics.predicted_regions,
            metrics.region_recall * 100.0,
            metrics.region_precision * 100.0,
            metrics.raw_mean_iou * 100.0,
            metrics.raw_mean_gold_coverage * 100.0,
            metrics.raw_mean_overpaint_ratio,
            metrics.painted_mean_iou * 100.0,
            metrics.painted_mean_gold_coverage * 100.0,
            metrics.painted_mean_overpaint_ratio,
            metrics.expansion_overpaint_delta,
        )?;
    }
    html.push_str("</div></body></html>");
    std::fs::write(output.join("localization-review.html"), html)
        .context("write localization-review.html")
}

fn save_overlay(
    source: &image::RgbaImage,
    gold: &[PixelRegion],
    observed: &[PixelRegion],
    gold_color: [u8; 4],
    observed_color: [u8; 4],
    path: &Path,
) -> Result<()> {
    let mut output = source.clone();
    let thickness = (source.width().max(source.height()) / 700).clamp(2, 5);
    for region in gold {
        draw_outline(&mut output, *region, gold_color, thickness);
    }
    for region in observed {
        draw_outline(&mut output, *region, observed_color, thickness);
    }
    output
        .save(path)
        .with_context(|| format!("write {}", path.display()))
}

fn draw_outline(image: &mut image::RgbaImage, region: PixelRegion, color: [u8; 4], thickness: u32) {
    if image.width() == 0 || image.height() == 0 {
        return;
    }
    let left = region.x.min(image.width() - 1);
    let top = region.y.min(image.height() - 1);
    let right = region
        .x
        .saturating_add(region.width)
        .saturating_sub(1)
        .min(image.width() - 1);
    let bottom = region
        .y
        .saturating_add(region.height)
        .saturating_sub(1)
        .min(image.height() - 1);
    for offset in 0..thickness {
        let x1 = left.saturating_add(offset).min(right);
        let x2 = right.saturating_sub(offset).max(left);
        let y1 = top.saturating_add(offset).min(bottom);
        let y2 = bottom.saturating_sub(offset).max(top);
        for x in x1..=x2 {
            image.put_pixel(x, y1, image::Rgba(color));
            image.put_pixel(x, y2, image::Rgba(color));
        }
        for y in y1..=y2 {
            image.put_pixel(x1, y, image::Rgba(color));
            image.put_pixel(x2, y, image::Rgba(color));
        }
    }
}

fn pixel_region([x, y, width, height]: [u32; 4]) -> PixelRegion {
    PixelRegion {
        x,
        y,
        width,
        height,
    }
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
