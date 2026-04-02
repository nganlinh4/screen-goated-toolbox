use qwen3_asr_rs::inference::{
    kv_cache_mode_from_name, kv_cache_mode_name, supported_kv_cache_mode_names, AsrInference,
    KV_CACHE_MODE_DENSE_APPEND,
};
use qwen3_asr_rs::streaming::{StreamingConfig, StreamingState, StreamingTranscript};
use qwen3_asr_rs::tensor::Device;
use qwen3_asr_rs::text_decoder::KvCacheMode;
use serde::Deserialize;
use serde_json::json;
use std::ffi::{c_char, c_void};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

const STATUS_OK: i32 = 0;
const STATUS_ERROR: i32 = 1;
const HANDLE_KIND_RUNTIME: u32 = 0x5152_5254;
const HANDLE_KIND_SESSION: u32 = 0x5153_4553;
const REFERENCE_EXECUTION_MODE: &str = "reference_uncompressed";
const REFERENCE_STREAMING_MODE: &str = "qwen_reference";

lazy_static::lazy_static! {
    static ref LAST_GLOBAL_ERROR_JSON: Mutex<String> =
        Mutex::new(error_payload("No Qwen3 TurboQuant runtime error recorded."));
}

#[derive(Deserialize, Default)]
struct RuntimeConfig {
    model_dir: String,
    #[serde(default)]
    quant_mode: String,
    #[serde(default)]
    kv_cache_mode: String,
    #[serde(default)]
    streaming_mode: String,
}

#[derive(Deserialize, Default)]
struct SessionConfig {
    #[serde(default = "default_sample_rate_hz")]
    sample_rate_hz: u32,
    #[serde(default = "default_chunk_ms")]
    chunk_size_ms: u32,
    #[serde(default = "default_unfixed_chunks")]
    unfixed_chunk_num: usize,
    #[serde(default = "default_unfixed_tokens")]
    unfixed_token_num: usize,
    #[serde(default)]
    language: String,
}

struct RuntimeHandle {
    kind: u32,
    model: Arc<AsrInference>,
    last_error_json: String,
    quant_mode: String,
    kv_cache_mode: String,
    streaming_mode: String,
}

struct SessionHandle {
    kind: u32,
    model: Arc<AsrInference>,
    state: StreamingState,
    last_error_json: String,
    last_payload_json: String,
    language: Option<String>,
    final_pending: bool,
    audio_samples_total: usize,
}

fn default_sample_rate_hz() -> u32 {
    16_000
}

fn default_chunk_ms() -> u32 {
    2_000
}

fn default_unfixed_chunks() -> usize {
    2
}

fn default_unfixed_tokens() -> usize {
    5
}

fn error_payload(message: impl AsRef<str>) -> String {
    json!({ "error": message.as_ref() }).to_string()
}

fn result_payload(
    transcript: &StreamingTranscript,
    audio_samples: usize,
    is_final: bool,
    latency_ms: u128,
    kv_cache_bytes: usize,
    kv_cache_dense_bytes: usize,
) -> String {
    json!({
        "language": transcript.language,
        "text": transcript.text,
        "fixed_text": transcript.fixed_text,
        "draft_text": transcript.draft_text,
        "latency_ms": latency_ms,
        "audio_samples": audio_samples,
        "is_final": is_final,
        "kv_cache_bytes": kv_cache_bytes,
        "kv_cache_dense_bytes": kv_cache_dense_bytes,
    })
    .to_string()
}

fn kv_cache_mode_from_config(mode: &str) -> anyhow::Result<KvCacheMode> {
    kv_cache_mode_from_name(mode).ok_or_else(|| {
        let supported = supported_kv_cache_mode_names();
        anyhow::anyhow!(
            "Unsupported kv_cache_mode '{}'. Supported modes are '{}' and '{}'.",
            mode, supported[0], supported[1]
        )
    })
}

fn set_global_error(message: impl AsRef<str>) {
    *LAST_GLOBAL_ERROR_JSON.lock().unwrap() = error_payload(message);
}

fn parse_json_slice<T: for<'de> Deserialize<'de>>(ptr: *const u8, len: usize) -> anyhow::Result<T> {
    if ptr.is_null() || len == 0 {
        return Ok(serde_json::from_str("{}")?);
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    Ok(serde_json::from_slice(bytes)?)
}

fn select_device() -> anyhow::Result<(Device, &'static str)> {
    if std::env::var("SGT_QWEN3_FORCE_CUDA")
        .ok()
        .or_else(|| std::env::var("QWEN3_FORCE_CUDA").ok())
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
    {
        if tch::Cuda::is_available() {
            return Ok((Device::Gpu(0), "cuda"));
        }
        anyhow::bail!("Qwen3 TurboQuant runtime was forced to CUDA, but CUDA is not available");
    }
    if tch::Cuda::is_available() {
        Ok((Device::Gpu(0), "cuda"))
    } else {
        anyhow::bail!(
            "Qwen3 TurboQuant runtime requires an NVIDIA CUDA-capable GPU. CPU fallback is not supported."
        )
    }
}

fn runtime_handle_from_ptr<'a>(ptr: *mut c_void) -> anyhow::Result<&'a mut RuntimeHandle> {
    if ptr.is_null() {
        anyhow::bail!("Runtime handle was null");
    }
    let handle = unsafe { &mut *(ptr as *mut RuntimeHandle) };
    if handle.kind != HANDLE_KIND_RUNTIME {
        anyhow::bail!("Handle is not a Qwen3 TurboQuant runtime");
    }
    Ok(handle)
}

fn session_handle_from_ptr<'a>(ptr: *mut c_void) -> anyhow::Result<&'a mut SessionHandle> {
    if ptr.is_null() {
        anyhow::bail!("Session handle was null");
    }
    let handle = unsafe { &mut *(ptr as *mut SessionHandle) };
    if handle.kind != HANDLE_KIND_SESSION {
        anyhow::bail!("Handle is not a Qwen3 TurboQuant session");
    }
    Ok(handle)
}

fn write_out_string(payload: &str, out_json: *mut *const c_char, out_len: *mut usize) {
    if !out_json.is_null() {
        unsafe {
            *out_json = payload.as_ptr() as *const c_char;
        }
    }
    if !out_len.is_null() {
        unsafe {
            *out_len = payload.len();
        }
    }
}

fn update_runtime_error(handle: &mut RuntimeHandle, message: impl AsRef<str>) {
    handle.last_error_json = error_payload(message.as_ref());
    set_global_error(message);
}

fn update_session_error(handle: &mut SessionHandle, message: impl AsRef<str>) {
    handle.last_error_json = error_payload(message.as_ref());
    set_global_error(message);
}

fn transcript_from_step(session: &mut SessionHandle) -> anyhow::Result<StreamingTranscript> {
    if session.final_pending {
        session.final_pending = false;
        Ok(session
            .model
            .finish_streaming_transcribe(&mut session.state, session.language.as_deref())?)
    } else {
        Ok(session.model.streaming_transcribe(
            &[],
            &mut session.state,
            session.language.as_deref(),
        )?)
    }
}

#[no_mangle]
pub extern "C" fn sgt_qwen3_runtime_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn sgt_qwen3_probe_cuda(out_json: *mut *const c_char, out_len: *mut usize) -> i32 {
    let payload = json!({
        "supported": tch::Cuda::is_available(),
        "implementation": "reference_rust",
        "quant_mode": REFERENCE_EXECUTION_MODE,
        "kv_cache_mode": KV_CACHE_MODE_DENSE_APPEND,
        "supported_kv_cache_modes": supported_kv_cache_mode_names(),
        "turboquant_kv": true,
        "cuda_devices": if tch::Cuda::is_available() { 1 } else { 0 },
        "message": "Native Qwen3 runtime defaults to the in-process Rust reference engine with an uncompressed dense KV cache. TurboQuant KV cache compression is parity-validated and available via kv_cache_mode."
    })
    .to_string();
    write_out_string(&payload, out_json, out_len);
    STATUS_OK
}

#[no_mangle]
pub extern "C" fn sgt_qwen3_create_runtime(
    config_json: *const u8,
    config_len: usize,
    out_runtime: *mut *mut c_void,
) -> i32 {
    if out_runtime.is_null() {
        set_global_error("Runtime output pointer was null");
        return STATUS_ERROR;
    }

    let config = match parse_json_slice::<RuntimeConfig>(config_json, config_len) {
        Ok(config) => config,
        Err(err) => {
            set_global_error(format!("Failed to parse runtime config JSON: {err}"));
            return STATUS_ERROR;
        }
    };

    if config.model_dir.trim().is_empty() {
        set_global_error("Runtime config did not include a model_dir");
        return STATUS_ERROR;
    }

    let model_path = Path::new(&config.model_dir);
    let (device, _) = match select_device() {
        Ok(selection) => selection,
        Err(err) => {
            set_global_error(err.to_string());
            return STATUS_ERROR;
        }
    };
    let kv_cache_mode = match kv_cache_mode_from_config(&if config.kv_cache_mode.is_empty() {
        KV_CACHE_MODE_DENSE_APPEND.to_string()
    } else {
        config.kv_cache_mode.clone()
    }) {
        Ok(mode) => mode,
        Err(err) => {
            set_global_error(err.to_string());
            return STATUS_ERROR;
        }
    };
    let model = match AsrInference::load_with_kv_mode(model_path, device, kv_cache_mode) {
        Ok(model) => Arc::new(model),
        Err(err) => {
            set_global_error(format!(
                "Failed to load Qwen3 model from '{}': {err}",
                model_path.display()
            ));
            return STATUS_ERROR;
        }
    };

    let handle = Box::new(RuntimeHandle {
        kind: HANDLE_KIND_RUNTIME,
        model,
        last_error_json: error_payload("No Qwen3 TurboQuant runtime error recorded."),
        quant_mode: if config.quant_mode.is_empty() {
            REFERENCE_EXECUTION_MODE.to_string()
        } else {
            config.quant_mode
        },
        kv_cache_mode: kv_cache_mode_name(kv_cache_mode).to_string(),
        streaming_mode: if config.streaming_mode.is_empty() {
            REFERENCE_STREAMING_MODE.to_string()
        } else {
            config.streaming_mode
        },
    });

    unsafe {
        *out_runtime = Box::into_raw(handle) as *mut c_void;
    }
    STATUS_OK
}

#[no_mangle]
pub extern "C" fn sgt_qwen3_destroy_runtime(runtime: *mut c_void) -> i32 {
    if runtime.is_null() {
        return STATUS_OK;
    }
    unsafe {
        drop(Box::from_raw(runtime as *mut RuntimeHandle));
    }
    STATUS_OK
}

#[no_mangle]
pub extern "C" fn sgt_qwen3_create_session(
    runtime: *mut c_void,
    session_json: *const u8,
    session_len: usize,
    out_session: *mut *mut c_void,
) -> i32 {
    if out_session.is_null() {
        set_global_error("Session output pointer was null");
        return STATUS_ERROR;
    }

    let runtime = match runtime_handle_from_ptr(runtime) {
        Ok(handle) => handle,
        Err(err) => {
            set_global_error(err.to_string());
            return STATUS_ERROR;
        }
    };

    let session_config = match parse_json_slice::<SessionConfig>(session_json, session_len) {
        Ok(config) => config,
        Err(err) => {
            update_runtime_error(
                runtime,
                format!("Failed to parse session config JSON: {err}"),
            );
            return STATUS_ERROR;
        }
    };

    if session_config.sample_rate_hz != 16_000 {
        update_runtime_error(
            runtime,
            format!(
                "Qwen3 runtime only supports 16 kHz PCM16 input, got {} Hz",
                session_config.sample_rate_hz
            ),
        );
        return STATUS_ERROR;
    }

    if runtime.streaming_mode != REFERENCE_STREAMING_MODE {
        update_runtime_error(
            runtime,
            format!(
                "Unsupported streaming_mode '{}'. The current native runtime only supports '{}'.",
                runtime.streaming_mode, REFERENCE_STREAMING_MODE
            ),
        );
        return STATUS_ERROR;
    }

    if runtime.quant_mode != REFERENCE_EXECUTION_MODE {
        update_runtime_error(
            runtime,
            format!(
                "Unsupported quant_mode '{}'. The current native runtime only supports '{}'.",
                runtime.quant_mode, REFERENCE_EXECUTION_MODE
            ),
        );
        return STATUS_ERROR;
    }

    let supported_kv_modes = supported_kv_cache_mode_names();
    if !supported_kv_modes.contains(&runtime.kv_cache_mode.as_str()) {
        update_runtime_error(
            runtime,
            format!(
                "Unsupported kv_cache_mode '{}'. The current native runtime only supports '{}' and '{}'.",
                runtime.kv_cache_mode, supported_kv_modes[0], supported_kv_modes[1]
            ),
        );
        return STATUS_ERROR;
    }

    let language = (!session_config.language.trim().is_empty()).then(|| session_config.language);
    let handle = Box::new(SessionHandle {
        kind: HANDLE_KIND_SESSION,
        model: Arc::clone(&runtime.model),
        state: StreamingState::new(StreamingConfig {
            chunk_size_ms: session_config.chunk_size_ms,
            unfixed_chunk_num: session_config.unfixed_chunk_num,
            unfixed_token_num: session_config.unfixed_token_num,
        }),
        last_error_json: error_payload("No Qwen3 TurboQuant session error recorded."),
        last_payload_json: String::new(),
        language,
        final_pending: false,
        audio_samples_total: 0,
    });

    unsafe {
        *out_session = Box::into_raw(handle) as *mut c_void;
    }
    STATUS_OK
}

#[no_mangle]
pub extern "C" fn sgt_qwen3_destroy_session(session: *mut c_void) -> i32 {
    if session.is_null() {
        return STATUS_OK;
    }
    unsafe {
        drop(Box::from_raw(session as *mut SessionHandle));
    }
    STATUS_OK
}

#[no_mangle]
pub extern "C" fn sgt_qwen3_append_pcm16(
    session: *mut c_void,
    samples: *const i16,
    sample_count: usize,
    is_final: i32,
) -> i32 {
    let session = match session_handle_from_ptr(session) {
        Ok(handle) => handle,
        Err(err) => {
            set_global_error(err.to_string());
            return STATUS_ERROR;
        }
    };

    if sample_count > 0 {
        if samples.is_null() {
            update_session_error(session, "append_pcm16 received a null samples pointer");
            return STATUS_ERROR;
        }
        let slice = unsafe { std::slice::from_raw_parts(samples, sample_count) };
        session.state.append_pcm16(slice);
        session.audio_samples_total += sample_count;
    }
    session.final_pending = is_final != 0;
    STATUS_OK
}

#[no_mangle]
pub extern "C" fn sgt_qwen3_step(
    session: *mut c_void,
    out_json: *mut *const c_char,
    out_len: *mut usize,
) -> i32 {
    let session = match session_handle_from_ptr(session) {
        Ok(handle) => handle,
        Err(err) => {
            set_global_error(err.to_string());
            return STATUS_ERROR;
        }
    };

    let is_final = session.final_pending;
    let started_at = Instant::now();
    match transcript_from_step(session) {
        Ok(transcript) => {
            let latency_ms = started_at.elapsed().as_millis();
            session.last_payload_json = result_payload(
                &transcript,
                session.audio_samples_total,
                is_final,
                latency_ms,
                session.state.kv_cache_bytes(),
                session.state.kv_cache_dense_bytes(),
            );
            write_out_string(&session.last_payload_json, out_json, out_len);
            STATUS_OK
        }
        Err(err) => {
            let latency_ms = started_at.elapsed().as_millis();
            update_session_error(session, err.to_string());
            session.last_payload_json = json!({
                "language": "",
                "text": "",
                "fixed_text": "",
                "draft_text": "",
                "latency_ms": latency_ms,
                "audio_samples": session.audio_samples_total,
                "is_final": is_final,
                "kv_cache_bytes": session.state.kv_cache_bytes(),
                "kv_cache_dense_bytes": session.state.kv_cache_dense_bytes(),
                "error": err.to_string(),
            })
            .to_string();
            write_out_string(&session.last_payload_json, out_json, out_len);
            STATUS_ERROR
        }
    }
}

#[no_mangle]
pub extern "C" fn sgt_qwen3_reset_session(session: *mut c_void) -> i32 {
    let session = match session_handle_from_ptr(session) {
        Ok(handle) => handle,
        Err(err) => {
            set_global_error(err.to_string());
            return STATUS_ERROR;
        }
    };
    session.state.reset();
    session.final_pending = false;
    session.audio_samples_total = 0;
    session.last_payload_json.clear();
    session.last_error_json = error_payload("No Qwen3 TurboQuant session error recorded.");
    STATUS_OK
}

#[no_mangle]
pub extern "C" fn sgt_qwen3_last_error(
    handle_or_null: *mut c_void,
    out_json: *mut *const c_char,
    out_len: *mut usize,
) -> i32 {
    if handle_or_null.is_null() {
        let payload = LAST_GLOBAL_ERROR_JSON.lock().unwrap().clone();
        write_out_string(&payload, out_json, out_len);
        return STATUS_OK;
    }

    let kind = unsafe { *(handle_or_null as *const u32) };
    match kind {
        HANDLE_KIND_RUNTIME => {
            let handle = unsafe { &mut *(handle_or_null as *mut RuntimeHandle) };
            write_out_string(&handle.last_error_json, out_json, out_len);
            STATUS_OK
        }
        HANDLE_KIND_SESSION => {
            let handle = unsafe { &mut *(handle_or_null as *mut SessionHandle) };
            write_out_string(&handle.last_error_json, out_json, out_len);
            STATUS_OK
        }
        _ => {
            let payload = error_payload("Unknown Qwen3 TurboQuant handle kind.");
            write_out_string(&payload, out_json, out_len);
            STATUS_OK
        }
    }
}
