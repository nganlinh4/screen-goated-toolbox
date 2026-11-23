use crate::gui::locale::{LocaleText, OverlayLocaleText};

pub fn show_api_key_error_notification(error: &str, lang: &str) {
    if let Some(message) = crate::credential_error::message(error, lang) {
        crate::overlay::auto_copy_badge::show_error_notification(&message);
    }
}

pub fn get_error_message(error: &str, lang: &str, model_name: Option<&str>) -> String {
    if let Some(message) = crate::credential_error::message(error, lang) {
        return message;
    }
    let text = LocaleText::get(lang).overlay;
    match HttpFailure::detect(error, model_name) {
        Some(failure) => failure.message(&text),
        None => text.chain_error_generic.replace("{error}", error),
    }
}

/// The label shown while a chain step retries on another model.
pub fn retry_message(lang: &str, model_name: &str) -> String {
    LocaleText::get(lang)
        .overlay
        .chain_retrying
        .replace("{model}", model_name)
}

/// An HTTP failure, as detected from a provider error: which status, which
/// provider and which model. The message text is the locale's.
#[derive(Debug, PartialEq)]
struct HttpFailure<'a> {
    status_code: u16,
    provider: String,
    model_name: Option<&'a str>,
}

impl<'a> HttpFailure<'a> {
    fn detect(error: &str, model_name: Option<&'a str>) -> Option<Self> {
        Some(Self {
            status_code: extract_http_status_code(error)?,
            provider: extract_provider_from_error(error),
            model_name,
        })
    }

    fn message(&self, text: &OverlayLocaleText) -> String {
        let template = match self.status_code {
            400 => text.chain_error_http_400,
            401 => text.chain_error_http_401,
            402 => text.chain_error_http_402,
            403 => text.chain_error_http_403,
            404 => text.chain_error_http_404,
            429 => text.chain_error_http_429,
            500 => text.chain_error_http_500,
            502 => text.chain_error_http_502,
            503 => text.chain_error_http_503,
            504 => text.chain_error_http_504,
            _ => text.chain_error_http_other,
        };
        let target = self.model_name.map_or_else(
            || self.provider.clone(),
            |model| format!("{model} ({})", self.provider),
        );
        template
            .replace("{code}", &self.status_code.to_string())
            .replace("{target}", &target)
            .replace("{provider}", &self.provider)
            .replace(
                "{model}",
                self.model_name.unwrap_or(text.chain_error_unnamed_model),
            )
    }
}

/// Extracts HTTP status code from error message.
///
/// Accepts the shapes providers actually emit: `status code 429`, `HTTP 402: ...`,
/// `error 500`, or a bare code delimited by non-alphanumeric characters. The
/// boundary check keeps model names such as `llama-3.3-70b` or `qwen3-235b` from
/// being mistaken for status codes.
pub(crate) fn extract_http_status_code(error: &str) -> Option<u16> {
    const CODE_PREFIXES: [&str; 5] = ["status code ", "http ", "http/1.1 ", "error ", "code "];

    let lower = error.to_ascii_lowercase();
    for prefix in CODE_PREFIXES {
        let mut search_from = 0;
        while let Some(pos) = lower[search_from..].find(prefix) {
            let after = search_from + pos + prefix.len();
            search_from = after;
            if let Some(code) = parse_status_code_at(&lower, after) {
                return Some(code);
            }
        }
    }

    first_delimited_status_code(&lower)
}

/// Parses a 3-digit HTTP status code starting at `offset`, rejecting longer digit runs.
fn parse_status_code_at(text: &str, offset: usize) -> Option<u16> {
    let bytes = text.as_bytes();
    let digits: Vec<u8> = bytes[offset..]
        .iter()
        .copied()
        .take_while(u8::is_ascii_digit)
        .collect();
    if digits.len() != 3 {
        return None;
    }
    let code: u16 = std::str::from_utf8(&digits).ok()?.parse().ok()?;
    (400..=599).contains(&code).then_some(code)
}

/// Scans for a standalone 3-digit code that is not embedded in a longer token.
fn first_delimited_status_code(text: &str) -> Option<u16> {
    let bytes = text.as_bytes();
    for start in 0..bytes.len() {
        if !bytes[start].is_ascii_digit() {
            continue;
        }
        let preceded_by_token = start > 0 && bytes[start - 1].is_ascii_alphanumeric();
        if preceded_by_token {
            continue;
        }
        let end = start + 3;
        if end > bytes.len() || bytes[end..].first().is_some_and(u8::is_ascii_alphanumeric) {
            continue;
        }
        if let Some(code) = parse_status_code_at(text, start) {
            return Some(code);
        }
    }
    None
}

/// Extracts the provider name from an error URL or host fragment.
fn extract_provider_from_error(error: &str) -> String {
    [
        ("api.groq.com", "groq"),
        ("generativelanguage.googleapis.com", "google"),
        ("gemini", "google"),
        ("api.openai.com", "openai"),
        ("openrouter.ai", "openrouter"),
        ("api.anthropic.com", "anthropic"),
        ("claude", "anthropic"),
    ]
    .into_iter()
    .find_map(|(needle, key)| error.contains(needle).then_some(key))
    .map_or_else(
        || "API".to_string(),
        |key| crate::model_config::provider_full_name(key).to_string(),
    )
}

/// Billing/credit exhaustion markers. These are provider-wide: every model behind
/// the same account fails identically, so retrying siblings only wastes requests.
pub fn is_billing_exhausted_error(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("payment required")
        || lower.contains("insufficient credit")
        || lower.contains("insufficient_quota")
        || lower.contains("insufficient funds")
        || lower.contains("out of credits")
        || lower.contains("credit balance is too low")
}

pub fn should_advance_retry_chain(error: &str) -> bool {
    if error.starts_with("PROVIDER_RESPONSE_INVALID:") {
        return true;
    }
    if error.contains("NO_API_KEY")
        || error.contains("INVALID_API_KEY")
        || error.contains(crate::api::client::PROVIDER_TRANSPORT_UNAVAILABLE)
    {
        return true;
    }

    if is_billing_exhausted_error(error) {
        return true;
    }

    if let Some(code) = extract_http_status_code(error) {
        if matches!(
            code,
            400 | 401 | 402 | 403 | 404 | 408 | 409 | 413 | 422 | 425 | 429
        ) {
            return true;
        }
        if (500..=599).contains(&code) {
            return true;
        }
        return false;
    }

    let lower_err = error.to_lowercase();
    if lower_err.contains("empty_model_response")
        || lower_err.contains("rate limit")
        || lower_err.contains("too many requests")
        || lower_err.contains("quota exceeded")
        || lower_err.contains("peer disconnected")
        || lower_err.contains("connection reset")
        || lower_err.contains("connection aborted")
        || lower_err.contains("connection closed")
        || lower_err.contains("broken pipe")
        || lower_err.contains("timed out")
        || lower_err.contains("timeout")
        || lower_err.contains("deadline exceeded")
        || lower_err.contains("not found")
        || lower_err.contains("unsupported")
        || lower_err.contains("not support")
    {
        return true;
    }

    false
}

pub fn should_block_retry_provider(error: &str) -> bool {
    if error.contains("NO_API_KEY")
        || error.contains("INVALID_API_KEY")
        || error.contains("PROVIDER_DISABLED")
        || error.contains(crate::api::client::PROVIDER_TRANSPORT_UNAVAILABLE)
        || error.contains("STRUCTURED_OUTPUT_REJECTED")
    {
        return true;
    }

    if is_billing_exhausted_error(error) {
        return true;
    }

    matches!(extract_http_status_code(error), Some(401..=403))
}

#[cfg(test)]
mod tests;
