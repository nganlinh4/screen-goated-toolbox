//! Runtime-loaded FFI bindings to the Moonshine Voice C API.
//!
//! Loads moonshine.lib + onnxruntime.dll dynamically at runtime via libloading
//! to avoid CRT mismatch (Moonshine libs use /MD, project uses /MT).

#![allow(non_camel_case_types, dead_code)]

use anyhow::{Result, anyhow};
use libloading::{Library, Symbol};
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

fn moonshine_sdk_dir() -> std::path::PathBuf {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    // Check next to executable first, then third_party in repo
    let candidates = [
        exe_dir.join("moonshine"),
        exe_dir.join("moonshine-voice-windows-x86_64"),
        std::path::PathBuf::from("third_party/moonshine-voice/moonshine-voice-windows-x86_64"),
    ];

    for c in &candidates {
        if c.join("lib").exists() {
            return c.clone();
        }
    }
    candidates[2].clone()
}

pub fn load() -> Result<&'static MoonshineLib> {
    MOONSHINE_LIB
        .get_or_init(|| {
            let sdk_dir = moonshine_sdk_dir();
            let lib_dir = sdk_dir.join("lib");

            crate::log_info!("[Moonshine] Loading from {:?}", lib_dir);

            // Load ORT first (moonshine depends on it)
            let ort_path = lib_dir.join("onnxruntime.dll");
            let ort_lib = if ort_path.exists() {
                unsafe { Library::new(&ort_path).ok() }
            } else {
                None
            };

            // Load moonshine as a DLL. The static .lib files have CRT mismatch,
            // but we can build a thin DLL wrapper or use the static libs via
            // a separate compilation unit. For now, we need moonshine as a DLL.
            //
            // Since the SDK only ships static .lib files (not a .dll), we need
            // to build a wrapper DLL. This is a known limitation.
            //
            // Alternative: use the Python wheel which ships moonshine as a .pyd/.dll.
            let moonshine_dll = lib_dir.join("moonshine.dll");
            if !moonshine_dll.exists() {
                return Err(format!(
                    "Moonshine DLL not found at {:?}. The SDK ships static .lib files which have CRT mismatch with this project. \
                    A moonshine.dll wrapper is needed.", moonshine_dll
                ));
            }

            let lib = unsafe {
                Library::new(&moonshine_dll)
                    .map_err(|e| format!("Failed to load moonshine.dll: {e}"))?
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
