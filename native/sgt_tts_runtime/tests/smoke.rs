//! End-to-end smoke test for the cdylib. Loads the built DLL, calls every
//! exported symbol once, and reports what happened.
//!
//! Run with: `cargo test --release --manifest-path native/sgt_tts_runtime/Cargo.toml -- --nocapture`

use std::ffi::c_void;
use std::os::raw::{c_char, c_float, c_int};
use std::path::PathBuf;

type FnVersion = unsafe extern "C" fn() -> u32;
type FnCreate = unsafe extern "C" fn(*const c_char, usize, *mut *mut c_void) -> c_int;
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

fn dll_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("SGT_TTS_RUNTIME_DLL") {
        return Some(PathBuf::from(path));
    }

    // The user's installed copy under %LOCALAPPDATA%; we test what the
    // app would actually use.
    std::env::var_os("LOCALAPPDATA").map(|local_app_data| {
        PathBuf::from(local_app_data)
            .join("screen-goated-toolbox")
            .join("bin")
            .join("x64")
            .join("sgt_voxtral_runtime.dll")
    })
}

unsafe fn read_err(rt: *mut c_void, last_err: FnLastError) -> String {
    let mut msg: *const c_char = std::ptr::null();
    let mut len: usize = 0;
    unsafe {
        let _ = last_err(rt, &mut msg, &mut len);
        if msg.is_null() || len == 0 {
            return "<empty>".to_string();
        }
        let slice = std::slice::from_raw_parts(msg as *const u8, len);
        String::from_utf8_lossy(slice).into_owned()
    }
}

#[test]
fn smoke_through_real_dll() {
    let Some(path) = dll_path() else {
        eprintln!("skipping real DLL smoke test: LOCALAPPDATA/SGT_TTS_RUNTIME_DLL is not set");
        return;
    };
    if !path.exists() {
        eprintln!(
            "skipping real DLL smoke test: DLL is not installed at {}",
            path.display()
        );
        return;
    }

    let lib = unsafe { libloading::Library::new(&path) }.expect("failed to load DLL");
    let version: FnVersion = *unsafe { lib.get(b"sgt_tts_runtime_version") }
        .expect("sgt_tts_runtime_version not exported");
    let create: FnCreate =
        *unsafe { lib.get(b"sgt_tts_create") }.expect("sgt_tts_create not exported");
    let destroy: FnDestroy =
        *unsafe { lib.get(b"sgt_tts_destroy") }.expect("sgt_tts_destroy not exported");
    let synth: FnSynth =
        *unsafe { lib.get(b"sgt_tts_synthesize") }.expect("sgt_tts_synthesize not exported");
    let free_audio: FnFreeAudio =
        *unsafe { lib.get(b"sgt_tts_free_audio") }.expect("sgt_tts_free_audio not exported");
    let last_err: FnLastError =
        *unsafe { lib.get(b"sgt_tts_last_error") }.expect("sgt_tts_last_error not exported");

    // 1. ABI version
    let abi = unsafe { version() };
    println!("ABI version: {abi}");
    assert_eq!(abi, 1, "ABI version mismatch");

    // 2. Create runtime
    let model_dir = b"C:\\fake\\voxtral_tts_2603";
    let mut rt: *mut c_void = std::ptr::null_mut();
    let rc = unsafe {
        create(
            model_dir.as_ptr() as *const c_char,
            model_dir.len(),
            &mut rt,
        )
    };
    println!("sgt_tts_create rc={rc} handle={:p}", rt);
    assert_eq!(rc, 0, "create failed");
    assert!(!rt.is_null(), "null handle");

    // 3. Try to synthesize - expect a "Python package not installed" error.
    // Point the DLL at our local Python script.
    unsafe {
        std::env::set_var(
            "SGT_TTS_PYTHON_SCRIPT",
            "C:\\WORK\\screen-goated-toolbox\\native\\sgt_tts_runtime_py\\synthesize.py",
        );
    }
    let text = b"hello world";
    let empty: &[u8] = b"";
    let mut pcm: *const i16 = std::ptr::null();
    let mut count: usize = 0;
    let mut sr: i32 = 0;
    let rc2 = unsafe {
        synth(
            rt,
            text.as_ptr() as *const c_char,
            text.len(),
            empty.as_ptr() as *const c_char,
            0,
            empty.as_ptr() as *const c_char,
            0,
            1.0,
            &mut pcm,
            &mut count,
            &mut sr,
        )
    };
    println!(
        "sgt_tts_synthesize rc={rc2} pcm={:p} count={count} sr={sr}",
        pcm
    );
    if rc2 == 0 {
        // unlikely without Voxtral installed, but verify we can free
        let free_rc = unsafe { free_audio(rt, pcm) };
        println!("sgt_tts_free_audio rc={free_rc}");
        assert_eq!(free_rc, 0);
    } else {
        let msg = unsafe { read_err(rt, last_err) };
        println!("last_error = {msg}");
        assert!(
            !msg.is_empty() && msg != "<empty>",
            "expected error message via last_error"
        );
    }

    // 4. Destroy
    let drc = unsafe { destroy(rt) };
    println!("sgt_tts_destroy rc={drc}");
    assert_eq!(drc, 0);
}
