//! Runtime-loaded FFI bindings to the Moonshine Voice C API.
//!
//! Loads moonshine.lib + onnxruntime.dll dynamically at runtime via libloading
//! to avoid CRT mismatch (Moonshine libs use /MD, project uses /MT).

#![allow(non_camel_case_types, dead_code)]

use anyhow::{Result, anyhow};
use libloading::Library;
use std::os::raw::{c_char, c_float};
use std::sync::OnceLock;

pub const MOONSHINE_HEADER_VERSION: i32 = 20000;
pub const MOONSHINE_MODEL_ARCH_TINY: u32 = 0;
pub const MOONSHINE_MODEL_ARCH_BASE: u32 = 1;
pub const MOONSHINE_MODEL_ARCH_TINY_STREAMING: u32 = 2;
pub const MOONSHINE_MODEL_ARCH_SMALL_STREAMING: u32 = 4;
pub const MOONSHINE_MODEL_ARCH_MEDIUM_STREAMING: u32 = 5;
pub const MOONSHINE_ERROR_NONE: i32 = 0;

#[repr(C)]
pub struct transcriber_option_t {
    pub name: *const c_char,
    pub value: *const c_char,
}

#[repr(C)]
pub struct transcript_word_t {
    pub text: *const c_char,
    pub start: c_float,
    pub end: c_float,
    pub confidence: c_float,
}

#[repr(C)]
pub struct transcript_line_t {
    pub text: *const c_char,
    pub audio_data: *const c_float,
    pub audio_data_count: usize,
    pub start_time: c_float,
    pub duration: c_float,
    pub id: u64,
    pub is_complete: i8,
    pub is_updated: i8,
    pub is_new: i8,
    pub has_text_changed: i8,
    pub has_speaker_id: i8,
    pub speaker_id: u64,
    pub speaker_index: u32,
    pub last_transcription_latency_ms: u32,
    pub words: *const transcript_word_t,
    pub word_count: u64,
}

#[repr(C)]
pub struct transcript_t {
    pub lines: *mut transcript_line_t,
    pub line_count: u64,
}

// Function pointer types
type FnLoadTranscriber = unsafe extern "C" fn(*const c_char, u32, *const transcriber_option_t, u64, i32) -> i32;
type FnFreeTranscriber = unsafe extern "C" fn(i32);
type FnCreateStream = unsafe extern "C" fn(i32, u32) -> i32;
type FnFreeStream = unsafe extern "C" fn(i32, i32) -> i32;
type FnStartStream = unsafe extern "C" fn(i32, i32) -> i32;
type FnStopStream = unsafe extern "C" fn(i32, i32) -> i32;
type FnAddAudio = unsafe extern "C" fn(i32, i32, *const c_float, u64, i32, u32) -> i32;
type FnTranscribeStream = unsafe extern "C" fn(i32, i32, u32, *mut *mut transcript_t) -> i32;

/// Holds the dynamically loaded Moonshine library and function pointers.
pub struct MoonshineLib {
    _lib: Library,
    _ort_lib: Option<Library>,
    pub load_transcriber: FnLoadTranscriber,
    pub free_transcriber: FnFreeTranscriber,
    pub create_stream: FnCreateStream,
    pub free_stream: FnFreeStream,
    pub start_stream: FnStartStream,
    pub stop_stream: FnStopStream,
    pub add_audio: FnAddAudio,
    pub transcribe_stream: FnTranscribeStream,
}

unsafe impl Send for MoonshineLib {}
unsafe impl Sync for MoonshineLib {}

static MOONSHINE_LIB: OnceLock<Result<MoonshineLib, String>> = OnceLock::new();

fn moonshine_dll_path() -> std::path::PathBuf {
    let private_bin = crate::unpack_dlls::private_bin_dir();
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let candidates = [
        private_bin.join("moonshine_wrapper.dll"),
        exe_dir.join("moonshine_wrapper.dll"),
        std::path::PathBuf::from("native/moonshine_wrapper/moonshine_wrapper.dll"),
        std::path::PathBuf::from("dist/moonshine-runtime-windows-x64/moonshine_wrapper.dll"),
    ];

    for c in &candidates {
        if c.exists() {
            return c.clone();
        }
    }
    candidates[0].clone()
}

pub fn load() -> Result<&'static MoonshineLib> {
    MOONSHINE_LIB
        .get_or_init(|| {
            let dll_path = moonshine_dll_path();
            let dll_dir = dll_path.parent().unwrap_or_else(|| std::path::Path::new("."));

            crate::log_info!("[Moonshine] Loading from {:?}", dll_path);

            // Load ORT first (moonshine depends on it)
            let ort_path = dll_dir.join("onnxruntime.dll");
            let ort_lib = if ort_path.exists() {
                // Set DLL directory so moonshine_wrapper.dll can find onnxruntime.dll
                unsafe {
                    use windows::Win32::System::LibraryLoader::SetDllDirectoryW;
                    let dir_wide: Vec<u16> = dll_dir.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect();
                    let _ = SetDllDirectoryW(windows::core::PCWSTR(dir_wide.as_ptr()));
                }
                unsafe { Library::new(&ort_path).ok() }
            } else {
                None
            };

            if !dll_path.exists() {
                return Err(format!(
                    "Moonshine runtime not found at {:?}. It will be downloaded automatically on first use.",
                    dll_path
                ));
            }

            let lib = unsafe {
                Library::new(&dll_path)
                    .map_err(|e| format!("Failed to load {}: {e}", dll_path.display()))?
            };

            unsafe {
                let load_transcriber = *lib.get::<FnLoadTranscriber>(b"moonshine_load_transcriber_from_files").map_err(|e| e.to_string())?;
                let free_transcriber = *lib.get::<FnFreeTranscriber>(b"moonshine_free_transcriber").map_err(|e| e.to_string())?;
                let create_stream = *lib.get::<FnCreateStream>(b"moonshine_create_stream").map_err(|e| e.to_string())?;
                let free_stream = *lib.get::<FnFreeStream>(b"moonshine_free_stream").map_err(|e| e.to_string())?;
                let start_stream = *lib.get::<FnStartStream>(b"moonshine_start_stream").map_err(|e| e.to_string())?;
                let stop_stream = *lib.get::<FnStopStream>(b"moonshine_stop_stream").map_err(|e| e.to_string())?;
                let add_audio = *lib.get::<FnAddAudio>(b"moonshine_transcribe_add_audio_to_stream").map_err(|e| e.to_string())?;
                let transcribe_stream = *lib.get::<FnTranscribeStream>(b"moonshine_transcribe_stream").map_err(|e| e.to_string())?;

                // Transmute function pointers from symbols (which borrow lib) to
                // raw fn pointers. Safe because lib is kept alive in the struct.
                Ok(MoonshineLib {
                    _lib: lib,
                    _ort_lib: ort_lib,
                    load_transcriber: std::mem::transmute(load_transcriber),
                    free_transcriber: std::mem::transmute(free_transcriber),
                    create_stream: std::mem::transmute(create_stream),
                    free_stream: std::mem::transmute(free_stream),
                    start_stream: std::mem::transmute(start_stream),
                    stop_stream: std::mem::transmute(stop_stream),
                    add_audio: std::mem::transmute(add_audio),
                    transcribe_stream: std::mem::transmute(transcribe_stream),
                })
            }
        })
        .as_ref()
        .map_err(|e| anyhow!("{e}"))
}
