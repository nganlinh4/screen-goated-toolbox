//! Runtime-loaded FFI for sherpa-onnx C API — OfflineTts.
//!
//! Mirrors the layout of [`super::ffi`] but only binds the OfflineTts entry
//! points needed to synthesise speech from sherpa-backed TTS bundles on disk. The
//! shared sherpa-onnx-c-api.dll is downloaded on demand by
//! [`super::dlls`]; this module just locates and resolves the symbols.
//!
//! Layout matches sherpa-onnx upstream `c-api.h` exactly (verified against
//! release tag v1.13.2). Keep field order in lock-step with the header when
//! bumping the version.

#![allow(non_camel_case_types)]

use anyhow::{Result, anyhow};
use libloading::Library;
use std::os::raw::{c_char, c_float, c_int, c_void};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub const SHERPA_TTS_LOAD_PROBE_FLAG: &str = "--sherpa-tts-load-probe";
const SHERPA_TTS_LOAD_PROBE_TIMEOUT: Duration = Duration::from_secs(8);

pub type SherpaOnnxOfflineTts = c_void;

// ---- Config structs ----

#[repr(C)]
pub struct SherpaOnnxOfflineTtsVitsModelConfig {
    pub model: *const c_char,
    pub lexicon: *const c_char,
    pub tokens: *const c_char,
    pub data_dir: *const c_char,
    pub noise_scale: c_float,
    pub noise_scale_w: c_float,
    pub length_scale: c_float,
    pub dict_dir: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineTtsMatchaModelConfig {
    pub acoustic_model: *const c_char,
    pub vocoder: *const c_char,
    pub lexicon: *const c_char,
    pub tokens: *const c_char,
    pub data_dir: *const c_char,
    pub noise_scale: c_float,
    pub length_scale: c_float,
    pub dict_dir: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineTtsKokoroModelConfig {
    pub model: *const c_char,
    pub voices: *const c_char,
    pub tokens: *const c_char,
    pub data_dir: *const c_char,
    pub length_scale: c_float,
    pub dict_dir: *const c_char,
    pub lexicon: *const c_char,
    pub lang: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineTtsKittenModelConfig {
    pub model: *const c_char,
    pub voices: *const c_char,
    pub tokens: *const c_char,
    pub data_dir: *const c_char,
    pub length_scale: c_float,
}

#[repr(C)]
pub struct SherpaOnnxOfflineTtsZipvoiceModelConfig {
    pub tokens: *const c_char,
    pub encoder: *const c_char,
    pub decoder: *const c_char,
    pub vocoder: *const c_char,
    pub data_dir: *const c_char,
    pub lexicon: *const c_char,
    pub feat_scale: c_float,
    pub t_shift: c_float,
    pub target_rms: c_float,
    pub guidance_scale: c_float,
}

#[repr(C)]
pub struct SherpaOnnxOfflineTtsPocketModelConfig {
    pub lm_flow: *const c_char,
    pub lm_main: *const c_char,
    pub encoder: *const c_char,
    pub decoder: *const c_char,
    pub text_conditioner: *const c_char,
    pub vocab_json: *const c_char,
    pub token_scores_json: *const c_char,
    pub voice_embedding_cache_capacity: c_int,
}

#[repr(C)]
pub struct SherpaOnnxOfflineTtsSupertonicModelConfig {
    pub duration_predictor: *const c_char,
    pub text_encoder: *const c_char,
    pub vector_estimator: *const c_char,
    pub vocoder: *const c_char,
    pub tts_json: *const c_char,
    pub unicode_indexer: *const c_char,
    pub voice_style: *const c_char,
}

#[repr(C)]
pub struct SherpaOnnxOfflineTtsModelConfig {
    pub vits: SherpaOnnxOfflineTtsVitsModelConfig,
    pub num_threads: c_int,
    pub debug: c_int,
    pub provider: *const c_char,
    pub matcha: SherpaOnnxOfflineTtsMatchaModelConfig,
    pub kokoro: SherpaOnnxOfflineTtsKokoroModelConfig,
    pub kitten: SherpaOnnxOfflineTtsKittenModelConfig,
    pub zipvoice: SherpaOnnxOfflineTtsZipvoiceModelConfig,
    pub pocket: SherpaOnnxOfflineTtsPocketModelConfig,
    pub supertonic: SherpaOnnxOfflineTtsSupertonicModelConfig,
}

#[repr(C)]
pub struct SherpaOnnxOfflineTtsConfig {
    pub model: SherpaOnnxOfflineTtsModelConfig,
    pub rule_fsts: *const c_char,
    pub max_num_sentences: c_int,
    pub rule_fars: *const c_char,
    pub silence_scale: c_float,
}

impl SherpaOnnxOfflineTtsConfig {
    /// Create a zeroed config (all pointers null, all numerics zero). Callers
    /// are expected to populate the model substruct + `provider` + `num_threads`.
    pub fn zeroed() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

/// Mirrors `SherpaOnnxGeneratedAudio` returned by the synthesis call.
/// `samples` is a heap-allocated float32 array of length `n`, sample rate
/// `sample_rate`. Must be freed via `SherpaOnnxDestroyOfflineTtsGeneratedAudio`.
#[repr(C)]
pub struct SherpaOnnxGeneratedAudioStruct {
    pub samples: *const c_float,
    pub n: c_int,
    pub sample_rate: c_int,
}

#[repr(C)]
pub struct SherpaOnnxGenerationConfig {
    pub silence_scale: c_float,
    pub speed: c_float,
    pub sid: c_int,
    pub reference_audio: *const c_float,
    pub reference_audio_len: c_int,
    pub reference_sample_rate: c_int,
    pub reference_text: *const c_char,
    pub num_steps: c_int,
    pub extra: *const c_char,
}

impl SherpaOnnxGenerationConfig {
    pub fn zeroed() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

// ---- Function pointer types ----

type FnCreateTts =
    unsafe extern "C" fn(*const SherpaOnnxOfflineTtsConfig) -> *const SherpaOnnxOfflineTts;
type FnDestroyTts = unsafe extern "C" fn(*const SherpaOnnxOfflineTts);
type FnGenerate = unsafe extern "C" fn(
    *const SherpaOnnxOfflineTts,
    *const c_char,
    c_int,
    c_float,
) -> *const SherpaOnnxGeneratedAudioStruct;
type FnProgressCallbackWithArg = Option<unsafe extern "C" fn(c_int, c_int, *mut c_void) -> c_int>;
type FnGenerateWithConfig = unsafe extern "C" fn(
    *const SherpaOnnxOfflineTts,
    *const c_char,
    *const SherpaOnnxGenerationConfig,
    FnProgressCallbackWithArg,
    *mut c_void,
) -> *const SherpaOnnxGeneratedAudioStruct;
type FnDestroyGenerated = unsafe extern "C" fn(*const SherpaOnnxGeneratedAudioStruct);

pub struct SherpaTtsLib {
    _lib: Library,
    _dep_libs: Vec<Library>,
    pub create_tts: FnCreateTts,
    pub destroy_tts: FnDestroyTts,
    pub generate: FnGenerate,
    pub generate_with_config: FnGenerateWithConfig,
    pub destroy_generated: FnDestroyGenerated,
}

unsafe impl Send for SherpaTtsLib {}
unsafe impl Sync for SherpaTtsLib {}

static SHERPA_TTS_LIB: OnceLock<Result<SherpaTtsLib, String>> = OnceLock::new();

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
            return std::fs::canonicalize(c).unwrap_or_else(|_| c.clone());
        }
    }
    candidates[0].clone()
}

pub fn run_load_probe_process() -> i32 {
    let dir = sherpa_dll_dir();
    let dll_path = dir.join("sherpa-onnx-c-api.dll");
    match load_dlls_for_probe(&dir, &dll_path) {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("[Sherpa-TTS probe] {err}");
            2
        }
    }
}

fn ensure_load_probe_passed() -> Result<()> {
    let current_exe = std::env::current_exe()?;
    let mut child = Command::new(current_exe)
        .arg(SHERPA_TTS_LOAD_PROBE_FLAG)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(());
            }
            return Err(anyhow!(
                "sherpa-onnx TTS DLL load probe failed with exit code {:?}. Reinstall sherpa-onnx from Settings > Downloaded Tools.",
                status.code()
            ));
        }

        if started.elapsed() >= SHERPA_TTS_LOAD_PROBE_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(anyhow!(
                "sherpa-onnx TTS DLL load probe timed out after {}s. Reinstall sherpa-onnx from Settings > Downloaded Tools.",
                SHERPA_TTS_LOAD_PROBE_TIMEOUT.as_secs()
            ));
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}

fn set_dll_search_dir(dir: &std::path::Path) {
    unsafe {
        use windows::Win32::System::LibraryLoader::SetDllDirectoryW;
        let dir_wide: Vec<u16> = dir
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let _ = SetDllDirectoryW(windows::core::PCWSTR(dir_wide.as_ptr()));
    }
}

fn preload_sherpa_deps(dir: &std::path::Path, log_prefix: &str) -> Vec<Library> {
    let mut dep_libs: Vec<Library> = Vec::new();
    for dep in &[
        "onnxruntime.dll",
        "onnxruntime_providers_shared.dll",
        "sherpa-onnx-cxx-api.dll",
    ] {
        let dep_path = dir.join(dep);
        if dep_path.exists() {
            match unsafe { Library::new(&dep_path) } {
                Ok(l) => dep_libs.push(l),
                Err(e) => {
                    crate::log_info!("{log_prefix} Warning: failed to pre-load {dep}: {e}");
                }
            }
        }
    }
    dep_libs
}

fn load_dlls_for_probe(
    dir: &std::path::Path,
    dll_path: &std::path::Path,
) -> std::result::Result<Vec<Library>, String> {
    if !dll_path.exists() {
        return Err(format!("sherpa-onnx-c-api.dll not found at {:?}", dll_path));
    }

    set_dll_search_dir(dir);
    let mut libs = preload_sherpa_deps(dir, "[Sherpa-TTS probe]");
    let lib = unsafe {
        Library::new(dll_path).map_err(|e| format!("Failed to load sherpa-onnx-c-api.dll: {e}"))?
    };
    unsafe {
        lib.get::<FnCreateTts>(b"SherpaOnnxCreateOfflineTts")
            .map_err(|e| e.to_string())?;
        lib.get::<FnDestroyTts>(b"SherpaOnnxDestroyOfflineTts")
            .map_err(|e| e.to_string())?;
        lib.get::<FnGenerate>(b"SherpaOnnxOfflineTtsGenerate")
            .map_err(|e| e.to_string())?;
        lib.get::<FnGenerateWithConfig>(b"SherpaOnnxOfflineTtsGenerateWithConfig")
            .map_err(|e| e.to_string())?;
        lib.get::<FnDestroyGenerated>(b"SherpaOnnxDestroyOfflineTtsGeneratedAudio")
            .map_err(|e| e.to_string())?;
    }
    libs.push(lib);
    Ok(libs)
}

pub fn load() -> Result<&'static SherpaTtsLib> {
    SHERPA_TTS_LIB
        .get_or_init(|| {
            let dir = sherpa_dll_dir();
            let dll_path = dir.join("sherpa-onnx-c-api.dll");

            crate::log_info!("[Sherpa-TTS] Loading from {:?}", dll_path);

            if !dll_path.exists() {
                return Err(format!("sherpa-onnx-c-api.dll not found at {:?}", dll_path));
            }

            ensure_load_probe_passed().map_err(|e| e.to_string())?;

            set_dll_search_dir(&dir);
            let dep_libs = preload_sherpa_deps(&dir, "[Sherpa-TTS]");

            let lib = unsafe {
                Library::new(&dll_path)
                    .map_err(|e| format!("Failed to load sherpa-onnx-c-api.dll: {e}"))?
            };

            unsafe {
                let create_tts = *lib
                    .get::<FnCreateTts>(b"SherpaOnnxCreateOfflineTts")
                    .map_err(|e| e.to_string())?;
                let destroy_tts = *lib
                    .get::<FnDestroyTts>(b"SherpaOnnxDestroyOfflineTts")
                    .map_err(|e| e.to_string())?;
                let generate = *lib
                    .get::<FnGenerate>(b"SherpaOnnxOfflineTtsGenerate")
                    .map_err(|e| e.to_string())?;
                let generate_with_config = *lib
                    .get::<FnGenerateWithConfig>(b"SherpaOnnxOfflineTtsGenerateWithConfig")
                    .map_err(|e| e.to_string())?;
                let destroy_generated = *lib
                    .get::<FnDestroyGenerated>(b"SherpaOnnxDestroyOfflineTtsGeneratedAudio")
                    .map_err(|e| e.to_string())?;
                Ok(SherpaTtsLib {
                    _lib: lib,
                    _dep_libs: dep_libs,
                    create_tts: std::mem::transmute::<*const c_void, FnCreateTts>(
                        create_tts as *const c_void,
                    ),
                    destroy_tts: std::mem::transmute::<*const c_void, FnDestroyTts>(
                        destroy_tts as *const c_void,
                    ),
                    generate: std::mem::transmute::<*const c_void, FnGenerate>(
                        generate as *const c_void,
                    ),
                    generate_with_config: std::mem::transmute::<*const c_void, FnGenerateWithConfig>(
                        generate_with_config as *const c_void,
                    ),
                    destroy_generated: std::mem::transmute::<*const c_void, FnDestroyGenerated>(
                        destroy_generated as *const c_void,
                    ),
                })
            }
        })
        .as_ref()
        .map_err(|e| anyhow!("{e}"))
}
