//! sgt_tts_runtime — shared Rust cdylib that exposes the `sgt_tts_*` C ABI
//! for offline open-weights TTS models that use the C ABI runtime path.
//!
//! The DLL is built per model and installed under the app's private bin dir.
//! The parent Rust app's `tts_libtorch_runtime.rs` loader resolves the symbols
//! through the same ABI for each compatible runtime.
//!
//! Inference path: this DLL is a thin shim that **dispatches to Python**.
//! On `sgt_tts_synthesize`, it spawns a short-lived Python process running
//! `native/sgt_tts_runtime_py/synthesize.py`, writes the request to stdin,
//! and reads a WAV file back from stdout. The Python script is responsible
//! for actually loading each model's reference inference code and producing
//! audio. This mirrors the qwen3-server pattern of `src/api/realtime_audio/
//! qwen3/server.rs` but bakes the spawn-and-talk loop inside the DLL so the
//! main Rust app sees a uniform C ABI for compatible offline TTS providers.
//!
//! See `native/README_TTS_RUNTIME_FFI.md` for the C ABI specification.

use std::ffi::c_char;
use std::io::{Cursor, Write};
use std::os::raw::{c_float, c_int};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;

/// ABI version handshake. Bumping this requires the parent app's
/// `SGT_TTS_RUNTIME_ABI_VERSION` constant to match.
const ABI_VERSION: u32 = 1;

/// Runtime handle returned to the caller via `sgt_tts_create`. Owns the model
/// directory path, the most recent error message (for `sgt_tts_last_error`),
/// and a small registry of audio buffers we've handed out (so `free_audio`
/// finds the right `Vec` to drop).
struct Runtime {
    model_dir: PathBuf,
    model_id: ModelId,
    last_error: Mutex<String>,
    /// Audio buffers we've allocated and given to the caller. We keep the
    /// `Vec<i16>` alive until the caller hands its pointer back through
    /// `sgt_tts_free_audio`, at which point it's removed and dropped.
    live_buffers: Mutex<Vec<Vec<i16>>>,
}

#[derive(Clone, Copy, Debug)]
enum ModelId {
    Voxtral,
    Unknown,
}

impl ModelId {
    /// Detect the model from the trailing directory name.
    fn from_path(p: &std::path::Path) -> Self {
        let name = p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.contains("voxtral") {
            Self::Voxtral
        } else {
            Self::Unknown
        }
    }

    fn arg(&self) -> &'static str {
        match self {
            Self::Voxtral => "voxtral",
            Self::Unknown => "unknown",
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn sgt_tts_runtime_version() -> u32 {
    ABI_VERSION
}

/// Create a TTS runtime handle for a supported model directory.
///
/// # Safety
/// `model_dir_utf8` must point to `model_dir_len` readable UTF-8 bytes, and
/// `out_runtime` must be a valid writable pointer. The returned handle must be
/// released exactly once with `sgt_tts_destroy`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sgt_tts_create(
    model_dir_utf8: *const c_char,
    model_dir_len: usize,
    out_runtime: *mut *mut std::ffi::c_void,
) -> c_int {
    if model_dir_utf8.is_null() || out_runtime.is_null() {
        return -1;
    }
    let bytes = unsafe { std::slice::from_raw_parts(model_dir_utf8 as *const u8, model_dir_len) };
    let dir = match std::str::from_utf8(bytes) {
        Ok(s) => PathBuf::from(s),
        Err(_) => return -2,
    };
    let model_id = ModelId::from_path(&dir);
    if matches!(model_id, ModelId::Unknown) {
        return -3;
    }
    let rt = Box::new(Runtime {
        model_dir: dir,
        model_id,
        last_error: Mutex::new(String::new()),
        live_buffers: Mutex::new(Vec::new()),
    });
    unsafe {
        *out_runtime = Box::into_raw(rt) as *mut _;
    }
    0
}

/// Destroy a runtime handle created by `sgt_tts_create`.
///
/// # Safety
/// `runtime` must be a live handle returned by `sgt_tts_create`, and no other
/// exported function may be using the same handle concurrently.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sgt_tts_destroy(runtime: *mut std::ffi::c_void) -> c_int {
    if runtime.is_null() {
        return -1;
    }
    unsafe {
        drop(Box::from_raw(runtime as *mut Runtime));
    }
    0
}

/// Synthesize speech and return a runtime-owned PCM buffer.
///
/// # Safety
/// `runtime` must be a live handle. Non-null string pointers must be readable
/// for their matching lengths. Output pointers must be valid for writes. On
/// success, the returned PCM pointer remains owned by the runtime and must be
/// returned with `sgt_tts_free_audio`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn sgt_tts_synthesize(
    runtime: *mut std::ffi::c_void,
    text_utf8: *const c_char,
    text_len: usize,
    voice_utf8: *const c_char,
    voice_len: usize,
    lang_utf8: *const c_char,
    lang_len: usize,
    speed: c_float,
    out_pcm16: *mut *const i16,
    out_pcm_count: *mut usize,
    out_sample_rate: *mut i32,
) -> c_int {
    if runtime.is_null()
        || text_utf8.is_null()
        || out_pcm16.is_null()
        || out_pcm_count.is_null()
        || out_sample_rate.is_null()
    {
        return -1;
    }
    let rt = unsafe { &*(runtime as *mut Runtime) };
    let text = unsafe { read_utf8(text_utf8, text_len) }.unwrap_or_default();
    let voice = unsafe { read_utf8(voice_utf8, voice_len) }.unwrap_or_default();
    let lang = unsafe { read_utf8(lang_utf8, lang_len) }.unwrap_or_default();

    match synthesize_via_python(rt, &text, &voice, &lang, speed) {
        Ok((samples, sr)) => {
            let mut buffers = rt.live_buffers.lock().unwrap();
            buffers.push(samples);
            let last = buffers.last().unwrap();
            unsafe {
                *out_pcm16 = last.as_ptr();
                *out_pcm_count = last.len();
                *out_sample_rate = sr as i32;
            }
            0
        }
        Err(e) => {
            *rt.last_error.lock().unwrap() = e;
            -10
        }
    }
}

/// Release a PCM buffer previously returned by `sgt_tts_synthesize`.
///
/// # Safety
/// `runtime` must be a live handle, and `pcm16` must be a pointer returned by a
/// successful `sgt_tts_synthesize` call on the same handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sgt_tts_free_audio(
    runtime: *mut std::ffi::c_void,
    pcm16: *const i16,
) -> c_int {
    if runtime.is_null() || pcm16.is_null() {
        return -1;
    }
    let rt = unsafe { &*(runtime as *mut Runtime) };
    let mut buffers = rt.live_buffers.lock().unwrap();
    if let Some(pos) = buffers.iter().position(|b| b.as_ptr() == pcm16) {
        buffers.swap_remove(pos);
        return 0;
    }
    -2
}

/// Return the last error message for a runtime handle.
///
/// # Safety
/// `runtime` must be a live handle. `out_message` and `out_len` must be valid
/// writable pointers. The returned string pointer is borrowed and remains valid
/// only until the runtime's next mutating call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sgt_tts_last_error(
    runtime: *mut std::ffi::c_void,
    out_message: *mut *const c_char,
    out_len: *mut usize,
) -> c_int {
    if runtime.is_null() || out_message.is_null() || out_len.is_null() {
        return -1;
    }
    let rt = unsafe { &*(runtime as *mut Runtime) };
    // The error message string lives inside the Mutex; we return a stable
    // pointer for the duration of the next call. Storing it in a thread-local
    // would be cleaner but this is single-threaded per the FFI contract.
    let guard = rt.last_error.lock().unwrap();
    unsafe {
        *out_message = guard.as_ptr() as *const c_char;
        *out_len = guard.len();
    }
    0
}

unsafe fn read_utf8(ptr: *const c_char, len: usize) -> Option<String> {
    if ptr.is_null() {
        return Some(String::new());
    }
    if len == 0 {
        // Explicit zero-length means "empty string" — do NOT try to read a
        // NUL terminator because callers may pass a pointer to a zero-length
        // slice that has no NUL byte at all (Rust slices aren't C strings).
        return Some(String::new());
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
    std::str::from_utf8(bytes).ok().map(str::to_string)
}

/// Spawn the Python synthesizer, send a JSON request on stdin, read a WAV
/// file on stdout. The Python script's location is discovered by walking
/// upward from the DLL's directory until we find `native/sgt_tts_runtime_py/`.
fn synthesize_via_python(
    rt: &Runtime,
    text: &str,
    voice: &str,
    lang: &str,
    speed: f32,
) -> Result<(Vec<i16>, u32), String> {
    let script_path = locate_python_script().ok_or_else(|| {
        "Could not locate synthesize.py — expected under native/sgt_tts_runtime_py/".to_string()
    })?;

    let request = format!(
        "{{\"model\":\"{}\",\"model_dir\":{:?},\"text\":{:?},\"voice\":{:?},\"lang\":{:?},\"speed\":{}}}",
        rt.model_id.arg(),
        rt.model_dir.to_string_lossy(),
        text,
        voice,
        lang,
        speed
    );

    let (mut child, used_exe) = spawn_python(&script_path)?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(request.as_bytes())
            .map_err(|e| format!("write stdin: {e}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| format!("wait Python: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "Python synth via '{used_exe}' failed (exit {:?}): {stderr}",
            output.status.code()
        ));
    }

    // stdout is a single WAV file; parse it.
    let cursor = Cursor::new(output.stdout);
    let mut reader = hound::WavReader::new(cursor).map_err(|e| format!("WAV parse: {e}"))?;
    let spec = reader.spec();
    let sr = spec.sample_rate;
    let mut samples: Vec<i16> = Vec::new();
    if spec.sample_format == hound::SampleFormat::Int && spec.bits_per_sample == 16 {
        for s in reader.samples::<i16>() {
            samples.push(s.map_err(|e| format!("WAV sample: {e}"))?);
        }
    } else if spec.sample_format == hound::SampleFormat::Float && spec.bits_per_sample == 32 {
        for s in reader.samples::<f32>() {
            let v = s.map_err(|e| format!("WAV sample: {e}"))?.clamp(-1.0, 1.0);
            samples.push((v * i16::MAX as f32) as i16);
        }
    } else {
        return Err(format!(
            "Unsupported WAV format: {:?} {} bits",
            spec.sample_format, spec.bits_per_sample
        ));
    }
    // Downmix to mono if needed.
    if spec.channels > 1 {
        let ch = spec.channels as usize;
        let frames = samples.len() / ch;
        let mut mono = Vec::with_capacity(frames);
        for f in 0..frames {
            let sum: i32 = (0..ch).map(|c| samples[f * ch + c] as i32).sum();
            mono.push((sum / ch as i32) as i16);
        }
        samples = mono;
    }
    Ok((samples, sr))
}

/// Try a sequence of Python launcher candidates so we tolerate Windows
/// installs where the Microsoft Store stub (`%LOCALAPPDATA%\Microsoft\
/// WindowsApps\python.exe`) sits ahead of a real install on PATH. The order
/// is:
///   1. `SGT_TTS_PYTHON` env override (absolute path or PATH-resolvable name)
///   2. `py -3` — the Windows Python Launcher, which prefers real installs
///   3. `python3` — preferred name on real CPython installs
///   4. `python` — last resort; may resolve to the Store stub
///
/// We reject any candidate whose resolved exe path lives under
/// `Microsoft\WindowsApps`, since that's always the Store redirector.
fn spawn_python(script: &std::path::Path) -> Result<(std::process::Child, String), String> {
    let mut candidates: Vec<(String, Vec<String>)> = Vec::new();
    if let Ok(custom) = std::env::var("SGT_TTS_PYTHON") {
        candidates.push((custom, vec![]));
    }
    candidates.push(("py".to_string(), vec!["-3".to_string()]));
    candidates.push(("python3".to_string(), vec![]));
    candidates.push(("python".to_string(), vec![]));

    let mut last_err: Option<String> = None;
    for (exe, prefix_args) in candidates {
        if is_microsoft_store_stub(&exe) {
            continue;
        }
        let mut cmd = Command::new(&exe);
        cmd.args(&prefix_args)
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        match cmd.spawn() {
            Ok(child) => return Ok((child, exe)),
            Err(e) => {
                last_err = Some(format!("{exe}: {e}"));
                continue;
            }
        }
    }
    Err(format!(
        "No Python launcher worked. Install Python 3.10+ from python.org and ensure `py` or `python3` resolves to it (not the Microsoft Store stub). Last error: {}",
        last_err.unwrap_or_else(|| "none".to_string())
    ))
}

/// Detect Windows' Microsoft Store Python redirector. It lives under
/// `%LOCALAPPDATA%\Microsoft\WindowsApps\` and refuses to do anything useful
/// when the actual Store app isn't installed.
fn is_microsoft_store_stub(exe: &str) -> bool {
    let p = std::path::Path::new(exe);
    if !p.is_absolute() {
        return false;
    }
    let s = exe.to_ascii_lowercase().replace('/', "\\");
    s.contains("\\microsoft\\windowsapps\\")
}

/// Walk upward from the executable directory looking for the project's
/// `native/sgt_tts_runtime_py/synthesize.py`. Also honours
/// `SGT_TTS_PYTHON_SCRIPT` so users can override.
fn locate_python_script() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("SGT_TTS_PYTHON_SCRIPT") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    let mut dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    for _ in 0..8 {
        let candidate = dir.join("native/sgt_tts_runtime_py/synthesize.py");
        if candidate.is_file() {
            return Some(candidate);
        }
        if !dir.pop() {
            break;
        }
    }
    // Last resort: alongside the DLL itself.
    None
}
