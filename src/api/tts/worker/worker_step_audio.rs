//! Step Audio EditX offline TTS worker.
//!
//! 3B-param PyTorch model from StepFun-AI. Synthesis runs through the shared
//! libtorch-shim DLL pattern; see `tts_libtorch_runtime.rs` for the canonical
//! commentary. The model weights live at `models/step_audio_editx/`; the
//! custom inference DLL is fetched from
//! `native/step_audio_runtime/dist/sgt_step_audio_runtime.dll` on the project
//! repo's `main` branch.

use std::sync::{Arc, Mutex};

use super::super::manager::TtsManager;
use super::super::types::AudioEvent;
use super::open_weights::{fail_request, stream_pcm_samples};
use crate::api::realtime_audio::step_audio_assets::{
    download_step_audio_model, get_step_audio_model_dir, is_step_audio_model_downloaded,
};
use crate::api::realtime_audio::tts_libtorch_runtime::{
    SgtTtsLib, TtsRuntimeHandle, create_runtime_handle, load_runtime_dll,
};
use crate::api::realtime_audio::tts_libtorch_runtime_assets::{
    STEP_AUDIO_RUNTIME, ensure_tts_runtime, tts_runtime_dll_path,
};

const PROVIDER: &str = "StepEditX";

lazy_static::lazy_static! {
    static ref STEP_RUNTIME: Mutex<Option<TtsRuntimeHandle>> = Mutex::new(None);
    static ref STEP_LIB: Mutex<Option<&'static SgtTtsLib>> = Mutex::new(None);
}
pub(super) fn handle_step_audio_tts(
    manager: Arc<TtsManager>,
    request: super::super::types::QueuedRequest,
    tx: std::sync::mpsc::Sender<AudioEvent>,
) {
    let hwnd = request.req.hwnd;

    if !is_step_audio_model_downloaded() {
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        std::thread::spawn(move || {
            let _ = download_step_audio_model(stop, false);
        });
        fail_request(
            PROVIDER,
            hwnd,
            &tx,
            "Step Audio EditX weights are downloading. Try again once the install completes.",
        );
        return;
    }

    if let Err(e) = ensure_tts_runtime(
        STEP_AUDIO_RUNTIME,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        false,
    ) {
        fail_request(
            PROVIDER,
            hwnd,
            &tx,
            format!(
                "Step Audio runtime DLL not available: {e}. Build it per native/step_audio_runtime/README.md and commit the binary."
            ),
        );
        return;
    }

    let mut lib_guard = match STEP_LIB.lock() {
        Ok(g) => g,
        Err(e) => {
            fail_request(PROVIDER, hwnd, &tx, format!("lib lock poisoned: {e:?}"));
            return;
        }
    };
    if lib_guard.is_none() {
        match load_runtime_dll(&tts_runtime_dll_path(STEP_AUDIO_RUNTIME)) {
            Ok(lib) => *lib_guard = Some(lib),
            Err(e) => {
                fail_request(PROVIDER, hwnd, &tx, format!("load DLL: {e}"));
                return;
            }
        }
    }
    let lib = lib_guard.unwrap();
    drop(lib_guard);

    let mut handle_guard = match STEP_RUNTIME.lock() {
        Ok(g) => g,
        Err(e) => {
            fail_request(PROVIDER, hwnd, &tx, format!("handle lock poisoned: {e:?}"));
            return;
        }
    };
    if handle_guard.is_none() {
        match create_runtime_handle(lib, &get_step_audio_model_dir()) {
            Ok(h) => *handle_guard = Some(h),
            Err(e) => {
                fail_request(PROVIDER, hwnd, &tx, format!("create handle: {e}"));
                return;
            }
        }
    }
    let handle = handle_guard.as_ref().unwrap();

    match handle.synthesize(&request.req.text, "", "", 1.0) {
        Ok((samples, sr)) => stream_pcm_samples(&manager, &request, &tx, samples, sr),
        Err(e) => fail_request(PROVIDER, hwnd, &tx, format!("synthesize: {e}")),
    }
}
