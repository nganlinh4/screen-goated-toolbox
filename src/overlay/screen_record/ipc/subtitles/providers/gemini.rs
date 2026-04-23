use std::thread;
use std::time::{Duration, Instant};

use base64::{Engine as _, engine::general_purpose};
use serde_json::{Value, json};

use crate::APP;
use crate::api::client::UREQ_AGENT;
use crate::model_config::get_model_by_id;
use crate::overlay::screen_record::ipc::subtitles::media::PreparedSubtitleMedia;
use crate::overlay::screen_record::ipc::subtitles::types::CompactSubtitleSegment;

use super::gemini_segments::{parse_gemini_segments_from_text, parse_streamed_segment_prefix};
use super::gemini_stream::stream_gemini_text_chunks;
use super::{SubtitleBackend, SubtitleBackendProgress, SubtitleBackendRequest};

const GEMINI_SUBTITLE_MODEL_ID: &str = "gemini-audio-3.1-flash-lite";
const GEMINI_INLINE_WAV_LIMIT_BYTES: usize = 14 * 1024 * 1024;
const GEMINI_FILE_PROCESSING_POLL_INTERVAL: Duration = Duration::from_secs(2);
const GEMINI_FILE_PROCESSING_TIMEOUT: Duration = Duration::from_secs(120);

pub struct GeminiSubtitleBackend {
    api_key: String,
    model_name: String,
}

impl GeminiSubtitleBackend {
    pub fn new() -> Result<Self, String> {
        let app = APP.lock().map_err(|_| "APP lock poisoned".to_string())?;
        if !app.config.use_gemini {
            return Err("PROVIDER_DISABLED:google".to_string());
        }
        if app.config.gemini_api_key.trim().is_empty() {
            return Err("NO_API_KEY:google".to_string());
        }
        let model = get_model_by_id(GEMINI_SUBTITLE_MODEL_ID)
            .ok_or_else(|| format!("{GEMINI_SUBTITLE_MODEL_ID} model config missing"))?;
        Ok(Self {
            api_key: app.config.gemini_api_key.clone(),
            model_name: model.full_name,
        })
    }
}

impl SubtitleBackend for GeminiSubtitleBackend {
    fn transcribe_clip(
        &mut self,
        request: SubtitleBackendRequest,
        on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
    ) -> Result<Vec<CompactSubtitleSegment>, String> {
        let clip_duration_sec = request.media.duration_sec;
        let prompt = build_gemini_subtitle_prompt();
        let segments = transcribe_with_gemini_structured(
            &self.api_key,
            &self.model_name,
            &prompt,
            &request.media,
            clip_duration_sec,
            on_progress,
        )?;
        on_progress(SubtitleBackendProgress {
            completed_steps: 1,
            total_steps: 1,
            segments: segments.clone(),
        })?;
        Ok(segments)
    }
}

#[derive(Clone, Debug)]
struct UploadedGeminiFile {
    name: String,
    uri: String,
    mime_type: String,
    state: Option<String>,
}

fn build_gemini_subtitle_prompt() -> String {
    format!(
        "Generate subtitle segments for this media clip.\n\
Detect the spoken language automatically and transcribe it verbatim. Do not translate.\n\
\n\
Rules:\n\
- Return JSON only and match the provided schema exactly.\n\
- Use integer millisecond timestamps relative to the start of the clip.\n\
- Segments must be sorted by start_ms, non-overlapping, and strictly increasing.\n\
- Each segment should be a short readable subtitle phrase or sentence, not a paragraph.\n\
- Keep punctuation natural.\n\
- Do not add speaker labels, summaries, notes, markdown, or commentary.\n\
- Do not invent text that is not present in the audio.\n\
- Avoid ultra-short fragments unless the audio itself is that brief.\n"
    )
}

fn transcribe_with_gemini_structured(
    api_key: &str,
    model_name: &str,
    prompt: &str,
    media: &PreparedSubtitleMedia,
    clip_duration_sec: f64,
    on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
) -> Result<Vec<CompactSubtitleSegment>, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{model_name}:streamGenerateContent?alt=sse"
    );
    let mut streamed_text = String::new();
    let mut published_count = 0usize;

    let full_text = if should_use_inline_media(media) {
        crate::log_info!(
            "[SubtitleGen][Gemini] request mode=inline-stream model={} mime_type={} bytes={}",
            model_name,
            media.mime_type,
            media.bytes.len()
        );
        let body = build_inline_request(model_name, prompt, media);
        stream_gemini_text_chunks(api_key, &url, body, |chunk| {
            streamed_text.push_str(chunk);
            maybe_publish_streamed_segments(
                &streamed_text,
                clip_duration_sec,
                &mut published_count,
                on_progress,
            )
        })?
    } else {
        crate::log_info!(
            "[SubtitleGen][Gemini] request mode=files-api-stream model={} mime_type={} bytes={}",
            model_name,
            media.mime_type,
            media.bytes.len()
        );
        let uploaded = ensure_gemini_file_active(api_key, upload_gemini_file(api_key, media)?)?;
        let body = build_file_request(model_name, prompt, &uploaded);
        let response = stream_gemini_text_chunks(api_key, &url, body, |chunk| {
            streamed_text.push_str(chunk);
            maybe_publish_streamed_segments(
                &streamed_text,
                clip_duration_sec,
                &mut published_count,
                on_progress,
            )
        });
        delete_gemini_file(api_key, &uploaded.name);
        response?
    };

    parse_gemini_segments_from_text(&full_text, clip_duration_sec)
}

fn should_use_inline_media(media: &PreparedSubtitleMedia) -> bool {
    media.mime_type == "audio/wav" && media.bytes.len() <= GEMINI_INLINE_WAV_LIMIT_BYTES
}

fn build_inline_request(model_name: &str, prompt: &str, media: &PreparedSubtitleMedia) -> Value {
    let media_b64 = general_purpose::STANDARD.encode(&media.bytes);
    build_gemini_request(
        json!({
            "inlineData": {
                "mimeType": media.mime_type,
                "data": media_b64,
            }
        }),
        model_name,
        prompt,
    )
}

fn build_file_request(model_name: &str, prompt: &str, uploaded: &UploadedGeminiFile) -> Value {
    build_gemini_request(
        json!({
            "fileData": {
                "mimeType": uploaded.mime_type,
                "fileUri": uploaded.uri,
            }
        }),
        model_name,
        prompt,
    )
}

fn build_gemini_request(audio_part: Value, model_name: &str, prompt: &str) -> Value {
    let mut payload = json!({
        "contents": [{
            "role": "user",
            "parts": [
                { "text": prompt },
                audio_part,
            ]
        }],
        "generationConfig": {
            "responseMimeType": "application/json",
            "responseJsonSchema": {
                "type": "object",
                "additionalProperties": false,
                "propertyOrdering": ["segments"],
                "properties": {
                    "segments": {
                        "type": "array",
                        "description": "Ordered subtitle segments spanning the spoken audio.",
                        "minItems": 1,
                        "items": {
                            "type": "object",
                            "additionalProperties": false,
                            "propertyOrdering": ["start_ms", "end_ms", "text"],
                            "properties": {
                                "start_ms": {
                                    "type": "integer",
                                    "minimum": 0,
                                    "description": "Segment start timestamp in milliseconds from clip start."
                                },
                                "end_ms": {
                                    "type": "integer",
                                    "minimum": 0,
                                    "description": "Segment end timestamp in milliseconds from clip start."
                                },
                                "text": {
                                    "type": "string",
                                    "description": "Verbatim subtitle text for the segment."
                                }
                            },
                            "required": ["start_ms", "end_ms", "text"]
                        }
                    }
                },
                "required": ["segments"]
            }
        }
    });

    if let Some(thinking_config) = crate::api::gemini_thinking_config(model_name) {
        payload["generationConfig"]["thinkingConfig"] = serde_json::to_value(thinking_config)
            .unwrap_or_else(|_| json!({ "thinkingBudget": 0 }));
    }

    payload
}

pub(super) fn map_gemini_request_error(error: ureq::Error) -> String {
    let error_string = error.to_string();
    if error_string.contains("401") || error_string.contains("403") {
        "INVALID_API_KEY".to_string()
    } else {
        format!("Gemini subtitle request failed: {error_string}")
    }
}

fn upload_gemini_file(
    api_key: &str,
    media: &PreparedSubtitleMedia,
) -> Result<UploadedGeminiFile, String> {
    let start_response = UREQ_AGENT
        .post("https://generativelanguage.googleapis.com/upload/v1beta/files")
        .header("x-goog-api-key", api_key)
        .header("X-Goog-Upload-Protocol", "resumable")
        .header("X-Goog-Upload-Command", "start")
        .header(
            "X-Goog-Upload-Header-Content-Length",
            &media.bytes.len().to_string(),
        )
        .header("X-Goog-Upload-Header-Content-Type", &media.mime_type)
        .send_json(json!({
            "file": {
                "display_name": media.file_name
            }
        }))
        .map_err(map_gemini_request_error)?;

    let upload_url = start_response
        .headers()
        .get("x-goog-upload-url")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Gemini Files API did not return an upload URL".to_string())?
        .to_string();

    let finalize_response = UREQ_AGENT
        .post(&upload_url)
        .header("Content-Length", &media.bytes.len().to_string())
        .header("X-Goog-Upload-Offset", "0")
        .header("X-Goog-Upload-Command", "upload, finalize")
        .send(&media.bytes)
        .map_err(map_gemini_request_error)?;

    let file_json: Value = finalize_response
        .into_body()
        .read_json()
        .map_err(|e| format!("Decode Gemini uploaded file metadata: {e}"))?;

    let file = file_json
        .get("file")
        .ok_or_else(|| "Gemini Files API upload metadata missing file object".to_string())?;

    parse_uploaded_gemini_file(file)
}

fn delete_gemini_file(api_key: &str, file_name: &str) {
    let delete_url = format!("https://generativelanguage.googleapis.com/v1beta/{file_name}");
    if let Err(error) = UREQ_AGENT
        .delete(&delete_url)
        .header("x-goog-api-key", api_key)
        .call()
    {
        crate::log_info!(
            "[SubtitleGen][Gemini] cleanup delete failed file={} error={}",
            file_name,
            error
        );
    }
}

fn ensure_gemini_file_active(
    api_key: &str,
    uploaded: UploadedGeminiFile,
) -> Result<UploadedGeminiFile, String> {
    let mut current = uploaded;
    let deadline = Instant::now() + GEMINI_FILE_PROCESSING_TIMEOUT;
    loop {
        match current.state.as_deref() {
            None | Some("ACTIVE") => return Ok(current),
            Some("FAILED") => {
                return Err(format!(
                    "Gemini Files API failed to process file {}",
                    current.name
                ));
            }
            Some("PROCESSING") => {
                if Instant::now() >= deadline {
                    return Err(format!(
                        "Gemini Files API file {} did not become ACTIVE before timeout",
                        current.name
                    ));
                }
                crate::log_info!(
                    "[SubtitleGen][Gemini] waiting for uploaded file to become ACTIVE file={}",
                    current.name
                );
                thread::sleep(GEMINI_FILE_PROCESSING_POLL_INTERVAL);
                current = get_gemini_file(api_key, &current.name)?;
            }
            Some(other) => return Err(format!("Unexpected Gemini file state {other}")),
        }
    }
}

fn get_gemini_file(api_key: &str, file_name: &str) -> Result<UploadedGeminiFile, String> {
    let url = format!("https://generativelanguage.googleapis.com/v1beta/{file_name}");
    let response = UREQ_AGENT
        .get(&url)
        .header("x-goog-api-key", api_key)
        .call()
        .map_err(map_gemini_request_error)?;
    let file_json: Value = response
        .into_body()
        .read_json()
        .map_err(|e| format!("Decode Gemini file metadata: {e}"))?;
    let file = file_json.get("file").unwrap_or(&file_json);
    parse_uploaded_gemini_file(file)
}

fn parse_uploaded_gemini_file(file: &Value) -> Result<UploadedGeminiFile, String> {
    let name = file
        .get("name")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Gemini uploaded file metadata missing name".to_string())?
        .to_string();
    let uri = file
        .get("uri")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Gemini uploaded file metadata missing uri".to_string())?
        .to_string();
    let mime_type = file
        .get("mimeType")
        .or_else(|| file.get("mime_type"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Gemini uploaded file metadata missing mimeType".to_string())?
        .to_string();
    let state = file.get("state").map(parse_gemini_file_state).transpose()?;
    Ok(UploadedGeminiFile {
        name,
        uri,
        mime_type,
        state,
    })
}

fn parse_gemini_file_state(value: &Value) -> Result<String, String> {
    if let Some(state) = value.as_str() {
        return Ok(state.to_string());
    }
    if let Some(state_name) = value.get("name").and_then(|state| state.as_str()) {
        return Ok(state_name.to_string());
    }
    Err("Gemini uploaded file metadata had invalid state".to_string())
}

fn maybe_publish_streamed_segments(
    streamed_text: &str,
    clip_duration_sec: f64,
    published_count: &mut usize,
    on_progress: &mut dyn FnMut(SubtitleBackendProgress) -> Result<(), String>,
) -> Result<(), String> {
    let segments = parse_streamed_segment_prefix(streamed_text, clip_duration_sec)?;
    if segments.len() <= *published_count {
        return Ok(());
    }
    *published_count = segments.len();
    on_progress(SubtitleBackendProgress {
        completed_steps: 0,
        total_steps: 1,
        segments,
    })
}
