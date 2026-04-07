//! Runtime-loaded FFI for sherpa-onnx C API (streaming online recognizer).

#![allow(non_camel_case_types)]

use anyhow::{Result, anyhow};
use libloading::Library;
use std::os::raw::{c_char, c_float, c_void};
use std::sync::OnceLock;

// Opaque types
pub type SherpaOnnxOnlineRecognizer = c_void;
pub type SherpaOnnxOnlineStream = c_void;

// ---- Config structs (must match C ABI layout exactly) ----

#[repr(C)]
pub struct SherpaOnnxOnlineTransducerModelConfig {
    pub encoder: *const c_char,
    pub decoder: *const c_char,
    pub joiner: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOnlineParaformerModelConfig {
    pub encoder: *const c_char,
    pub decoder: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOnlineZipformer2CtcModelConfig {
    pub model: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOnlineNemoCtcModelConfig {
    pub model: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOnlineToneCtcModelConfig {
    pub model: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOnlineModelConfig {
    pub transducer: SherpaOnnxOnlineTransducerModelConfig,
    pub paraformer: SherpaOnnxOnlineParaformerModelConfig,
    pub zipformer2_ctc: SherpaOnnxOnlineZipformer2CtcModelConfig,
    pub tokens: *const c_char,
    pub num_threads: i32,
    pub provider: *const c_char,
    pub debug: i32,
    pub model_type: *const c_char,
    pub modeling_unit: *const c_char,
    pub bpe_vocab: *const c_char,
    pub tokens_buf: *const c_char,
    pub tokens_buf_size: i32,
    pub nemo_ctc: SherpaOnnxOnlineNemoCtcModelConfig,
    pub t_one_ctc: SherpaOnnxOnlineToneCtcModelConfig,
}

#[repr(C)]
pub struct SherpaOnnxFeatureConfig {
    pub sample_rate: i32,
    pub feature_dim: i32,
}

#[repr(C)]
pub struct SherpaOnnxOnlineCtcFstDecoderConfig {
    pub graph: *const c_char,
    pub max_active: i32,
}

#[repr(C)]
pub struct SherpaOnnxHomophoneReplacerConfig {
    pub dict_dir: *const c_char,
    pub lexicon: *const c_char,
    pub rule_fsts: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOnlineRecognizerConfig {
    pub feat_config: SherpaOnnxFeatureConfig,
    pub model_config: SherpaOnnxOnlineModelConfig,
    pub decoding_method: *const c_char,
    pub max_active_paths: i32,
    pub enable_endpoint: i32,
    pub rule1_min_trailing_silence: f32,
    pub rule2_min_trailing_silence: f32,
    pub rule3_min_utterance_length: f32,
    pub hotwords_file: *const c_char,
    pub hotwords_score: f32,
    pub ctc_fst_decoder_config: SherpaOnnxOnlineCtcFstDecoderConfig,
    pub rule_fsts: *const c_char,
    pub rule_fars: *const c_char,
    pub blank_penalty: f32,
    pub hotwords_buf: *const c_char,
    pub hotwords_buf_size: i32,
    pub hr: SherpaOnnxHomophoneReplacerConfig,
}

impl SherpaOnnxOnlineRecognizerConfig {
    /// Create a zeroed config (all pointers null, all ints/floats zero).
    pub fn zeroed() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

// ---- Function pointer types ----

type FnCreate = unsafe extern "C" fn(
    *const SherpaOnnxOnlineRecognizerConfig,
) -> *const SherpaOnnxOnlineRecognizer;
type FnDestroy = unsafe extern "C" fn(*const SherpaOnnxOnlineRecognizer);
type FnCreateStream =
    unsafe extern "C" fn(*const SherpaOnnxOnlineRecognizer) -> *const SherpaOnnxOnlineStream;
type FnDestroyStream = unsafe extern "C" fn(*const SherpaOnnxOnlineStream);
type FnAcceptWaveform =
    unsafe extern "C" fn(*const SherpaOnnxOnlineStream, i32, *const c_float, i32);
type FnIsReady =
    unsafe extern "C" fn(*const SherpaOnnxOnlineRecognizer, *const SherpaOnnxOnlineStream) -> i32;
type FnDecode =
    unsafe extern "C" fn(*const SherpaOnnxOnlineRecognizer, *const SherpaOnnxOnlineStream);
type FnGetResultJson = unsafe extern "C" fn(
    *const SherpaOnnxOnlineRecognizer,
    *const SherpaOnnxOnlineStream,
) -> *const c_char;
type FnDestroyResultJson = unsafe extern "C" fn(*const c_char);
type FnIsEndpoint =
    unsafe extern "C" fn(*const SherpaOnnxOnlineRecognizer, *const SherpaOnnxOnlineStream) -> i32;
type FnReset =
    unsafe extern "C" fn(*const SherpaOnnxOnlineRecognizer, *const SherpaOnnxOnlineStream);

pub struct SherpaLib {
    _lib: Library,
    _dep_libs: Vec<Library>,
    pub create: FnCreate,
    pub destroy: FnDestroy,
    pub create_stream: FnCreateStream,
    pub destroy_stream: FnDestroyStream,
    pub accept_waveform: FnAcceptWaveform,
    pub is_ready: FnIsReady,
    pub decode: FnDecode,
    pub get_result_json: FnGetResultJson,
    pub destroy_result_json: FnDestroyResultJson,
    pub is_endpoint: FnIsEndpoint,
    pub reset: FnReset,
}

unsafe impl Send for SherpaLib {}
unsafe impl Sync for SherpaLib {}

static SHERPA_LIB: OnceLock<Result<SherpaLib, String>> = OnceLock::new();

fn sherpa_dll_dir() -> std::path::PathBuf {
    let private_bin = crate::unpack_dlls::private_bin_dir();
    let sherpa_bin = private_bin.join("sherpa-onnx");
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let candidates = [
        sherpa_bin,
        private_bin,
        exe_dir,
        std::path::PathBuf::from("third_party/sherpa-onnx-win/lib"),
    ];

    for c in &candidates {
        if c.join("sherpa-onnx-c-api.dll").exists() {
            // Canonicalize so LoadLibraryExW gets absolute path
            return std::fs::canonicalize(c).unwrap_or_else(|_| c.clone());
        }
    }
    candidates[0].clone()
}

pub fn load() -> Result<&'static SherpaLib> {
    SHERPA_LIB
        .get_or_init(|| {
            let dir = sherpa_dll_dir();
            let dll_path = dir.join("sherpa-onnx-c-api.dll");

            crate::log_info!("[Sherpa] Loading from {:?}", dll_path);

            if !dll_path.exists() {
                return Err(format!("sherpa-onnx-c-api.dll not found at {:?}", dll_path));
            }

            // Set DLL search directory
            unsafe {
                use windows::Win32::System::LibraryLoader::SetDllDirectoryW;
                let dir_wide: Vec<u16> = dir
                    .to_string_lossy()
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let _ = SetDllDirectoryW(windows::core::PCWSTR(dir_wide.as_ptr()));
            }

            // Pre-load dependency DLLs in order
            let mut dep_libs: Vec<Library> = Vec::new();
            for dep in &[
                "onnxruntime.dll",
                "onnxruntime_providers_shared.dll",
                "sherpa-onnx-cxx-api.dll",
            ] {
                let dep_path = dir.join(dep);
                if dep_path.exists() {
                    match unsafe { Library::new(&dep_path) } {
                        Ok(l) => {
                            crate::log_info!("[Sherpa] Pre-loaded {dep}");
                            dep_libs.push(l);
                        }
                        Err(e) => {
                            crate::log_info!("[Sherpa] Warning: failed to pre-load {dep}: {e}");
                        }
                    }
                }
            }

            let lib = unsafe {
                Library::new(&dll_path)
                    .map_err(|e| format!("Failed to load sherpa-onnx-c-api.dll: {e}"))?
            };

            unsafe {
                let create = *lib
                    .get::<FnCreate>(b"SherpaOnnxCreateOnlineRecognizer")
                    .map_err(|e| e.to_string())?;
                let destroy = *lib
                    .get::<FnDestroy>(b"SherpaOnnxDestroyOnlineRecognizer")
                    .map_err(|e| e.to_string())?;
                let create_stream = *lib
                    .get::<FnCreateStream>(b"SherpaOnnxCreateOnlineStream")
                    .map_err(|e| e.to_string())?;
                let destroy_stream = *lib
                    .get::<FnDestroyStream>(b"SherpaOnnxDestroyOnlineStream")
                    .map_err(|e| e.to_string())?;
                let accept_waveform = *lib
                    .get::<FnAcceptWaveform>(b"SherpaOnnxOnlineStreamAcceptWaveform")
                    .map_err(|e| e.to_string())?;
                let is_ready = *lib
                    .get::<FnIsReady>(b"SherpaOnnxIsOnlineStreamReady")
                    .map_err(|e| e.to_string())?;
                let decode = *lib
                    .get::<FnDecode>(b"SherpaOnnxDecodeOnlineStream")
                    .map_err(|e| e.to_string())?;
                let get_result_json = *lib
                    .get::<FnGetResultJson>(b"SherpaOnnxGetOnlineStreamResultAsJson")
                    .map_err(|e| e.to_string())?;
                let destroy_result_json = *lib
                    .get::<FnDestroyResultJson>(b"SherpaOnnxDestroyOnlineStreamResultJson")
                    .map_err(|e| e.to_string())?;
                let is_endpoint = *lib
                    .get::<FnIsEndpoint>(b"SherpaOnnxOnlineStreamIsEndpoint")
                    .map_err(|e| e.to_string())?;
                let reset = *lib
                    .get::<FnReset>(b"SherpaOnnxOnlineStreamReset")
                    .map_err(|e| e.to_string())?;

                Ok(SherpaLib {
                    _lib: lib,
                    _dep_libs: dep_libs,
                    create: std::mem::transmute(create),
                    destroy: std::mem::transmute(destroy),
                    create_stream: std::mem::transmute(create_stream),
                    destroy_stream: std::mem::transmute(destroy_stream),
                    accept_waveform: std::mem::transmute(accept_waveform),
                    is_ready: std::mem::transmute(is_ready),
                    decode: std::mem::transmute(decode),
                    get_result_json: std::mem::transmute(get_result_json),
                    destroy_result_json: std::mem::transmute(destroy_result_json),
                    is_endpoint: std::mem::transmute(is_endpoint),
                    reset: std::mem::transmute(reset),
                })
            }
        })
        .as_ref()
        .map_err(|e| anyhow!("{e}"))
}
