// --- HOTKEY PROCESSOR ---
// Window procedure for handling hotkey messages.

use crate::APP;
use crate::overlay;
use crate::overlay::image_capture_target::ImageCaptureTarget;
use crate::screen_capture::{capture_screen_fast, format_gui_resources, gui_resources_snapshot};
use crate::win_types::SendHwnd;
use std::sync::OnceLock;
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::*;

fn capture_diag_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("SGT_CAPTURE_DIAG")
            .map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false)
    })
}

/// Window procedure for handling hotkey and inter-process messages.
pub unsafe extern "system" fn hotkey_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match msg {
            WM_HOTKEY => {
                handle_hotkey(wparam.0 as i32);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// Handle a hotkey message.
fn handle_hotkey(id: i32) {
    if (crate::hotkey::SCREEN_TRANSLATE_HOTKEY_ID..crate::hotkey::COMPUTER_CONTROL_HOTKEY_ID)
        .contains(&id)
    {
        handle_screen_translate_hotkey(id);
        return;
    }

    if (crate::hotkey::COMPUTER_CONTROL_HOTKEY_ID..crate::hotkey::TRANSLATION_GUMMY_HOTKEY_ID)
        .contains(&id)
    {
        if overlay::computer_control::is_active() {
            overlay::computer_control::stop_overlay();
        } else {
            overlay::computer_control::show_overlay();
        }
        return;
    }

    if (9800..9900).contains(&id) {
        overlay::translation_gummy::toggle_translation_gummy();
        return;
    }

    // Screen record hotkey
    if (9900..=9999).contains(&id) {
        overlay::screen_record::toggle_recording();
        return;
    }

    if id <= 0 {
        return;
    }

    let is_repeat = track_hotkey_heartbeat();

    if is_repeat {
        return;
    }

    // Check continuous mode states
    let mut just_activated_continuous = false;
    let preset_idx_early = ((id - 1) / 1000) as usize;

    // Check image continuous mode
    if overlay::image_continuous_mode::is_active() {
        let active_target = overlay::image_continuous_mode::get_target();
        let trigger_id = overlay::image_continuous_mode::get_trigger_id();
        let requested_target = ImageCaptureTarget::Preset(preset_idx_early);

        crate::log_info!(
            "[Hotkey] ImageContinuous Active: active_target={active_target:?}, trigger_id={}, current_id={}, requested_target={requested_target:?}",
            trigger_id,
            id,
        );

        if requested_target == active_target {
            if id == trigger_id && overlay::image_continuous_mode::can_exit_now() {
                crate::log_info!("[Hotkey] Toggling ImageContinuous OFF (id matches)");
                overlay::image_continuous_mode::exit();
                return;
            }
            return;
        }
        overlay::image_continuous_mode::exit();
    }

    // Check text continuous mode
    if overlay::continuous_mode::is_active() {
        let cm_preset = overlay::continuous_mode::get_preset_idx();
        if cm_preset == preset_idx_early {
            just_activated_continuous = true;
        } else {
            let is_new_image = {
                if let Ok(app) = crate::APP.lock() {
                    app.config
                        .presets
                        .get(preset_idx_early)
                        .map(|p| p.preset_type == "image")
                        .unwrap_or(false)
                } else {
                    false
                }
            };

            if !is_new_image {
                overlay::continuous_mode::deactivate();
                overlay::text_selection::cancel_selection();
            }
        }
    } else if overlay::continuous_mode::is_pending_start() {
        let pending_idx = overlay::continuous_mode::get_preset_idx();
        if pending_idx == preset_idx_early {
            crate::log_info!(
                "[Hotkey] Promoting PENDING continuous mode for preset {}",
                pending_idx
            );
            let hotkey = overlay::continuous_mode::get_hotkey_name();
            overlay::continuous_mode::activate(pending_idx, hotkey);
            just_activated_continuous = true;
        } else {
            crate::log_info!(
                "[Hotkey] Ignoring PENDING continuous mode for diff preset (pending={}, early={})",
                pending_idx,
                preset_idx_early
            );
        }
    }

    // Dismiss preset wheel if active
    if overlay::preset_wheel::is_wheel_active() {
        overlay::preset_wheel::dismiss_wheel();
        return;
    }

    let preset_idx = ((id - 1) / 1000) as usize;

    // Get preset context
    let (preset_type, text_mode, is_audio_stopping, hotkey_name) =
        get_preset_context(id, preset_idx);
    crate::log_info!(
        "[Action] hotkey={} preset={} type={} input_mode={}",
        hotkey_name,
        preset_idx,
        preset_type,
        text_mode
    );

    // Capture target window for paste (unless stopping audio)
    if !is_audio_stopping {
        let target_window = overlay::utils::get_target_window_for_paste();
        if let Ok(mut app) = APP.lock() {
            app.last_active_window = target_window.map(SendHwnd);
        }
    }

    // Dispatch based on preset type
    match preset_type.as_str() {
        "audio" => handle_audio_preset(preset_idx),
        "text" => handle_text_preset(
            preset_idx,
            &text_mode,
            &hotkey_name,
            just_activated_continuous,
        ),
        _ => handle_image_capture_target(
            ImageCaptureTarget::Preset(preset_idx),
            id,
            Some(preset_idx),
        ),
    }
}

fn handle_screen_translate_hotkey(id: i32) {
    let is_repeat = track_hotkey_heartbeat();
    let target = overlay::screen_translate::capture_target();
    if overlay::image_continuous_mode::is_active() {
        let active_target = overlay::image_continuous_mode::get_target();
        if active_target == target {
            if id == overlay::image_continuous_mode::get_trigger_id()
                && overlay::image_continuous_mode::can_exit_now()
            {
                overlay::image_continuous_mode::exit();
            }
            return;
        }
        overlay::image_continuous_mode::exit();
    }
    let index = (id - crate::hotkey::SCREEN_TRANSLATE_HOTKEY_ID) as usize;
    if let Some(hotkey) = APP
        .lock()
        .ok()
        .and_then(|app| app.config.screen_translate.hotkeys.get(index).cloned())
    {
        overlay::continuous_mode::set_current_hotkey(hotkey.modifiers, hotkey.code);
        overlay::continuous_mode::set_latest_hotkey_name(hotkey.name);
    }
    if !is_repeat {
        handle_image_capture_target(target, id, None);
    }
}

fn track_hotkey_heartbeat() -> bool {
    static LAST_HOTKEY_TIMESTAMP: std::sync::Mutex<Option<std::time::Instant>> =
        std::sync::Mutex::new(None);
    let now = std::time::Instant::now();
    let mut last = LAST_HOTKEY_TIMESTAMP.lock().unwrap();
    let is_repeat = last.is_some_and(|previous| now.duration_since(previous).as_millis() < 150);
    if !is_repeat {
        *last = Some(now);
        overlay::continuous_mode::reset_heartbeat();
    }
    drop(last);
    overlay::continuous_mode::update_last_trigger_time();
    is_repeat
}

/// Get preset context information.
fn get_preset_context(id: i32, preset_idx: usize) -> (String, String, bool, String) {
    if let Ok(app) = APP.lock()
        && preset_idx < app.config.presets.len()
    {
        let p = &app.config.presets[preset_idx];
        let p_type = p.preset_type.clone();
        let t_mode = p.text_input_mode.clone();
        let stopping = p_type == "audio" && overlay::is_recording_overlay_active();

        let hk_idx = ((id - 1) % 1000) as usize;
        let hk_name = if hk_idx < p.hotkeys.len() {
            let hk = &p.hotkeys[hk_idx];
            if overlay::continuous_mode::supports_continuous_mode(&p_type) {
                overlay::continuous_mode::set_current_hotkey(hk.modifiers, hk.code);
                overlay::continuous_mode::set_latest_hotkey_name(hk.name.clone());
            }
            hk.name.clone()
        } else {
            String::new()
        };

        return (p_type, t_mode, stopping, hk_name);
    }
    (
        "image".to_string(),
        "select".to_string(),
        false,
        String::new(),
    )
}

/// Handle audio preset hotkey.
fn handle_audio_preset(preset_idx: usize) {
    let is_realtime = {
        if let Ok(app) = APP.lock() {
            if preset_idx < app.config.presets.len() {
                app.config.presets[preset_idx].audio_processing_mode == "realtime"
            } else {
                false
            }
        } else {
            false
        }
    };

    if is_realtime {
        let is_minimal_active =
            overlay::realtime_egui::MINIMAL_ACTIVE.load(std::sync::atomic::Ordering::SeqCst);
        let is_webview_active = overlay::is_realtime_overlay_active();

        if is_webview_active {
            overlay::stop_realtime_overlay();
        } else if !is_minimal_active {
            if overlay::realtime_egui::recently_stopped_minimal(preset_idx) {
                return;
            }
            std::thread::spawn(move || {
                overlay::show_realtime_overlay(preset_idx);
            });
        }
    } else if overlay::is_recording_overlay_active() {
        overlay::stop_recording_and_submit();
    } else {
        std::thread::spawn(move || {
            overlay::show_recording_overlay(preset_idx);
        });
    }
}

/// Handle text preset hotkey.
fn handle_text_preset(
    preset_idx: usize,
    text_mode: &str,
    hotkey_name: &str,
    just_activated_continuous: bool,
) {
    if text_mode == "select" {
        handle_text_select_mode(preset_idx, hotkey_name, just_activated_continuous);
    } else {
        handle_text_type_mode(preset_idx, hotkey_name);
    }
}

/// Handle text preset in select mode.
fn handle_text_select_mode(preset_idx: usize, hotkey_name: &str, just_activated_continuous: bool) {
    let cm_active = overlay::continuous_mode::is_active();

    let is_visible = overlay::text_selection::is_active();

    if is_visible {
        if !overlay::text_selection::is_hotkey_held() {
            if cm_active {
                let is_proc = overlay::text_selection::is_processing();
                if !is_proc {
                    std::thread::spawn(move || {
                        let success = overlay::text_selection::try_instant_process(preset_idx);
                        if !success {
                            crate::log_info!(
                                "[TextHotkey] Instant process failed - no text selected"
                            );
                        }
                    });
                }
            } else {
                // Don't toggle off while the preset wheel is showing (e.g. master preset
                // triggered processing which opened the wheel — key repeat gaps would
                // otherwise cancel it)
                if overlay::preset_wheel::is_wheel_active() {
                    return;
                }
                overlay::text_selection::cancel_selection();
            }
        } else {
            if !overlay::continuous_mode::is_active() {
                // Check if this is a master preset - exclude from continuous mode
                let is_master = {
                    if let Ok(app) = APP.lock() {
                        app.config
                            .presets
                            .get(preset_idx)
                            .map(|p| p.is_master)
                            .unwrap_or(false)
                    } else {
                        false
                    }
                };

                if !is_master {
                    overlay::continuous_mode::activate(preset_idx, hotkey_name.to_string());
                    overlay::text_selection::update_badge_for_continuous_mode();

                    let preset_id = {
                        if let Ok(app) = APP.lock() {
                            app.config
                                .presets
                                .get(preset_idx)
                                .map(|p| p.id.clone())
                                .unwrap_or_default()
                        } else {
                            String::new()
                        }
                    };
                    if !preset_id.is_empty() {
                        overlay::continuous_mode::show_activation_notification(
                            &preset_id,
                            hotkey_name,
                        );
                    }
                }
            }
            overlay::continuous_mode::update_last_trigger_time();
        }
    } else if overlay::text_selection::is_warming_up()
        || (overlay::continuous_mode::is_active()
            && !just_activated_continuous
            && !overlay::image_continuous_mode::is_active())
    {
        overlay::continuous_mode::update_last_trigger_time();
    } else {
        let is_proc = overlay::text_selection::is_processing();
        if is_proc {
            return;
        }

        std::thread::spawn(move || {
            overlay::show_text_selection_tag(preset_idx);
            let success = overlay::text_selection::try_instant_process(preset_idx);

            if success && !overlay::continuous_mode::is_active() {
                overlay::text_selection::cancel_selection();
            }
        });
    }
}

/// Handle text preset in type mode.
fn handle_text_type_mode(preset_idx: usize, hotkey_name: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    if overlay::text_input::is_active() {
        overlay::text_input::cancel_input();
    } else if let Ok(app) = APP.lock() {
        let config = app.config.clone();
        let preset = config.presets[preset_idx].clone();
        let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
        let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
        let center_rect = RECT {
            left: (screen_w - 700) / 2,
            top: (screen_h - 300) / 2,
            right: (screen_w + 700) / 2,
            bottom: (screen_h + 300) / 2,
        };

        let localized_name =
            crate::gui::settings_ui::get_localized_preset_name(&preset.id, &config.ui_language);

        let hotkey_name_clone = hotkey_name.to_string();
        std::thread::spawn(move || {
            overlay::process::start_text_processing(
                String::new(),
                center_rect,
                config,
                preset,
                localized_name,
                hotkey_name_clone,
            );
        });
    }
}

/// Handle image preset hotkey.
fn handle_image_capture_target(
    target: ImageCaptureTarget,
    id: i32,
    continuous_preset_idx: Option<usize>,
) {
    if overlay::is_busy() || overlay::is_selection_overlay_active() {
        overlay::continuous_mode::update_last_trigger_time();
        return;
    }

    overlay::set_is_busy(true);

    target.prepare();

    let app_clone = APP.clone();
    std::thread::spawn(move || {
        let mut capture_attempt = 0usize;
        let diag_enabled = capture_diag_enabled();
        loop {
            capture_attempt += 1;
            let before = gui_resources_snapshot();
            let started_at = std::time::Instant::now();
            if diag_enabled {
                crate::log_info!(
                    "[CaptureDiag] attempt={} target={target:?} hotkey_id={} thread_id={} before={}",
                    capture_attempt,
                    id,
                    unsafe { GetCurrentThreadId() },
                    format_gui_resources(before)
                );
            }

            match capture_screen_fast() {
                Ok(capture) => {
                    let after = gui_resources_snapshot();
                    if diag_enabled {
                        crate::log_info!(
                            "[CaptureDiag] attempt={} result=success elapsed_ms={} bitmap={}x{} after={} delta_gdi={} delta_user={}",
                            capture_attempt,
                            started_at.elapsed().as_millis(),
                            capture.width,
                            capture.height,
                            format_gui_resources(after),
                            after.gdi_objects as i64 - before.gdi_objects as i64,
                            after.user_objects as i64 - before.user_objects as i64
                        );
                    }
                    if let Ok(mut app) = app_clone.lock() {
                        app.screenshot_handle = Some(capture);
                    } else {
                        break;
                    }

                    overlay::show_image_capture_overlay(target, id);
                }
                Err(e) => {
                    let after = gui_resources_snapshot();
                    crate::log_info!(
                        "[CaptureDiag] attempt={} result=failure elapsed_ms={} after={} delta_gdi={} delta_user={} error={}",
                        capture_attempt,
                        started_at.elapsed().as_millis(),
                        format_gui_resources(after),
                        after.gdi_objects as i64 - before.gdi_objects as i64,
                        after.user_objects as i64 - before.user_objects as i64,
                        e
                    );
                    eprintln!("Capture Error: {}", e);
                    break;
                }
            }

            if continuous_preset_idx.is_none() && !overlay::image_continuous_mode::is_active() {
                break;
            }

            if continuous_preset_idx.is_some()
                && !overlay::continuous_mode::is_active()
                && !overlay::image_continuous_mode::is_active()
            {
                break;
            }

            if overlay::image_continuous_mode::is_active() {
                crate::log_info!(
                    "[MainLoop] ImageContinuous active, breaking blocking capture loop"
                );
                break;
            }

            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        overlay::set_is_busy(false);
    });
}
