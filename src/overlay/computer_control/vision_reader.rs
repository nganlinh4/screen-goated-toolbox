//! Auxiliary vision grounding for pixels (the counterpart to UIA for widgets).
//!
//! The Live model gets only ~258 tokens per frame, too few to read or precisely
//! locate fine canvas/pixel content (game boards, charts, maps, raster images).
//! This routes a CLEAN high-res crop of the current view through the catalog-owned
//! grounding chain (using the same provider dispatch as other image tasks), giving:
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

use super::vision_contract::{
    GROUNDING_STREAMING_ENABLED, VerificationDecision, context_prefix, drag_request,
    grounding_reports_not_visible, marks_request, parse_named_grounding_records,
    parse_open_grounding_records, parse_verification, point_request, response_reports_not_visible,
    verification_request,
};

mod candidates;
mod circuit;
mod schemas;
mod text_candidates;
pub(super) use candidates::{CandidateAttempt, CandidateReport};
use schemas::box_schema;
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
        _ => String::new(),
    };
    let v = v.trim().to_string();
    (!v.is_empty()).then_some(v)
}

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
/// to its catalog-owned locator chain: a weak image-to-text fallback must fail
/// closed rather than silently becoming permission to click the wrong place.
fn chain_ids(config: &Config, prefer: &[&str], task: VisionTask) -> Vec<String> {
    let grounding_chain = std::env::var("CC_VISION_MODEL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(|model| vec![model])
        .unwrap_or_else(|| {
            crate::model_config::COMPUTER_CONTROL_GROUNDING_MODEL_CHAIN_IDS
                .iter()
                .map(|model| (*model).to_string())
                .collect()
        });
    let configured: Vec<String> = match task {
        VisionTask::General => config.model_priority_chains.image_to_text.clone(),
        VisionTask::Grounding => grounding_chain,
    };
    let candidates = match task {
        VisionTask::General => prefer
            .iter()
            .map(|model| model.trim().to_string())
            .chain(configured)
            .collect::<Vec<_>>(),
        VisionTask::Grounding => configured,
    };
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

/// Run a structured grounding prompt through the isolated locator chain.
fn run_grounding_chain(
    jpeg: &[u8],
    prompt: &str,
    schema: Option<serde_json::Value>,
    accept: impl FnMut(&str) -> bool,
) -> Result<String> {
    run_chain_where(
        jpeg,
        prompt,
        &[],
        schema,
        ChainRun {
            task: VisionTask::Grounding,
            cancel_token: None,
            request_timeout: None,
            attempts: None,
        },
        |_| {},
        accept,
    )
}

/// Runs the selected chain, but accepts a non-empty answer only when `accept`
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
            streaming_enabled: GROUNDING_STREAMING_ENABLED,
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

pub(super) type Verification = VerificationDecision;

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
        &format!("{}{question}", context_prefix(ctx)),
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
    let request = point_request(description, ctx);
    let answer = run_grounding_chain(jpeg, &request.prompt, request.response_schema, |response| {
        parse_named_grounding_records(response, &["target"]).is_some()
            || grounding_reports_not_visible(response, &["target"])
    })?;
    let point = parse_named_grounding_records(&answer, &["target"])
        .and_then(|mut points| points.pop())
        .ok_or_else(|| anyhow!("target is not visible or grounding output was invalid"))?;
    Ok(Located {
        x: point.x,
        y: point.y,
        note: Some(point.label),
    })
}

/// Independently inspect a fresh crop whose red crosshair marks the proposed
/// click point. A localization is authorization to click only when this check
/// confirms that the crosshair itself lies inside the requested target.
pub(super) fn verify_target(jpeg: &[u8], description: &str, ctx: &str) -> Result<Verification> {
    let request = verification_request(description, ctx);
    let answer = run_grounding_chain(jpeg, &request.prompt, request.response_schema, |response| {
        parse_verification(response).is_some()
    })?;
    parse_verification(&answer).ok_or_else(|| anyhow!("verification JSON invalid: {answer}"))
}

/// Ask the vision stack for the target's bounding BOX (Gemini `box_2d`) and
/// return its CENTER. Box localization is a core Gemini spatial skill, but it
/// mis-locates tiny adjacent cells — used only behind `CC_LOCATE_MODE=box`.
pub(super) fn locate_box(jpeg: &[u8], description: &str, ctx: &str) -> Result<Located> {
    let prompt = format!(
        "{}Find this target in the image: {description}. Output ONLY JSON {{\"box_2d\": [ymin, xmin, ymax, xmax]}} \
- integer coordinates 0-1000 (y from top, x from left) for the target's TIGHT bounding box. If the target is not \
visible, output {{\"error\": \"not visible\"}}.",
        context_prefix(ctx)
    );
    let answer = run_grounding_chain(jpeg, &prompt, Some(box_schema()), |response| {
        parse_box(response).is_some() || response_reports_not_visible(response)
    })?;
    parse_box(&answer)
        // box_2d order is [ymin, xmin, ymax, xmax]; center = (x mid, y mid).
        .map(|[ymin, xmin, ymax, xmax]| Located {
            x: (xmin + xmax) / 2.0,
            y: (ymin + ymax) / 2.0,
            note: None,
        })
        .ok_or_else(|| anyhow!("could not parse a box from vision answer: {answer}"))
}

/// Ask the vision stack to enumerate every relevant target in one strict,
/// model-neutral record set.
pub(super) fn locate_points(jpeg: &[u8], description: &str, ctx: &str) -> Result<Vec<Located>> {
    let request = marks_request(description, ctx);
    let answer = run_grounding_chain(jpeg, &request.prompt, request.response_schema, |response| {
        parse_open_grounding_records(response).is_some()
    })?;
    parse_open_grounding_records(&answer)
        .map(|points| {
            points
                .into_iter()
                .map(|point| Located {
                    x: point.x,
                    y: point.y,
                    note: Some(point.label),
                })
                .collect()
        })
        .ok_or_else(|| anyhow!("could not parse visual marks from grounding answer"))
}

pub(super) fn locate_drag_points(
    jpeg: &[u8],
    from_description: &str,
    to_description: &str,
    ctx: &str,
) -> Result<(Located, Located)> {
    let request = drag_request(from_description, to_description, ctx);
    let answer = run_grounding_chain(jpeg, &request.prompt, request.response_schema, |response| {
        parse_named_grounding_records(response, &["from", "to"]).is_some()
            || grounding_reports_not_visible(response, &["from", "to"])
    })?;
    let points = parse_named_grounding_records(&answer, &["from", "to"])
        .ok_or_else(|| anyhow!("one or both drag endpoints are not visible"))?;
    let located = |id: &str| {
        points
            .iter()
            .find(|point| point.id.as_deref() == Some(id))
            .map(|point| Located {
                x: point.x,
                y: point.y,
                note: Some(point.label.clone()),
            })
            .ok_or_else(|| anyhow!("grounding output omitted the {id} endpoint"))
    };
    let from = located("from")?;
    let to = located("to")?;
    let dx = from.x - to.x;
    let dy = from.y - to.y;
    if dx * dx + dy * dy < 100.0 {
        anyhow::bail!("drag endpoints resolved to the same point");
    }
    Ok((from, to))
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

#[cfg(test)]
mod tests;
