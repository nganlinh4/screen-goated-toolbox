use anyhow::{Context, Result};
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Multipart, Path as AxumPath, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use clap::Parser;
use qwen3_asr_rs::cuda_runtime::{
    force_cuda_requested, maybe_reexec_with_cuda_preload, preload_cuda_runtime,
};
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use qwen3_asr_rs::inference::{AsrInference, KvCacheMode, kv_cache_mode_from_name, kv_cache_mode_name};

fn resolve_kv_cache_mode() -> (KvCacheMode, Option<String>) {
    let requested = std::env::var("SGT_QWEN3_RUNTIME_KV_MODE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let mode = requested
        .as_deref()
        .and_then(kv_cache_mode_from_name)
        .unwrap_or_else(|| {
            if let Some(requested) = requested.as_deref() {
                tracing::warn!(
                    "Unrecognized SGT_QWEN3_RUNTIME_KV_MODE='{}'; defaulting to dense_append",
                    requested
                );
            }
            KvCacheMode::DenseAppend
        });
    (mode, requested)
}
use qwen3_asr_rs::streaming::{StreamingConfig, StreamingState, StreamingTranscript};
use qwen3_asr_rs::tensor::Device;

// ---------------------------------------------------------------------------
// CLI arguments
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(name = "asr-server", about = "OpenAI-compatible ASR API server for Qwen3-ASR")]
struct Args {
    /// Path to the Qwen3-ASR model directory
    #[arg(long)]
    model_dir: String,

    /// Host address to bind to
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Port to listen on
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// Default language for transcription (e.g., chinese, english)
    #[arg(long)]
    language: Option<String>,

    /// Verbose output (-v for debug, -vv for trace)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct AppState {
    model: Arc<Mutex<AsrInference>>,
    default_language: Option<String>,
    sessions: Arc<Mutex<HashMap<u64, Arc<Mutex<StreamingState>>>>>,
    next_session_id: Arc<AtomicU64>,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct TranscriptionResponse {
    text: String,
}

#[derive(Serialize)]
struct VerboseTranscriptionResponse {
    task: String,
    language: String,
    duration: f64,
    text: String,
}

#[derive(Serialize)]
struct ModelObject {
    id: String,
    object: String,
    owned_by: String,
}

#[derive(Serialize)]
struct ModelsResponse {
    object: String,
    data: Vec<ModelObject>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
}

#[derive(Serialize)]
struct StreamingSessionResponse {
    session_id: u64,
}

#[derive(Deserialize, Default)]
struct StreamingTranscriptionRequest {
    language: Option<String>,
    finalize: bool,
}

#[derive(Deserialize, Default)]
struct CreateStreamingSessionRequest {
    chunk_size_ms: Option<u32>,
    unfixed_chunk_num: Option<usize>,
    unfixed_token_num: Option<usize>,
}

#[derive(Serialize)]
struct StreamingTranscriptResponse {
    language: String,
    fixed_text: String,
    draft_text: String,
    text: String,
    kv_cache_bytes: usize,
    kv_cache_dense_bytes: usize,
}

// ---------------------------------------------------------------------------
// Error handling
// ---------------------------------------------------------------------------

struct AppError(anyhow::Error);

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

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

async fn health_handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
    })
}

async fn models_handler() -> Json<ModelsResponse> {
    Json(ModelsResponse {
        object: "list".to_string(),
        data: vec![ModelObject {
            id: "qwen3-asr".to_string(),
            object: "model".to_string(),
            owned_by: "qwen".to_string(),
        }],
    })
}

async fn transcribe_handler(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let mut file_bytes: Option<(String, Vec<u8>)> = None;
    let mut language: Option<String> = None;
    let mut response_format = "json".to_string();

    // Parse multipart fields
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError(anyhow::anyhow!("Multipart error: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let filename = field.file_name().unwrap_or("audio.wav").to_string();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError(anyhow::anyhow!("Failed to read file: {}", e)))?;
                file_bytes = Some((filename, bytes.to_vec()));
            }
            "language" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| AppError(anyhow::anyhow!("Failed to read language: {}", e)))?;
                if !val.is_empty() {
                    language = Some(val);
                }
            }
            "response_format" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| AppError(anyhow::anyhow!("Failed to read response_format: {}", e)))?;
                if !val.is_empty() {
                    response_format = val;
                }
            }
            _ => {
                // Accept and ignore: model, temperature, prompt, etc.
                let _ = field.bytes().await;
            }
        }
    }

    // Validate file
    let (filename, bytes) = file_bytes
        .ok_or_else(|| AppError(anyhow::anyhow!("Missing required field: file")))?;
    if bytes.is_empty() {
        return Err(AppError(anyhow::anyhow!("Uploaded file is empty")));
    }

    // Language: request field > CLI default > None
    let lang = language.or(state.default_language.clone());

    // Write to temp file, preserving extension for ffmpeg format detection
    let extension = Path::new(&filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("wav");
    let mut tmp = tempfile::Builder::new()
        .suffix(&format!(".{}", extension))
        .tempfile()
        .map_err(|e| AppError(anyhow::anyhow!("Failed to create temp file: {}", e)))?;
    tmp.write_all(&bytes)
        .map_err(|e| AppError(anyhow::anyhow!("Failed to write temp file: {}", e)))?;

    let tmp_path = tmp.into_temp_path();
    let tmp_path_str = tmp_path.to_string_lossy().to_string();

    // Run inference in blocking task (GPU-bound)
    let model = state.model.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _keep = tmp_path; // ensure temp file lives until transcription completes
        let model = model
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        model.transcribe(&tmp_path_str, lang.as_deref())
    })
    .await
    .map_err(|e| AppError(anyhow::anyhow!("Blocking task failed: {}", e)))??;

    // Format response
    match response_format.as_str() {
        "text" => Ok((StatusCode::OK, result.text).into_response()),
        "verbose_json" => Ok(Json(VerboseTranscriptionResponse {
            task: "transcribe".to_string(),
            language: result.language,
            duration: result.duration_seconds,
            text: result.text,
        })
        .into_response()),
        _ => Ok(Json(TranscriptionResponse { text: result.text }).into_response()),
    }
}

async fn create_streaming_session_handler(
    State(state): State<AppState>,
    request: Option<Json<CreateStreamingSessionRequest>>,
) -> Result<Json<StreamingSessionResponse>, AppError> {
    let request = request.map(|payload| payload.0).unwrap_or_default();
    let config = StreamingConfig {
        chunk_size_ms: request.chunk_size_ms.unwrap_or(400),
        unfixed_chunk_num: request.unfixed_chunk_num.unwrap_or(2),
        unfixed_token_num: request.unfixed_token_num.unwrap_or(5),
    };
    let streaming_state = {
        let model = state
            .model
            .lock()
            .map_err(|e| AppError(anyhow::anyhow!("Model lock poisoned: {}", e)))?;
        model.init_streaming_state(config)
    };
    let session_id = state.next_session_id.fetch_add(1, Ordering::SeqCst);
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|e| AppError(anyhow::anyhow!("Session lock poisoned: {}", e)))?;
    sessions.insert(session_id, Arc::new(Mutex::new(streaming_state)));
    Ok(Json(StreamingSessionResponse { session_id }))
}

async fn append_streaming_audio_handler(
    AxumPath(session_id): AxumPath<u64>,
    State(state): State<AppState>,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let mut chunks = body.chunks_exact(2);
    if !chunks.remainder().is_empty() {
        return Err(AppError(anyhow::anyhow!(
            "Streaming audio body must contain 16-bit PCM little-endian samples"
        )));
    }

    let new_samples: Vec<i16> = chunks
        .by_ref()
        .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]))
        .collect();

    let session = state
        .sessions
        .lock()
        .map_err(|e| AppError(anyhow::anyhow!("Session lock poisoned: {}", e)))?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| AppError(anyhow::anyhow!("Unknown streaming session: {}", session_id)))?;
    session
        .lock()
        .map_err(|e| AppError(anyhow::anyhow!("Session lock poisoned: {}", e)))?
        .append_pcm16(&new_samples);

    Ok(StatusCode::NO_CONTENT)
}

async fn transcribe_streaming_session_handler(
    AxumPath(session_id): AxumPath<u64>,
    State(state): State<AppState>,
    request: Option<Json<StreamingTranscriptionRequest>>,
) -> Result<Json<StreamingTranscriptResponse>, AppError> {
    let request = request.map(|payload| payload.0).unwrap_or_default();
    let language = request.language.or_else(|| state.default_language.clone());
    let session = state
        .sessions
        .lock()
        .map_err(|e| AppError(anyhow::anyhow!("Session lock poisoned: {}", e)))?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| AppError(anyhow::anyhow!("Unknown streaming session: {}", session_id)))?;

    let model = state.model.clone();
    let (result, kv_cache_bytes, kv_cache_dense_bytes): (
        StreamingTranscript,
        usize,
        usize,
    ) = tokio::task::spawn_blocking(move || {
        let model = model
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock poisoned: {}", e))?;
        let mut session = session
            .lock()
            .map_err(|e| anyhow::anyhow!("Session lock poisoned: {}", e))?;
        let transcript = if request.finalize {
            model.finish_streaming_transcribe(&mut session, language.as_deref())
        } else {
            session.transcribe(&model, language.as_deref())
        }?;
        Ok::<_, anyhow::Error>((
            transcript,
            session.kv_cache_bytes(),
            session.kv_cache_dense_bytes(),
        ))
    })
    .await
    .map_err(|e| AppError(anyhow::anyhow!("Blocking task failed: {}", e)))??;

    Ok(Json(StreamingTranscriptResponse {
        language: result.language,
        fixed_text: result.fixed_text,
        draft_text: result.draft_text,
        text: result.text,
        kv_cache_bytes,
        kv_cache_dense_bytes,
    }))
}

async fn reset_streaming_session_handler(
    AxumPath(session_id): AxumPath<u64>,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    let sessions = state
        .sessions
        .lock()
        .map_err(|e| AppError(anyhow::anyhow!("Session lock poisoned: {}", e)))?;
    let session = sessions
        .get(&session_id)
        .cloned()
        .ok_or_else(|| AppError(anyhow::anyhow!("Unknown streaming session: {}", session_id)))?;
    drop(sessions);
    session
        .lock()
        .map_err(|e| AppError(anyhow::anyhow!("Session lock poisoned: {}", e)))?
        .reset();
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_streaming_session_handler(
    AxumPath(session_id): AxumPath<u64>,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    let mut sessions = state
        .sessions
        .lock()
        .map_err(|e| AppError(anyhow::anyhow!("Session lock poisoned: {}", e)))?;
    if sessions.remove(&session_id).is_none() {
        return Err(AppError(anyhow::anyhow!(
            "Unknown streaming session: {}",
            session_id
        )));
    }
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    if let Err(err) = maybe_reexec_with_cuda_preload() {
        tracing::warn!("Failed to re-exec with Linux CUDA preload: {err}");
    }

    preload_cuda_runtime();

    // Initialize tracing
    let filter = match args.verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(filter)),
        )
        .init();

    // Verify model directory
    let model_dir = Path::new(&args.model_dir);
    if !model_dir.exists() {
        anyhow::bail!("Model directory not found: {}", args.model_dir);
    }

    // Select device
    #[cfg(feature = "tch-backend")]
    let device = if force_cuda_requested() {
        tracing::warn!("Forcing CUDA device selection via environment override");
        Device::Gpu(0)
    } else if tch::Cuda::is_available() {
        tracing::info!("Using CUDA device");
        Device::Gpu(0)
    } else {
        tracing::info!("Using CPU device");
        Device::Cpu
    };

    #[cfg(feature = "mlx")]
    let device = {
        qwen3_asr_rs::backend::mlx::stream::init_mlx(true);
        tracing::info!("Using MLX Metal GPU");
        Device::Gpu(0)
    };

    // Load model
    tracing::info!("Loading model from {:?}", model_dir);
    let (kv_cache_mode, requested_kv_cache_mode) = resolve_kv_cache_mode();
    if let Some(requested_kv_cache_mode) = requested_kv_cache_mode.as_deref() {
        let canonical_kv_cache_mode = kv_cache_mode_name(kv_cache_mode);
        if requested_kv_cache_mode == canonical_kv_cache_mode {
            tracing::info!("Using KV cache mode: {}", canonical_kv_cache_mode);
        } else {
            tracing::info!(
                "Using KV cache mode: {} (requested: {})",
                canonical_kv_cache_mode,
                requested_kv_cache_mode
            );
        }
    } else {
        tracing::info!("Using KV cache mode: {}", kv_cache_mode_name(kv_cache_mode));
    }
    let model = AsrInference::load_with_kv_mode(model_dir, device, kv_cache_mode)
        .context("Failed to load model")?;
    tracing::info!("Model loaded successfully");

    let state = AppState {
        model: Arc::new(Mutex::new(model)),
        default_language: args.language,
        sessions: Arc::new(Mutex::new(HashMap::new())),
        next_session_id: Arc::new(AtomicU64::new(1)),
    };

    // Build router
    let app = Router::new()
        .route("/v1/audio/transcriptions", post(transcribe_handler))
        .route("/v1/audio/streaming/sessions", post(create_streaming_session_handler))
        .route(
            "/v1/audio/streaming/sessions/:session_id/audio",
            post(append_streaming_audio_handler),
        )
        .route(
            "/v1/audio/streaming/sessions/:session_id/transcriptions",
            post(transcribe_streaming_session_handler),
        )
        .route(
            "/v1/audio/streaming/sessions/:session_id/reset",
            post(reset_streaming_session_handler),
        )
        .route(
            "/v1/audio/streaming/sessions/:session_id",
            delete(delete_streaming_session_handler),
        )
        .route("/v1/models", get(models_handler))
        .route("/health", get(health_handler))
        .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .context("Invalid host:port")?;
    tracing::info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
