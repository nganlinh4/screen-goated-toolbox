//! Mistral Voxtral 4B TTS offline worker.
//!
//! 4B-param open-weights checkpoint (CC BY-NC 4.0). Synthesis runs through
//! the shared libtorch-shim DLL pattern; see `tts_libtorch_runtime.rs` for the
//! canonical commentary. Weights at `models/voxtral_tts_2603/`; runtime DLL
//! at `native/voxtral_runtime/dist/sgt_voxtral_runtime.dll`.

use std::sync::{Arc, LazyLock, Mutex};

use super::super::manager::TtsManager;
use super::super::types::AudioEvent;
use super::open_weights::{fail_request, stream_pcm_samples};
use crate::api::realtime_audio::tts_libtorch_runtime::{
    SgtTtsLib, TtsRuntimeHandle, create_runtime_handle, load_runtime_dll,
};
use crate::api::realtime_audio::tts_libtorch_runtime_assets::{
    VOXTRAL_RUNTIME, ensure_tts_runtime, tts_runtime_dll_path,
};
use crate::api::realtime_audio::voxtral_assets::{
    download_voxtral_model, get_voxtral_model_dir, is_voxtral_model_downloaded,
};

const PROVIDER: &str = "Voxtral";

static VOX_RUNTIME: LazyLock<Mutex<Option<TtsRuntimeHandle>>> = LazyLock::new(|| Mutex::new(None));
static VOX_LIB: LazyLock<Mutex<Option<&'static SgtTtsLib>>> = LazyLock::new(|| Mutex::new(None));
pub(super) fn handle_voxtral_tts(
    manager: Arc<TtsManager>,
    request: super::super::types::QueuedRequest,
    tx: std::sync::mpsc::Sender<AudioEvent>,
) {
    let hwnd = request.req.hwnd;

    if !is_voxtral_model_downloaded() {
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        std::thread::spawn(move || {
            let _ = download_voxtral_model(stop, false);
        });
        fail_request(
            PROVIDER,
            hwnd,
            &tx,
            "Voxtral 4B weights are downloading. Try again once the install completes.",
        );
        return;
    }

    if let Err(e) = ensure_tts_runtime(
        VOXTRAL_RUNTIME,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        false,
    ) {
        fail_request(
            PROVIDER,
            hwnd,
            &tx,
            format!(
                "Voxtral runtime DLL not available: {e}. Build it per native/voxtral_runtime/README.md and commit the binary."
            ),
        );
        return;
    }

    let mut lib_guard = match VOX_LIB.lock() {
        Ok(g) => g,
        Err(e) => {
            fail_request(PROVIDER, hwnd, &tx, format!("lib lock poisoned: {e:?}"));
            return;
        }
    };
    if lib_guard.is_none() {
        match load_runtime_dll(&tts_runtime_dll_path(VOXTRAL_RUNTIME)) {
            Ok(lib) => *lib_guard = Some(lib),
            Err(e) => {
                fail_request(PROVIDER, hwnd, &tx, format!("load DLL: {e}"));
                return;
            }
        }
    }
    let lib = lib_guard.unwrap();
    drop(lib_guard);

    let mut handle_guard = match VOX_RUNTIME.lock() {
        Ok(g) => g,
        Err(e) => {
            fail_request(PROVIDER, hwnd, &tx, format!("handle lock poisoned: {e:?}"));
            return;
        }
    };
    if handle_guard.is_none() {
        match create_runtime_handle(lib, &get_voxtral_model_dir()) {
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
