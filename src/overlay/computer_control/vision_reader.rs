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
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use crate::api::{TranslateImageRequest, translate_image_streaming};
use crate::config::Config;
use crate::model_config::{get_model_by_id_with_custom, model_is_non_llm};

mod candidates;
mod circuit;
mod mark_labels;
mod schemas;
mod text_candidates;
pub(super) use candidates::{CandidateAttempt, CandidateReport};
pub(super) use mark_labels::label_clickable_marks;
use schemas::{box_schema, point_schema, points_schema, verification_schema};
pub(super) use text_candidates::read_text_pref_where;

/// Per-provider API key, preferring the repo `.env` overrides (so the headless
/// harness works) and falling back to the saved app config.
fn key_for(provider: &str, config: &Config) -> Option<String> {
    let v = match provider {
        "google" | "gemini-live" => {
            crate::api::provider_credentials::resolve("GEMINI_API_KEY", &config.gemini_api_key)
        }
        "groq" => crate::api::provider_credentials::resolve("GROQ_API_KEY", &config.api_key),
        "openrouter" => crate::api::provider_credentials::resolve(
            "OPENROUTER_API_KEY",
            &config.openrouter_api_key,
        ),
        "cerebras" => {
            crate::api::provider_credentials::resolve("CEREBRAS_API_KEY", &config.cerebras_api_key)
        }
        _ => String::new(),
    };
    let v = v.trim().to_string();
    (!v.is_empty()).then_some(v)
}

/// Computer-control's default vision model: a strong, accurate reader/locator,
/// preferred over the user's OCR-tuned `image_to_text` stack (whose first entry
/// can be too weak for fine board reading / pixel pointing). Overridable via
/// `CC_VISION_MODEL`.
const CC_DEFAULT_VISION_MODEL: &str = "gemini-3.1-flash-lite";

#[derive(Clone, Copy)]
enum VisionTask {
    General,
    Grounding,
}

struct ChainRun<'a> {
    task: VisionTask,
    cancel_token: Option<Arc<AtomicBool>>,
    request_timeout: Option<Duration>,
    attempts: Option<&'a mut Vec<CandidateAttempt>>,
}

pub(super) struct CandidateCallbacks<OnAttempt, Accept> {
    on_attempt: OnAttempt,
    accept: Accept,
}

impl<OnAttempt, Accept> CandidateCallbacks<OnAttempt, Accept> {
    pub(super) fn new(on_attempt: OnAttempt, accept: Accept) -> Self {
        Self { on_attempt, accept }
    }
}

/// General reading follows the user's image chain. Pixel grounding is isolated
/// to its benchmarked locator model: a weak image-to-text fallback must fail
/// closed rather than silently becoming permission to click the wrong place.
fn chain_ids(config: &Config, prefer: &[&str], task: VisionTask) -> Vec<String> {
    let default_first = std::env::var("CC_VISION_MODEL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| CC_DEFAULT_VISION_MODEL.to_string());
    let configured: Vec<String> = match task {
        VisionTask::General => config.model_priority_chains.image_to_text.clone(),
        VisionTask::Grounding => vec![default_first],
    };
    let candidates = prefer
        .iter()
        .map(|p| p.trim().to_string())
        .chain(configured);
    let mut ids: Vec<String> = Vec::new();
    for c in candidates {
        if !c.is_empty() && !ids.contains(&c) {
            ids.push(c);
        }
    }
    ids
}

pub(super) fn configured_general_chain(prefer: &[&str]) -> Vec<String> {
    chain_ids(&crate::load_config(), prefer, VisionTask::General)
}

/// Run `prompt` over `jpeg` through the model chain (`prefer` ids tried first),
/// returning the first non-empty answer.
fn run_chain(
    jpeg: &[u8],
    prompt: &str,
    prefer: &[&str],
    schema: Option<serde_json::Value>,
    task: VisionTask,
) -> Result<String> {
    run_chain_where(
        jpeg,
        prompt,
        prefer,
        schema,
        ChainRun {
            task,
            cancel_token: None,
            request_timeout: None,
            attempts: None,
        },
        |_| {},
        |_| true,
    )
}

/// As [`run_chain`], but a non-empty answer is accepted only when `accept`
/// validates its task-specific contract. Invalid structured output falls
/// through to the next configured provider instead of disabling grounding.
fn run_chain_where(
    jpeg: &[u8],
    prompt: &str,
    prefer: &[&str],
    schema: Option<serde_json::Value>,
    run: ChainRun<'_>,
    mut on_attempt: impl FnMut(&CandidateAttempt),
    mut accept: impl FnMut(&str) -> bool,
) -> Result<String> {
    let ChainRun {
        task,
        cancel_token,
        request_timeout,
        mut attempts,
    } = run;
    let config = crate::load_config();
    let gemini_key = key_for("google", &config).unwrap_or_default();
    let groq_key = key_for("groq", &config).unwrap_or_default();
    let img = image::load_from_memory(jpeg)
        .map_err(|e| anyhow!("decode crop: {e}"))?
        .to_rgba8();

    let mut last_err = None;
    for id in &chain_ids(&config, prefer, task) {
        if cancel_token
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::SeqCst))
        {
            last_err = Some(anyhow!("cancelled"));
            break;
        }
        if let Some(remaining) = circuit::remaining(id) {
            eprintln!(
                "[vision] {id} skipped: rate-limit cooldown {}s remaining",
                remaining.as_secs().max(1)
            );
            last_err = Some(anyhow!("{id} is cooling down after a rate limit"));
            continue;
        }
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
            response_schema: schema.clone(),
            cancel_token: cancel_token.clone(),
            request_timeout,
        };
        match translate_image_streaming(req, |_| {}) {
            Ok(response) => {
                let trimmed = response.trim();
                let accepted = !trimmed.is_empty() && accept(trimmed);
                let attempt =
                    CandidateAttempt::response(&mc.id, &mc.provider, response.clone(), accepted);
                on_attempt(&attempt);
                if let Some(attempts) = attempts.as_deref_mut() {
                    attempts.push(attempt);
                }
                if accepted {
                    eprintln!("[vision] {} ({})", mc.id, mc.provider);
                    return Ok(trimmed.to_string());
                }
                if trimmed.is_empty() {
                    last_err = Some(anyhow!("{} returned empty", mc.id));
                } else {
                    eprintln!("[vision] {} returned non-accepted output", mc.id);
                    last_err = Some(anyhow!("{} did not satisfy the caller contract", mc.id));
                }
            }
            Err(e) => {
                let attempt = CandidateAttempt::error(&mc.id, &mc.provider, e.to_string());
                on_attempt(&attempt);
                if let Some(attempts) = attempts.as_deref_mut() {
                    attempts.push(attempt);
                }
                eprintln!("[vision] {} failed: {e}", mc.id);
                if circuit::is_rate_limit_error(&e.to_string()) {
                    circuit::cool_down(&mc.id);
                    eprintln!("[vision] {} entered rate-limit cooldown", mc.id);
                }
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow!("no usable model in image_to_text chain")))
}

/// A located click point (0-1000 over the image) plus what the vision model
/// observed AT that point (e.g. "empty cell", "an X") — fed back to the Live
/// model so it knows the target's state without a separate look.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Located {
    pub x: f64,
    pub y: f64,
    pub note: Option<String>,
}

pub(super) struct Verification {
    pub matches: bool,
    pub confidence: u64,
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

/// Read a question about one immutable image. A candidate reply is usable only
/// when `accept` validates the caller's output contract. Every candidate sees
/// the exact same image and prompt; malformed output falls through the configured
/// chain. Pass `&[]` to honor its order without an override.
pub(super) fn read_image_pref_where(
    jpeg: &[u8],
    question: &str,
    ctx: &str,
    prefer: &[&str],
    cancel_token: Option<Arc<AtomicBool>>,
    request_timeout: Duration,
    callbacks: CandidateCallbacks<impl FnMut(&CandidateAttempt), impl FnMut(&str) -> bool>,
) -> CandidateReport {
    let CandidateCallbacks { on_attempt, accept } = callbacks;
    let mut attempts = Vec::new();
    let answer = run_chain_where(
        jpeg,
        &format!("{}{question}", ctx_prefix(ctx)),
        prefer,
        None,
        ChainRun {
            task: VisionTask::General,
            cancel_token,
            request_timeout: Some(request_timeout),
            attempts: Some(&mut attempts),
        },
        on_attempt,
        accept,
    );
    CandidateReport { answer, attempts }
}

/// Ask the vision stack for the click point of `description` (+ what's there).
pub(super) fn locate_point(jpeg: &[u8], description: &str, ctx: &str) -> Result<Located> {
    locate_point_pref(jpeg, description, None, ctx)
}

/// Like [`locate_point`] but tries `model` first (the FINE pass on a zoomed crop
/// is easy localization, so a faster stack model often suffices) — falling back
/// to the accurate default if it fails. Stateless and per-call: never loses
/// correctness, only speeds the common case.
pub(super) fn locate_point_with(
    jpeg: &[u8],
    description: &str,
    model: &str,
    ctx: &str,
) -> Result<Located> {
    locate_point_pref(jpeg, description, Some(model), ctx)
}

fn locate_point_pref(
    jpeg: &[u8],
    description: &str,
    prefer: Option<&str>,
    ctx: &str,
) -> Result<Located> {
    let prompt = format!(
        "{}Find this target in the image: {description}. Output ONLY JSON \
{{\"x\": <int>, \"y\": <int>, \"what\": \"<2-4 words: what is AT that location, e.g. empty cell, an X, a button>\"}} \
- x,y are the CENTER on a 0-1000 grid (x: 0 left to 1000 right; y: 0 top to 1000 bottom). If the target is not \
visible, output {{\"error\": \"not visible\"}}.",
        ctx_prefix(ctx)
    );
    let pref: Vec<&str> = prefer.into_iter().collect();
    let answer = run_chain(
        jpeg,
        &prompt,
        &pref,
        Some(point_schema()),
        VisionTask::Grounding,
    )?;
    let (x, y) = parse_point(&answer)
        .ok_or_else(|| anyhow!("could not parse a point from vision answer: {answer}"))?;
    Ok(Located {
        x,
        y,
        note: parse_str_field(&answer, "what"),
    })
}

/// Independently inspect a fresh crop whose red crosshair marks the proposed
/// click point. A localization is authorization to click only when this check
/// confirms that the crosshair itself lies inside the requested target.
pub(super) fn verify_target(jpeg: &[u8], description: &str, ctx: &str) -> Result<Verification> {
    let prompt = format!(
        "{}The red crosshair marks a proposed click. Requested target: {description}. \
Output ONLY JSON {{\"matches\": <bool>, \"confidence\": <0-100 int>, \"what\": \"<what the crosshair is on>\"}}. \
matches is true only if the CROSSHAIR CENTER is visibly inside the requested target; merely seeing the target \
elsewhere in the crop is false.",
        ctx_prefix(ctx)
    );
    let answer = run_chain(
        jpeg,
        &prompt,
        &[],
        Some(verification_schema()),
        VisionTask::Grounding,
    )?;
    let start = answer
        .find('{')
        .ok_or_else(|| anyhow!("verification JSON missing: {answer}"))?;
    let end = answer
        .rfind('}')
        .ok_or_else(|| anyhow!("verification JSON missing: {answer}"))?;
    let value: serde_json::Value = serde_json::from_str(&answer[start..=end])
        .map_err(|_| anyhow!("verification JSON invalid: {answer}"))?;
    Ok(Verification {
        matches: value
            .get("matches")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        confidence: value
            .get("confidence")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
            .min(100),
        note: value
            .get("what")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    })
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
    let answer = run_chain(
        jpeg,
        &prompt,
        &[],
        Some(box_schema()),
        VisionTask::Grounding,
    )?;
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
- x,y are the CENTER on a 0-1000 grid (x 0 left..1000 right, y 0 top..1000 bottom). Output [] if none. Cap at 30.",
        ctx_prefix(ctx)
    );
    let answer = run_chain(
        jpeg,
        &prompt,
        &[],
        Some(points_schema()),
        VisionTask::Grounding,
    )?;
    parse_points(&answer)
        .ok_or_else(|| anyhow!("could not parse point array from vision answer: {answer}"))
}

/// Parse a JSON array of `{x,y,what}` objects from a vision answer (tolerant of
/// surrounding prose / markdown fences).
fn parse_points(s: &str) -> Option<Vec<Located>> {
    let (Some(a), Some(b)) = (s.find('['), s.rfind(']')) else {
        return None;
    };
    if b <= a {
        return None;
    }
    let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(&s[a..=b])
    else {
        return None;
    };
    let input_was_empty = arr.is_empty();
    let mut points: Vec<Located> = arr
        .iter()
        .filter_map(|item| {
            let x = item.get("x").and_then(serde_json::Value::as_f64)?;
            let y = item.get("y").and_then(serde_json::Value::as_f64)?;
            if !x.is_finite()
                || !y.is_finite()
                || !(0.0..=1000.0).contains(&x)
                || !(0.0..=1000.0).contains(&y)
            {
                return None;
            }
            let note = item
                .get("what")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string);
            Some(Located { x, y, note })
        })
        .collect();
    if !input_was_empty && points.is_empty() {
        return None;
    }
    points.sort_by(|left, right| left.y.total_cmp(&right.y).then(left.x.total_cmp(&right.x)));
    let mut unique: Vec<Located> = Vec::with_capacity(points.len().min(30));
    for point in points {
        let duplicate = unique.iter().any(|existing| {
            let dx = existing.x - point.x;
            let dy = existing.y - point.y;
            dx * dx + dy * dy < 100.0
        });
        if !duplicate {
            unique.push(point);
        }
        if unique.len() == 30 {
            break;
        }
    }
    Some(unique)
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
    let mut found = None;
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
                    found = Some(v);
                }
            }
        }
        i += 1;
    }
    found
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod vision_benchmark_tests;
