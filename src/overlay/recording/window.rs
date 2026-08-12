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
        AUDIO_STOP_SIGNAL.store(true, Ordering::SeqCst);
        AUDIO_ABORT_SIGNAL.store(true, Ordering::SeqCst);
        AUDIO_PAUSE_SIGNAL.store(false, Ordering::SeqCst);
        AUDIO_WARMUP_COMPLETE.store(false, Ordering::SeqCst);
        CURRENT_RMS.store(0, Ordering::Relaxed);
    }
}

pub fn start_audio_thread(hwnd: HWND, preset_idx: usize) {
    let (preset, last_active_window) = {
        let app = APP.lock().unwrap();
        (
            app.config.presets[preset_idx].clone(),
            app.last_active_window,
        )
    };
    let hwnd_val = hwnd.0 as usize;
    let (use_gemini_live_stream, use_parakeet_stream) = {
        let mut gemini = false;
        let mut parakeet = false;
        for block in &preset.blocks {
            if block.block_type == "audio"
                && let Some(config) = crate::model_config::get_model_by_id(&block.model)
            {
                gemini |= config.provider == "gemini-live";
                parakeet |= config.provider == "parakeet";
            }
        }
        (gemini, parakeet)
    };

    std::thread::spawn(move || {
        let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
        let target = last_active_window.map(|window| window.0);
        if use_gemini_live_stream {
            crate::api::record_and_stream_gemini_live(
                preset,
                AUDIO_STOP_SIGNAL.clone(),
                AUDIO_PAUSE_SIGNAL.clone(),
                AUDIO_ABORT_SIGNAL.clone(),
                hwnd,
                target,
            );
        } else if use_parakeet_stream {
            crate::api::audio::record_and_stream_parakeet(
                preset,
                AUDIO_STOP_SIGNAL.clone(),
                AUDIO_PAUSE_SIGNAL.clone(),
                AUDIO_ABORT_SIGNAL.clone(),
                hwnd,
                target,
            );
        } else {
            crate::api::record_audio_and_transcribe(
                preset,
                AUDIO_STOP_SIGNAL.clone(),
                AUDIO_PAUSE_SIGNAL.clone(),
                AUDIO_ABORT_SIGNAL.clone(),
                hwnd,
            );
        }
    });
}
