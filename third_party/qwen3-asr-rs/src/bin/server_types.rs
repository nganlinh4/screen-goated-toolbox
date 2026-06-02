use axum::{
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};

use super::server_segmentation::TimedTranscriptSegment;

#[derive(Serialize)]
pub(crate) struct TranscriptionResponse {
    pub(crate) text: String,
}

#[derive(Serialize)]
pub(crate) struct VerboseTranscriptionResponse {
    pub(crate) task: String,
    pub(crate) language: String,
    pub(crate) duration: f64,
    pub(crate) text: String,
    pub(crate) segments: Vec<TimedTranscriptSegment>,
}

#[derive(Serialize)]
pub(crate) struct ModelObject {
    pub(crate) id: String,
    pub(crate) object: String,
    pub(crate) owned_by: String,
}

#[derive(Serialize)]
pub(crate) struct ModelsResponse {
    pub(crate) object: String,
    pub(crate) data: Vec<ModelObject>,
}

#[derive(Serialize)]
pub(crate) struct HealthResponse {
    pub(crate) status: String,
}

#[derive(Serialize)]
pub(crate) struct StreamingSessionResponse {
    pub(crate) session_id: u64,
}

#[derive(Deserialize, Default)]
pub(crate) struct StreamingTranscriptionRequest {
    pub(crate) language: Option<String>,
    pub(crate) finalize: bool,
}

#[derive(Deserialize, Default)]
pub(crate) struct CreateStreamingSessionRequest {
    pub(crate) chunk_size_ms: Option<u32>,
    pub(crate) unfixed_chunk_num: Option<usize>,
    pub(crate) unfixed_token_num: Option<usize>,
}

#[derive(Serialize)]
pub(crate) struct StreamingTranscriptResponse {
    pub(crate) language: String,
    pub(crate) fixed_text: String,
    pub(crate) draft_text: String,
    pub(crate) text: String,
    pub(crate) kv_cache_bytes: usize,
    pub(crate) kv_cache_dense_bytes: usize,
}

pub(crate) struct AppError(pub(crate) anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        tracing::error!("Request error: {:?}", self.0);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": {
                    "message": self.0.to_string(),
                    "type": "server_error",
                }
            })),
        )
            .into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        Self(err)
    }
}
