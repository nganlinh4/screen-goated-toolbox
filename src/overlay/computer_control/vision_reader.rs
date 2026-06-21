//! Auxiliary vision grounding for pixels (the counterpart to UIA for widgets).
//!
//! The Live model gets only ~258 tokens per frame, too few to read or precisely
//! locate fine canvas/pixel content (game boards, charts, maps, raster images).
//! This routes a CLEAN high-res crop of the current view through the user's
//! configured `image_to_text` model priority stack (`translate_image_streaming`,
//! same provider dispatch the rest of the app uses), giving:
//!   * `read_image` — a plain-text reading of the content (perception), and
//!   * `locate_point` — the exact 0-1000 click point of a described target
//!     (localization), which fixes the coarse-grid click-accuracy problem.

use anyhow::{Result, anyhow};

use crate::api::{TranslateImageRequest, translate_image_streaming};
use crate::config::Config;
use crate::model_config::{get_model_by_id_with_custom, model_is_non_llm};

/// Per-provider API key, preferring the repo `.env` overrides (so the headless
/// harness works) and falling back to the saved app config.
fn key_for(provider: &str, config: &Config) -> Option<String> {
    let env_or = |env: &str, cfg: &str| {
        std::env::var(env)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| cfg.to_string())
    };
    let v = match provider {
        "google" | "gemini-live" => env_or("GEMINI_API_KEY", &config.gemini_api_key),
        "groq" => env_or("GROQ_API_KEY", &config.api_key),
        "openrouter" => config.openrouter_api_key.clone(),
        "cerebras" => config.cerebras_api_key.clone(),
        _ => String::new(),
    };
    let v = v.trim().to_string();
    (!v.is_empty()).then_some(v)
}

/// Computer-control's default vision model: a strong, accurate reader/locator,
/// preferred over the user's OCR-tuned `image_to_text` stack (whose first entry
/// can be too weak for fine board reading / pixel pointing). Overridable via
/// `CC_VISION_MODEL`.
const CC_DEFAULT_VISION_MODEL: &str = "gemini-3.1-flash-lite-preview";

/// CC vision fallback order AFTER the accurate default (flash-lite): the Live
/// model as a vision model (Unlimited quota, comparable accuracy, ~2x slower) and
/// then accurate gemma. So flash-lite stays #1; these are graceful fallbacks if it
/// fails / is rate-limited. Tried before the user's generic image_to_text chain.
const CC_VISION_FALLBACKS: &[&str] = &["gemini-live-vision-3.1", "gemma-4-26b-a4b-vision"];

/// The ordered model ids to try: `prefer` (if any) first, then the CC default
/// (or `CC_VISION_MODEL` override), then the CC fallbacks, then the user's
/// `image_to_text` chain. A preferred (e.g. faster) model is tried first but
/// ALWAYS falls back to the accurate default — it can never lose correctness.
fn chain_ids(config: &Config, prefer: Option<&str>) -> Vec<String> {
    let default_first = std::env::var("CC_VISION_MODEL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| CC_DEFAULT_VISION_MODEL.to_string());
    let candidates = prefer
        .map(|p| p.trim().to_string())
        .into_iter()
        .chain(std::iter::once(default_first))
        .chain(CC_VISION_FALLBACKS.iter().map(|s| s.to_string()))
        .chain(config.model_priority_chains.image_to_text.iter().cloned());
    let mut ids: Vec<String> = Vec::new();
    for c in candidates {
        if !c.is_empty() && !ids.contains(&c) {
            ids.push(c);
        }
    }
    ids
}

/// Run `prompt` over `jpeg` through the model chain (`prefer` tried first if set),
/// returning the first non-empty answer.
fn run_chain(jpeg: &[u8], prompt: &str, prefer: Option<&str>) -> Result<String> {
    let config = crate::load_config();
    let gemini_key = key_for("google", &config).unwrap_or_default();
    let groq_key = key_for("groq", &config).unwrap_or_default();
    let img = image::load_from_memory(jpeg)
        .map_err(|e| anyhow!("decode crop: {e}"))?
        .to_rgba8();

    let mut last_err = None;
    for id in &chain_ids(&config, prefer) {
        let Some(mc) = get_model_by_id_with_custom(id, &config.custom_models) else {
            continue;
        };
        if model_is_non_llm(&mc.id) {
            continue;
        }
        if key_for(&mc.provider, &config).is_none() {
            continue; // provider not usable headless / no key
        }
        let req = TranslateImageRequest {
            groq_api_key: &groq_key,
            gemini_api_key: &gemini_key,
            prompt: prompt.to_string(),
            model: mc.full_name.clone(),
            provider: mc.provider.clone(),
            image: img.clone(),
            original_bytes: Some(jpeg.to_vec()),
            streaming_enabled: false,
            use_json_format: false,
            cancel_token: None,
        };
        match translate_image_streaming(req, |_| {}) {
            Ok(t) if !t.trim().is_empty() => {
                eprintln!("[vision] {} ({})", mc.id, mc.provider);
                return Ok(t.trim().to_string());
            }
            Ok(_) => last_err = Some(anyhow!("{} returned empty", mc.id)),
            Err(e) => {
                eprintln!("[vision] {} failed: {e}", mc.id);
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("no usable model in image_to_text chain")))
}

/// A located click point (0-1000 over the image) plus what the vision model
/// observed AT that point (e.g. "empty cell", "an X") — fed back to the Live
/// model so it knows the target's state without a separate look.
pub(super) struct Located {
    pub x: f64,
    pub y: f64,
    pub note: Option<String>,
}

/// Optional disambiguation context (task + agent intent) prepended to a vision
/// prompt — gives the otherwise-stateless vision model the "why".
fn ctx_prefix(ctx: &str) -> String {
    let ctx = ctx.trim();
    if ctx.is_empty() {
        String::new()
    } else {
        format!("Context (for disambiguation only; do not echo it): {ctx}\n")
    }
}

/// Read `question` about `jpeg` with the vision stack (with optional `ctx`).
pub(super) fn read_image(jpeg: &[u8], question: &str, ctx: &str) -> Result<String> {
    run_chain(jpeg, &format!("{}{question}", ctx_prefix(ctx)), None)
}

/// Ask the vision stack for the click point of `description` (+ what's there).
pub(super) fn locate_point(jpeg: &[u8], description: &str, ctx: &str) -> Result<Located> {
    locate_point_pref(jpeg, description, None, ctx)
}

/// Like [`locate_point`] but tries `model` first (the FINE pass on a zoomed crop
/// is easy localization, so a faster stack model often suffices) — falling back
/// to the accurate default if it fails. Stateless and per-call: never loses
/// correctness, only speeds the common case.
pub(super) fn locate_point_with(jpeg: &[u8], description: &str, model: &str, ctx: &str) -> Result<Located> {
    locate_point_pref(jpeg, description, Some(model), ctx)
}

fn locate_point_pref(jpeg: &[u8], description: &str, prefer: Option<&str>, ctx: &str) -> Result<Located> {
    let prompt = format!(
        "{}Find this target in the image: {description}. Output ONLY JSON \
{{\"x\": <int>, \"y\": <int>, \"what\": \"<2-4 words: what is AT that location, e.g. empty cell, an X, a button>\"}} \
- x,y are the CENTER on a 0-1000 grid (x: 0 left to 1000 right; y: 0 top to 1000 bottom). If the target is not \
visible, output {{\"error\": \"not visible\"}}.",
        ctx_prefix(ctx)
    );
    let answer = run_chain(jpeg, &prompt, prefer)?;
    let (x, y) = parse_point(&answer)
        .ok_or_else(|| anyhow!("could not parse a point from vision answer: {answer}"))?;
    Ok(Located { x, y, note: parse_str_field(&answer, "what") })
}

/// Ask the vision stack for the target's bounding BOX (Gemini `box_2d`) and
/// return its CENTER. Box localization is a core Gemini spatial skill, but it
/// mis-locates tiny adjacent cells — used only behind `CC_LOCATE_MODE=box`.
pub(super) fn locate_box(jpeg: &[u8], description: &str, ctx: &str) -> Result<Located> {
    let prompt = format!(
        "{}Find this target in the image: {description}. Output ONLY JSON {{\"box_2d\": [ymin, xmin, ymax, xmax]}} \
- integer coordinates 0-1000 (y from top, x from left) for the target's TIGHT bounding box. If the target is not \
visible, output {{\"error\": \"not visible\"}}.",
        ctx_prefix(ctx)
    );
    let answer = run_chain(jpeg, &prompt, None)?;
    parse_box(&answer)
        // box_2d order is [ymin, xmin, ymax, xmax]; center = (x mid, y mid).
        .map(|[ymin, xmin, ymax, xmax]| Located {
            x: (xmin + xmax) / 2.0,
            y: (ymin + ymax) / 2.0,
            note: None,
        })
        .ok_or_else(|| anyhow!("could not parse a box from vision answer: {answer}"))
}

/// Ask the vision stack to enumerate EVERY target matching `description` as a JSON
/// array of centre points — for building a reusable set of click anchors in ONE
/// call (then the Live model clicks them by id, no per-click vision).
pub(super) fn locate_points(jpeg: &[u8], description: &str, ctx: &str) -> Result<Vec<Located>> {
    let prompt = format!(
        "{}Find EVERY target matching: {description}. Output ONLY a JSON array, one object per target, in reading \
order (top row left-to-right, then next row): [{{\"x\": <int>, \"y\": <int>, \"what\": \"<2-4 words at that spot>\"}}, ...] \
- x,y are the CENTER on a 0-1000 grid (x 0 left..1000 right, y 0 top..1000 bottom). Output [] if none. Cap at 60.",
        ctx_prefix(ctx)
    );
    let answer = run_chain(jpeg, &prompt, None)?;
    let pts = parse_points(&answer);
    if pts.is_empty() {
        anyhow::bail!("no points parsed from vision answer: {answer}");
    }
    Ok(pts)
}

/// Parse a JSON array of `{x,y,what}` objects from a vision answer (tolerant of
/// surrounding prose / markdown fences).
fn parse_points(s: &str) -> Vec<Located> {
    let (Some(a), Some(b)) = (s.find('['), s.rfind(']')) else {
        return Vec::new();
    };
    if b <= a {
        return Vec::new();
    }
    let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(&s[a..=b]) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let x = item.get("x").and_then(serde_json::Value::as_f64)?;
            let y = item.get("y").and_then(serde_json::Value::as_f64)?;
            let note = item.get("what").and_then(serde_json::Value::as_str).map(str::to_string);
            Some(Located { x, y, note })
        })
        .collect()
}

/// Extract a JSON string field `"key": "value"` from a vision answer.
fn parse_str_field(s: &str, key: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    let k = lower.find(&format!("\"{key}\""))?;
    let after = &s[k..];
    let colon = after.find(':')?;
    let rest = &after[colon + 1..];
    let q1 = rest.find('"')?;
    let q2 = rest[q1 + 1..].find('"')?;
    let v = rest[q1 + 1..q1 + 1 + q2].trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// Parse a `box_2d` [ymin, xmin, ymax, xmax] from a vision answer. Reads numbers
/// AFTER the `box_2d` key (so the `2` in the key isn't mistaken for a value),
/// else from the first `[`.
fn parse_box(s: &str) -> Option<[f64; 4]> {
    let region = match s.to_ascii_lowercase().find("box_2d") {
        Some(k) => &s[k + "box_2d".len()..],
        None => &s[s.find('[')?..],
    };
    let nums = first_numbers(region, 4);
    (nums.len() == 4).then(|| [nums[0], nums[1], nums[2], nums[3]])
}

/// The first `max` numbers in `s`, clamped 0-1000.
fn first_numbers(s: &str, max: usize) -> Vec<f64> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() && out.len() < max {
        if b[i].is_ascii_digit() {
            let start = i;
            while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                i += 1;
            }
            if let Ok(v) = s[start..i].parse::<f64>() {
                out.push(v.clamp(0.0, 1000.0));
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Pull an (x, y) 0-1000 point out of a vision answer, tolerating code fences,
/// prose, and `x`/`y` in either order (matched by key, not position). Returns
/// None if either key is absent (e.g. an "not visible" answer).
fn parse_point(s: &str) -> Option<(f64, f64)> {
    let x = num_after_key(s, b'x')?;
    let y = num_after_key(s, b'y')?;
    Some((x.clamp(0.0, 1000.0), y.clamp(0.0, 1000.0)))
}

/// Find `<key>` (a JSON key letter) followed by `:`/`=` then a number, e.g.
/// `"x": 420`, `x=420`. Skips the letter when it's part of a word (e.g. "max").
fn num_after_key(s: &str, key: u8) -> Option<f64> {
    let lc = s.to_ascii_lowercase();
    let b = lc.as_bytes();
    let key = key.to_ascii_lowercase();
    let mut i = 0;
    while i < b.len() {
        if b[i] == key && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let mut j = i + 1;
            while j < b.len() && matches!(b[j], b'"' | b'\'' | b' ' | b'\t') {
                j += 1;
            }
            if j < b.len() && (b[j] == b':' || b[j] == b'=') {
                j += 1;
                while j < b.len() && matches!(b[j], b'"' | b'\'' | b' ' | b'\t') {
                    j += 1;
                }
                let start = j;
                while j < b.len() && (b[j].is_ascii_digit() || b[j] == b'.') {
                    j += 1;
                }
                if j > start
                    && let Ok(v) = lc[start..j].parse::<f64>()
                {
                    return Some(v);
                }
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{parse_box, parse_point};

    #[test]
    fn parses_box_2d_ignoring_the_key_digit() {
        // The "2" in box_2d must NOT be read as the first coordinate.
        let b = parse_box(r#"{"box_2d": [100, 200, 300, 460]}"#).unwrap();
        assert_eq!(b, [100.0, 200.0, 300.0, 460.0]);
    }

    #[test]
    fn parses_bare_box_array() {
        let b = parse_box("```json\n[10, 20, 30, 40]\n```").unwrap();
        assert_eq!(b, [10.0, 20.0, 30.0, 40.0]);
    }

    #[test]
    fn rejects_box_not_visible() {
        assert_eq!(parse_box(r#"{"error": "not visible"}"#), None);
    }

    #[test]
    fn parses_json_point() {
        assert_eq!(parse_point(r#"{"x": 420, "y": 680}"#), Some((420.0, 680.0)));
    }

    #[test]
    fn parses_fenced_and_reordered() {
        let s = "```json\n{ \"y\": 100, \"x\": 900 }\n```";
        assert_eq!(parse_point(s), Some((900.0, 100.0)));
    }

    #[test]
    fn rejects_not_visible() {
        assert_eq!(parse_point(r#"{"error": "not visible"}"#), None);
    }
}
