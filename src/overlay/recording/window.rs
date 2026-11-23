use super::messages::recording_wnd_proc;
use super::state::*;
use crate::APP;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

/// The recording controller remains a native message target because audio pipelines
/// already use its HWND as their cancellation/lifetime signal. It owns no visual surface.
pub fn internal_create_recording_window() {
    unsafe {
        let instance = match GetModuleHandleW(None) {
            Ok(instance) => instance,
            Err(error) => {
                crate::log_info!("[Recording] controller module lookup failed: {error}");
                RECORDING_STATE.store(0, Ordering::SeqCst);
                return;
            }
        };
        let class_name = w!("SGT_Recording_Controller");
        REGISTER_RECORDING_CLASS.call_once(|| {
            let class = WNDCLASSW {
                lpfnWndProc: Some(recording_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                ..Default::default()
            };
            RegisterClassW(&class);
        });

        let hwnd = match CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("SGT Recording Controller"),
            WS_POPUP,
            -32_000,
            -32_000,
            1,
            1,
            None,
            None,
            Some(instance.into()),
            None,
        ) {
            Ok(hwnd) => hwnd,
            Err(error) => {
                crate::log_info!("[Recording] controller creation failed: {error}");
                RECORDING_STATE.store(0, Ordering::SeqCst);
                return;
            }
        };
        RECORDING_HWND_VAL.store(hwnd.0 as isize, Ordering::SeqCst);
        RECORDING_STATE.store(1, Ordering::SeqCst);
        crate::log_info!("[Recording] native controller ready");

        let hook = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(super::messages::recording_hook_proc),
            Some(instance.into()),
            0,
        );
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        if let Ok(hook) = hook {
            let _ = UnhookWindowsHookEx(hook);
        }

        RECORDING_HWND_VAL.store(0, Ordering::SeqCst);
        RECORDING_STATE.store(0, Ordering::SeqCst);
        stop_current_recording();
        let _ = cancel_recording_chain();
        AUDIO_PAUSE_SIGNAL.store(false, Ordering::SeqCst);
        AUDIO_WARMUP_COMPLETE.store(false, Ordering::SeqCst);
        CURRENT_RMS.store(0, Ordering::Relaxed);
    }
}

/// Snapshots the preset a delayed show refers to. The index can be stale by the
/// time the warmup poll delivers it, so a miss is a normal outcome, not a panic.
fn snapshot_recording_preset(
    presets: &[crate::config::Preset],
    preset_idx: usize,
) -> Option<crate::config::Preset> {
    presets.get(preset_idx).cloned()
}

/// Starts the capture session for `preset_idx`, or returns `None` when that
/// preset no longer exists (deleted or reordered while the overlay warmed up).
pub fn start_audio_thread(hwnd: HWND, preset_idx: usize) -> Option<usize> {
    let (preset, last_active_window) = {
        let app = APP.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        (
            snapshot_recording_preset(&app.config.presets, preset_idx)?,
            app.last_active_window,
        )
    };
    let destination =
        crate::overlay::process::chain::AutoPasteDestination::capture(last_active_window);
    let hwnd_val = hwnd.0 as usize;
    let recording_chain = crate::overlay::result::ChainCancelToken::new();
    let session_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let session_abort = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    recording_chain.register_cancel_flag(&session_stop);
    recording_chain.register_cancel_flag(&session_abort);
    let session_id = register_recording_chain(&recording_chain, &session_stop);
    let audio_blocks = crate::overlay::process::chain::initial_audio_blocks(
        &preset.blocks,
        &preset.block_connections,
        preset.graph_explicit,
    );
    let use_gemini_live_stream = audio_blocks
        .first()
        .and_then(|index| preset.blocks.get(*index))
        .and_then(|block| crate::model_config::get_model_by_id(&block.model))
        .map(|model| recording_stream_route(&model.id, &model.provider))
        .filter(|_| audio_blocks.len() == 1)
        .unwrap_or_default();

    // Bind the original foreground target and reserve the delivery order before
    // the audio thread can be delayed behind a later recording.
    let auto_paste = use_gemini_live_stream.then(|| {
        std::sync::Arc::new(crate::overlay::utils::StreamingAutoPaste::for_recording(
            preset.auto_paste,
            session_abort.clone(),
            last_active_window.map(|window| window.0),
        ))
    });

    std::thread::spawn(move || {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        if use_gemini_live_stream {
            crate::api::record_and_stream_gemini_live(
                preset,
                session_stop,
                AUDIO_PAUSE_SIGNAL.clone(),
                session_abort,
                hwnd,
                auto_paste.expect("streaming paste owner"),
                recording_chain,
            );
        } else {
            crate::api::record_audio_and_transcribe(
                preset,
                session_stop,
                AUDIO_PAUSE_SIGNAL.clone(),
                session_abort,
                hwnd,
                destination,
                recording_chain,
            );
        }
    });
    Some(session_id)
}

fn recording_stream_route(model_id: &str, provider: &str) -> bool {
    // Input-only streaming cannot preserve a prompt or a translation target.
    provider == "gemini-live"
        && crate::model_config::model_is_non_llm(model_id)
        && !crate::model_config::is_gemini_live_translate_model_id(model_id)
}

#[cfg(test)]
mod tests {
    use super::{recording_stream_route, snapshot_recording_preset};

    #[test]
    fn stale_preset_index_is_a_miss_instead_of_a_panic() {
        let presets = vec![crate::config::Preset::default()];
        assert!(snapshot_recording_preset(&presets, 0).is_some());
        assert!(snapshot_recording_preset(&presets, 1).is_none());
        assert!(snapshot_recording_preset(&[], 0).is_none());
    }

    #[test]
    fn prompt_and_translation_operations_do_not_use_input_only_streaming() {
        let models: Vec<_> = crate::model_config::get_all_models()
            .iter()
            .filter(|model| {
                model.provider == "gemini-live"
                    && model.model_type == crate::model_config::ModelType::Audio
            })
            .collect();
        assert!(
            models
                .iter()
                .any(|model| !crate::model_config::model_is_non_llm(&model.id))
        );
        for model in models {
            let route = recording_stream_route(&model.id, &model.provider);
            if !crate::model_config::model_is_non_llm(&model.id)
                || crate::model_config::is_gemini_live_translate_model_id(&model.id)
            {
                assert!(!route, "{}", model.id);
            }
        }
    }

    #[test]
    fn recording_routes_match_shared_logical_audio_operations() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/preset-system/live-audio-routing.json"
        ))
        .unwrap();
        assert_eq!(fixture["version"], 1);
        for case in fixture["cases"].as_array().unwrap() {
            let id = case["model_id"].as_str().unwrap();
            let model = crate::model_config::get_model_by_id(id).unwrap();
            assert_eq!(model.model_type, crate::model_config::ModelType::Audio);
            let gemini = recording_stream_route(id, &model.provider);
            assert_eq!(
                gemini,
                case["gemini_input_stream"].as_bool().unwrap(),
                "{id}"
            );
            assert_eq!(
                crate::model_config::is_gemini_live_translate_model_id(id),
                case["translate_to_selected_language"].as_bool().unwrap(),
                "{id}"
            );
        }
        assert!(!crate::model_config::is_gemini_live_translate_model_id(
            "unknown-audio-model"
        ));
    }
}
