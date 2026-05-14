//! Shared loader for libtorch-backed TTS runtime DLLs.
//!
//! This mirrors the pattern used by `src/api/realtime_audio/qwen3/runtime.rs`
//! but is **TTS-shaped** instead of ASR-shaped — instead of `append_pcm16` +
//! streaming `step`, the FFI is a one-shot `synthesize(text) -> pcm16`.
//!
//! Each offline TTS model (Step Audio EditX, Magpie, Voxtral)
//! ships its own `sgt_<model>_runtime.dll` at
//! `native/<model>_runtime/dist/sgt_<model>_runtime.dll`, committed to the
//! `main` branch of the project repo and downloaded via raw.githubusercontent.
//!
//! All DLLs link statically against the libtorch C++ runtime that
//! `qwen3::runtime` already downloads (~2 GB CUDA-12 build). Once libtorch is
//! present in the private bin dir, every TTS DLL reuses the same `torch_*.dll`
//! / `c10*.dll` files — there's no second libtorch copy.
//!
//! The FFI surface every runtime DLL must export (UTF-8 strings throughout,
//! C ABI):
//!
//! ```c
//! // ABI version handshake. Must return SGT_TTS_RUNTIME_ABI_VERSION below
//! // exactly, or the loader rejects the DLL.
//! uint32_t sgt_tts_runtime_version(void);
//!
//! // Create a runtime instance bound to a model directory on disk.
//! // model_dir_utf8/len: path to the directory containing the model weights.
//! // out_runtime: receives an opaque handle on success.
//! // Returns 0 on success, negative on failure.
//! int32_t sgt_tts_create(const char* model_dir_utf8, size_t model_dir_len,
//!                        void** out_runtime);
//!
//! // Free a runtime instance.
//! int32_t sgt_tts_destroy(void* runtime);
//!
//! // Synthesize one utterance.
//! //   text_utf8/text_len: input text
//! //   voice_utf8/voice_len: voice id (may be empty)
//! //   lang_utf8/lang_len: BCP-47 language hint (may be empty)
//! //   speed: 1.0 = natural pace; range typically [0.5, 2.0]
//! // Outputs (all owned by the runtime; freed via sgt_tts_free_audio):
//! //   out_pcm16: pointer to little-endian PCM16 mono samples
//! //   out_pcm_count: number of int16_t samples
//! //   out_sample_rate: native sample rate (Hz)
//! // Returns 0 on success, negative on failure.
//! int32_t sgt_tts_synthesize(void* runtime,
//!                            const char* text_utf8, size_t text_len,
//!                            const char* voice_utf8, size_t voice_len,
//!                            const char* lang_utf8, size_t lang_len,
//!                            float speed,
//!                            const int16_t** out_pcm16,
//!                            size_t* out_pcm_count,
//!                            int32_t* out_sample_rate);
//!
//! // Release a PCM buffer previously returned by sgt_tts_synthesize.
//! int32_t sgt_tts_free_audio(void* runtime, const int16_t* pcm16);
//!
//! // Retrieve the last error message produced by this runtime. The returned
//! // pointer is owned by the runtime and remains valid until the next call.
//! int32_t sgt_tts_last_error(void* runtime, const char** out_message,
//!                            size_t* out_len);
//! ```
//!
//! See `native/<model>_runtime/README.md` for the per-model build instructions.

use anyhow::{Context, Result, anyhow};
use libloading::Library;
use std::ffi::c_void;
use std::os::raw::{c_char, c_float, c_int};
use std::path::{Path, PathBuf};

/// ABI version compiled against. Any runtime DLL must return this exact value
/// from `sgt_tts_runtime_version()` or the loader will refuse to use it.
pub const SGT_TTS_RUNTIME_ABI_VERSION: u32 = 1;

type FnRuntimeVersion = unsafe extern "C" fn() -> u32;
type FnCreate = unsafe extern "C" fn(*const c_char, usize, *mut *mut c_void) -> c_int;
type FnDestroy = unsafe extern "C" fn(*mut c_void) -> c_int;
type FnSynthesize = unsafe extern "C" fn(
    *mut c_void,
    *const c_char,
    usize,
    *const c_char,
    usize,
    *const c_char,
    usize,
    c_float,
    *mut *const i16,
    *mut usize,
    *mut i32,
) -> c_int;
type FnFreeAudio = unsafe extern "C" fn(*mut c_void, *const i16) -> c_int;
type FnLastError = unsafe extern "C" fn(*mut c_void, *mut *const c_char, *mut usize) -> c_int;

pub struct SgtTtsLib {
    _lib: Library,
    _dep_libs: Vec<Library>,
    pub create: FnCreate,
    pub destroy: FnDestroy,
    pub synthesize: FnSynthesize,
    pub free_audio: FnFreeAudio,
    pub last_error: FnLastError,
}

unsafe impl Send for SgtTtsLib {}
unsafe impl Sync for SgtTtsLib {}

/// Owns a model-bound runtime instance. Drop calls `sgt_tts_destroy` once.
pub struct TtsRuntimeHandle {
    handle: *mut c_void,
    lib: &'static SgtTtsLib,
}

unsafe impl Send for TtsRuntimeHandle {}

impl TtsRuntimeHandle {
    /// Run a single synthesis. Returns (pcm16, sample_rate).
    pub fn synthesize(
        &self,
        text: &str,
        voice: &str,
        lang: &str,
        speed: f32,
    ) -> Result<(Vec<i16>, u32)> {
        let mut out_pcm: *const i16 = std::ptr::null();
        let mut out_count: usize = 0;
        let mut out_rate: i32 = 0;
        let code = unsafe {
            (self.lib.synthesize)(
                self.handle,
                text.as_ptr() as *const c_char,
                text.len(),
                voice.as_ptr() as *const c_char,
                voice.len(),
                lang.as_ptr() as *const c_char,
                lang.len(),
                speed,
                &mut out_pcm,
                &mut out_count,
                &mut out_rate,
            )
        };
        if code != 0 || out_pcm.is_null() {
            return Err(anyhow!(
                "sgt_tts_synthesize failed (code={code}): {}",
                self.last_error_message()
            ));
        }
        let samples = unsafe { std::slice::from_raw_parts(out_pcm, out_count).to_vec() };
        unsafe {
            (self.lib.free_audio)(self.handle, out_pcm);
        }
        Ok((samples, out_rate.max(0) as u32))
    }

    fn last_error_message(&self) -> String {
        let mut msg_ptr: *const c_char = std::ptr::null();
        let mut msg_len: usize = 0;
        unsafe {
            let code = (self.lib.last_error)(self.handle, &mut msg_ptr, &mut msg_len);
            if code == 0 && !msg_ptr.is_null() && msg_len > 0 {
                let slice = std::slice::from_raw_parts(msg_ptr as *const u8, msg_len);
                String::from_utf8_lossy(slice).into_owned()
            } else {
                "<unknown>".to_string()
            }
        }
    }
}

impl Drop for TtsRuntimeHandle {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { (self.lib.destroy)(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}

/// Load a TTS runtime DLL from `dll_path`. Verifies the ABI version. The
/// returned `SgtTtsLib` is leaked and lives for the rest of the process — the
/// caller is expected to cache it (one DLL per provider, leaked once).
pub fn load_runtime_dll(dll_path: &Path) -> Result<&'static SgtTtsLib> {
    if !dll_path.exists() {
        return Err(anyhow!(
            "Runtime DLL not found at {}. Build the C++ shim per native/<model>_runtime/README.md and commit it to native/<model>_runtime/dist/.",
            dll_path.display()
        ));
    }

    let dir = dll_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    // Use the same DLL search path setup as the qwen3 loader so libtorch
    // dependencies resolve from the shared private bin dir.
    unsafe {
        use windows::Win32::System::LibraryLoader::SetDllDirectoryW;
        let bin_dir = crate::unpack_dlls::private_bin_dir();
        let dir_str = bin_dir.to_string_lossy();
        let dir_wide: Vec<u16> = dir_str.encode_utf16().chain(std::iter::once(0)).collect();
        let _ = SetDllDirectoryW(windows::core::PCWSTR(dir_wide.as_ptr()));
    }

    // Pre-load libtorch dependency DLLs if present.
    let mut dep_libs: Vec<Library> = Vec::new();
    let bin_dir = crate::unpack_dlls::private_bin_dir();
    for dep in &["torch_cpu.dll", "torch_cuda.dll", "c10.dll", "c10_cuda.dll"] {
        let dep_path = bin_dir.join(dep);
        if dep_path.exists()
            && let Ok(l) = unsafe { Library::new(&dep_path) }
        {
            dep_libs.push(l);
        }
    }
    // Also try the local directory next to the DLL.
    let _ = dir; // currently unused but reserved for per-model dep dirs

    let lib = unsafe {
        Library::new(dll_path)
            .with_context(|| format!("Failed to load runtime DLL at {}", dll_path.display()))?
    };

    unsafe {
        let version: FnRuntimeVersion = *lib
            .get::<FnRuntimeVersion>(b"sgt_tts_runtime_version")
            .context("sgt_tts_runtime_version symbol missing")?;
        let abi = version();
        if abi != SGT_TTS_RUNTIME_ABI_VERSION {
            return Err(anyhow!(
                "Runtime DLL ABI version mismatch at {}: got {abi}, expected {}",
                dll_path.display(),
                SGT_TTS_RUNTIME_ABI_VERSION
            ));
        }

        let create = *lib
            .get::<FnCreate>(b"sgt_tts_create")
            .context("sgt_tts_create symbol missing")?;
        let destroy = *lib
            .get::<FnDestroy>(b"sgt_tts_destroy")
            .context("sgt_tts_destroy symbol missing")?;
        let synthesize = *lib
            .get::<FnSynthesize>(b"sgt_tts_synthesize")
            .context("sgt_tts_synthesize symbol missing")?;
        let free_audio = *lib
            .get::<FnFreeAudio>(b"sgt_tts_free_audio")
            .context("sgt_tts_free_audio symbol missing")?;
        let last_error = *lib
            .get::<FnLastError>(b"sgt_tts_last_error")
            .context("sgt_tts_last_error symbol missing")?;

        let boxed = Box::new(SgtTtsLib {
            _lib: lib,
            _dep_libs: dep_libs,
            create,
            destroy,
            synthesize,
            free_audio,
            last_error,
        });
        Ok(Box::leak(boxed))
    }
}

/// Open a model on disk using an already-loaded runtime DLL.
pub fn create_runtime_handle(
    lib: &'static SgtTtsLib,
    model_dir: &Path,
) -> Result<TtsRuntimeHandle> {
    let dir_str = model_dir.to_string_lossy();
    let dir_bytes = dir_str.as_bytes();
    let mut handle: *mut c_void = std::ptr::null_mut();
    let code = unsafe {
        (lib.create)(
            dir_bytes.as_ptr() as *const c_char,
            dir_bytes.len(),
            &mut handle,
        )
    };
    if code != 0 || handle.is_null() {
        return Err(anyhow!(
            "sgt_tts_create failed (code={code}) for {}",
            model_dir.display()
        ));
    }
    Ok(TtsRuntimeHandle { handle, lib })
}
