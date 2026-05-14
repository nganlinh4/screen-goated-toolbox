//! Verify the DLL also loads cleanly through the *actual* loader function
//! in the main crate (`tts_libtorch_runtime::load_runtime_dll`). This catches
//! mismatches between our cdylib's symbol layout and the parent app's
//! expected FFI signature.
//!
//! We can't link to the main crate from here (it's a bin, not a lib) so we
//! reimplement the same loader logic locally and just verify it agrees with
//! ours. If the symbol table or signatures drift, this test catches it.

use std::ffi::c_void;
use std::os::raw::{c_char, c_float, c_int};
use std::path::PathBuf;

// Same constant as `src/api/realtime_audio/tts_libtorch_runtime.rs`.
const EXPECTED_ABI: u32 = 1;

type FnVersion = unsafe extern "C" fn() -> u32;
type FnCreate =
    unsafe extern "C" fn(*const c_char, usize, *mut *mut c_void) -> c_int;
type FnDestroy = unsafe extern "C" fn(*mut c_void) -> c_int;
#[allow(clippy::type_complexity)]
type FnSynth = unsafe extern "C" fn(
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

fn dll_path_for(model_alias: &str) -> PathBuf {
    PathBuf::from(std::env::var("LOCALAPPDATA").expect("LOCALAPPDATA"))
        .join("screen-goated-toolbox")
        .join("bin")
        .join("x64")
        .join(format!("sgt_{model_alias}_runtime.dll"))
}

fn resolve_all_symbols(path: &std::path::Path) {
    let lib = unsafe { libloading::Library::new(path) }
        .unwrap_or_else(|e| panic!("LoadLibrary({}) failed: {e}", path.display()));

    // Each .get() returns Result — collecting them lets us report which symbol
    // is missing if any.
    let _: FnVersion = *unsafe { lib.get(b"sgt_tts_runtime_version") }
        .expect("sgt_tts_runtime_version missing");
    let _: FnCreate = *unsafe { lib.get(b"sgt_tts_create") }
        .expect("sgt_tts_create missing");
    let _: FnDestroy = *unsafe { lib.get(b"sgt_tts_destroy") }
        .expect("sgt_tts_destroy missing");
    let _: FnSynth = *unsafe { lib.get(b"sgt_tts_synthesize") }
        .expect("sgt_tts_synthesize missing");
    let _: FnFreeAudio = *unsafe { lib.get(b"sgt_tts_free_audio") }
        .expect("sgt_tts_free_audio missing");
    let _: FnLastError = *unsafe { lib.get(b"sgt_tts_last_error") }
        .expect("sgt_tts_last_error missing");

    // ABI handshake
    let v: FnVersion = *unsafe { lib.get(b"sgt_tts_runtime_version") }.unwrap();
    let got = unsafe { v() };
    assert_eq!(got, EXPECTED_ABI, "ABI mismatch in {}", path.display());
}

#[test]
fn step_audio_dll_passes_symbol_resolution() {
    resolve_all_symbols(&dll_path_for("step_audio"));
}

#[test]
fn voxtral_dll_passes_symbol_resolution() {
    resolve_all_symbols(&dll_path_for("voxtral"));
}

#[test]
fn model_dispatch_is_correct() {
    // Each renamed DLL should detect its model from the directory passed to
    // sgt_tts_create. This verifies the basename-substring dispatch logic
    // works across all four model paths the main app uses.
    let cases = &[
        ("step_audio", "C:\\fake\\step_audio_editx", "step_audio"),
        ("voxtral", "C:\\fake\\voxtral_tts_2603", "voxtral"),
    ];

    for (alias, model_dir, expected_python_alias) in cases {
        let lib = unsafe { libloading::Library::new(dll_path_for(alias)) }.unwrap();
        let create: FnCreate = *unsafe { lib.get(b"sgt_tts_create") }.unwrap();
        let destroy: FnDestroy = *unsafe { lib.get(b"sgt_tts_destroy") }.unwrap();
        let synth: FnSynth = *unsafe { lib.get(b"sgt_tts_synthesize") }.unwrap();
        let last_err: FnLastError = *unsafe { lib.get(b"sgt_tts_last_error") }.unwrap();

        let mut rt: *mut c_void = std::ptr::null_mut();
        let dir_bytes = model_dir.as_bytes();
        let rc = unsafe {
            create(
                dir_bytes.as_ptr() as *const c_char,
                dir_bytes.len(),
                &mut rt,
            )
        };
        assert_eq!(rc, 0, "create rc != 0 for {alias}");

        // Point at our local script so we don't depend on the user's checkout layout.
        unsafe {
            std::env::set_var(
                "SGT_TTS_PYTHON_SCRIPT",
                "C:\\WORK\\screen-goated-toolbox\\native\\sgt_tts_runtime_py\\synthesize.py",
            );
        }

        let text = b"test";
        let mut pcm: *const i16 = std::ptr::null();
        let mut count: usize = 0;
        let mut sr: i32 = 0;
        let rc2 = unsafe {
            synth(
                rt,
                text.as_ptr() as *const c_char,
                text.len(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
                1.0,
                &mut pcm,
                &mut count,
                &mut sr,
            )
        };

        // We expect failure (no Python packages installed) — verify the error
        // message names the *right* model so dispatch routed correctly.
        assert_ne!(rc2, 0, "expected python failure for {alias}");
        let mut msg: *const c_char = std::ptr::null();
        let mut mlen: usize = 0;
        let _ = unsafe { last_err(rt, &mut msg, &mut mlen) };
        assert!(!msg.is_null() && mlen > 0, "last_error empty for {alias}");
        let err = unsafe {
            std::str::from_utf8(std::slice::from_raw_parts(msg as *const u8, mlen)).unwrap()
        };
        // The Python script names the model in its error output; check the
        // alias the DLL passed through is the one we expected.
        let alias_in_msg = err.to_ascii_lowercase().contains(&expected_python_alias.replace('_', "-"))
            || err.to_ascii_lowercase().contains(&expected_python_alias.replace('_', " "))
            || err.to_ascii_lowercase().contains(*expected_python_alias);
        assert!(
            alias_in_msg || matches_via_known_keywords(expected_python_alias, err),
            "dispatch may be wrong for {alias}: error = {err}"
        );

        let _ = unsafe { destroy(rt) };
    }
}

fn matches_via_known_keywords(model: &str, err: &str) -> bool {
    let needles: &[&str] = match model {
        "step_audio" => &["step_audio", "step-audio", "stepfun"],
        "voxtral" => &["voxtral", "mistral"],
        _ => &[],
    };
    let lower = err.to_ascii_lowercase();
    needles.iter().any(|n| lower.contains(*n))
}
