use super::TranslateImageRequest;
use anyhow::Result;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const MAX_OBSERVED_CHUNKS: usize = 32;
const MAX_OBSERVED_CHARS: usize = 8_192;
const MAX_ERROR_CHARS: usize = 320;

static NEXT_CALL_ID: AtomicU64 = AtomicU64::new(1);

pub(super) struct VisionCallTrace {
    call_id: u64,
    started: Instant,
    provider: String,
    model: String,
    streaming: bool,
    source_width: u32,
    source_height: u32,
    input_bytes: Option<usize>,
    rgba_bytes: usize,
    prompt_bytes: usize,
    has_schema: bool,
    timeout_ms: Option<u128>,
    prepare_ms: Option<u128>,
    provider_started_ms: Option<u128>,
    wire_width: Option<u32>,
    wire_height: Option<u32>,
    wire_bytes: Option<usize>,
    wire_mime: Option<String>,
    retry_count: usize,
    retry_wait_ms: u128,
}

pub(super) struct OutputObserver {
    started: Instant,
    events: Vec<(u128, String)>,
}

impl VisionCallTrace {
    pub(super) fn start(request: &TranslateImageRequest<'_>) -> Self {
        Self {
            call_id: NEXT_CALL_ID.fetch_add(1, Ordering::Relaxed),
            started: Instant::now(),
            provider: request.provider.clone(),
            model: request.model.clone(),
            streaming: request.streaming_enabled,
            source_width: request.image.width(),
            source_height: request.image.height(),
            input_bytes: request.original_bytes.as_ref().map(Vec::len),
            rgba_bytes: request.image.as_raw().len(),
            prompt_bytes: request.prompt.len(),
            has_schema: request.response_schema.is_some(),
            timeout_ms: request.request_timeout.map(|timeout| timeout.as_millis()),
            prepare_ms: None,
            provider_started_ms: None,
            wire_width: None,
            wire_height: None,
            wire_bytes: None,
            wire_mime: None,
            retry_count: 0,
            retry_wait_ms: 0,
        }
    }

    pub(super) fn output_observer(&self) -> OutputObserver {
        OutputObserver {
            started: self.started,
            events: Vec::new(),
        }
    }

    pub(super) fn record_prepared(
        &mut self,
        width: u32,
        height: u32,
        bytes: usize,
        mime: &str,
        elapsed: Duration,
    ) {
        self.prepare_ms = Some(elapsed.as_millis());
        self.wire_width = Some(width);
        self.wire_height = Some(height);
        self.wire_bytes = Some(bytes);
        self.wire_mime = Some(mime.to_string());
    }

    pub(super) fn mark_provider_started(&mut self) {
        self.provider_started_ms
            .get_or_insert_with(|| self.started.elapsed().as_millis());
    }

    pub(super) fn record_retry(&mut self, delay: Duration) {
        self.retry_count += 1;
        self.retry_wait_ms += delay.as_millis();
    }

    pub(super) fn finish(self, result: &Result<String>, output: &OutputObserver) {
        crate::log_info!("{}", self.summary(result, output));
    }

    fn summary(&self, result: &Result<String>, output: &OutputObserver) -> String {
        let total_ms = self.started.elapsed().as_millis();
        let response = result.as_ref().ok().map(String::as_str).unwrap_or("");
        let first_output_ms = result
            .is_ok()
            .then(|| output.first_output_ms(response, total_ms))
            .flatten();
        let provider_ms = self
            .provider_started_ms
            .map(|started| total_ms.saturating_sub(started));
        let provider_first_output_ms = self
            .provider_started_ms
            .and_then(|started| first_output_ms.map(|first| first.saturating_sub(started)));
        let output_chars = result.as_ref().ok().map(|content| content.chars().count());
        let wire_dimensions = match (self.wire_width, self.wire_height) {
            (Some(width), Some(height)) => format!("{width}x{height}"),
            _ => "-".to_string(),
        };
        let mut line = format!(
            "[VisionPerf] call={} status={} provider={:?} model={:?} stream={} \
             source={}x{} input_bytes={} rgba_bytes={} wire={} wire_bytes={} mime={} \
             prompt_bytes={} schema={} timeout_ms={} prepare_ms={} provider_start_ms={} \
             retries={} retry_wait_ms={} \
             first_output_ms={} provider_first_output_ms={} provider_ms={} total_ms={} \
             output_chars={}",
            self.call_id,
            if result.is_ok() { "ok" } else { "error" },
            one_line(&self.provider, usize::MAX),
            one_line(&self.model, usize::MAX),
            self.streaming,
            self.source_width,
            self.source_height,
            optional(self.input_bytes),
            self.rgba_bytes,
            wire_dimensions,
            optional(self.wire_bytes),
            self.wire_mime.as_deref().unwrap_or("-"),
            self.prompt_bytes,
            self.has_schema,
            optional(self.timeout_ms),
            optional(self.prepare_ms),
            optional(self.provider_started_ms),
            self.retry_count,
            self.retry_wait_ms,
            optional(first_output_ms),
            optional(provider_first_output_ms),
            optional(provider_ms),
            total_ms,
            optional(output_chars),
        );
        if let Err(error) = result {
            let _ = write!(
                line,
                " error={:?}",
                one_line(&error.to_string(), MAX_ERROR_CHARS)
            );
        }
        line
    }
}

impl OutputObserver {
    pub(super) fn observe(&mut self, chunk: &str) {
        if self.events.len() >= MAX_OBSERVED_CHUNKS {
            return;
        }
        self.events.push((
            self.started.elapsed().as_millis(),
            chunk.chars().take(MAX_OBSERVED_CHARS).collect(),
        ));
    }

    fn first_output_ms(&self, response: &str, fallback: u128) -> Option<u128> {
        if response.is_empty() {
            return None;
        }
        Some(
            self.events
                .iter()
                .find_map(|(elapsed_ms, chunk)| {
                    let content = chunk
                        .strip_prefix(crate::api::WIPE_SIGNAL)
                        .unwrap_or(chunk)
                        .trim_end_matches('\0');
                    (!content.is_empty() && response.starts_with(content)).then_some(*elapsed_ms)
                })
                .unwrap_or(fallback),
        )
    }
}

fn optional<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or_else(|| "-".to_string(), |value| value.to_string())
}

fn one_line(value: &str, max_chars: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut truncated = normalized.chars().take(max_chars).collect::<String>();
    truncated.push('…');
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};

    #[test]
    fn first_output_ignores_thinking_placeholder() {
        let mut observer = OutputObserver {
            started: Instant::now(),
            events: vec![
                (10, "Model is thinking".to_string()),
                (25, format!("{}Hello", crate::api::WIPE_SIGNAL)),
            ],
        };
        observer.observe("");
        assert_eq!(observer.first_output_ms("Hello world", 90), Some(25));
    }

    #[test]
    fn error_text_is_single_line_and_bounded() {
        let normalized = one_line(&format!("first\n{}\tlast", "x".repeat(400)), 32);
        assert!(!normalized.contains('\n'));
        assert!(!normalized.contains('\t'));
        assert_eq!(normalized.chars().count(), 33);
        assert!(normalized.ends_with('…'));
    }

    #[test]
    fn summary_keeps_content_private_and_reports_retries() {
        let request = TranslateImageRequest {
            groq_api_key: "secret-key",
            gemini_api_key: "",
            prompt: "secret prompt text".to_string(),
            model: "test-model".to_string(),
            provider: "test-provider".to_string(),
            image: ImageBuffer::from_pixel(2, 3, Rgba([1, 2, 3, 255])),
            original_bytes: Some(vec![1, 2, 3]),
            streaming_enabled: false,
            response_schema: None,
            cancel_token: None,
            request_timeout: Some(Duration::from_secs(5)),
        };
        let mut trace = VisionCallTrace::start(&request);
        let observer = trace.output_observer();
        trace.record_prepared(2, 3, 12, "image/png", Duration::from_millis(4));
        trace.mark_provider_started();
        trace.record_retry(Duration::from_millis(125));
        let result = Ok("secret response text".to_string());

        let summary = trace.summary(&result, &observer);
        assert!(summary.contains("retries=1 retry_wait_ms=125"));
        assert!(summary.contains("prompt_bytes=18"));
        assert!(!summary.contains("secret-key"));
        assert!(!summary.contains("secret prompt text"));
        assert!(!summary.contains("secret response text"));
    }
}
