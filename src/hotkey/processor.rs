// --- HOTKEY PROCESSOR ---
// Window procedure for handling hotkey messages.

use crate::overlay;
use crate::screen_capture::capture_screen_fast;
use crate::win_types::SendHwnd;
use crate::APP;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const WM_APP_PROCESS_PENDING_FILE: u32 = WM_USER + 102;

/// Window procedure for handling hotkey and inter-process messages.
pub unsafe extern "system" fn hotkey_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_APP_PROCESS_PENDING_FILE => {
            handle_pending_file();
            LRESULT(0)
        }
        WM_HOTKEY => {
            handle_hotkey(wparam.0 as i32);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Handle pending file from inter-process communication.
fn handle_pending_file() {
    let temp_file = std::env::temp_dir().join("sgt_pending_file.txt");
    if temp_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&temp_file) {
            let path = std::path::PathBuf::from(content.trim());
            if path.exists() {
                crate::log_info!("HOTKEY LISTENER: Processing pending file: {:?}", path);
                let path_clone = path.clone();
                std::thread::spawn(move || {
                    crate::gui::app::input_handler::process_file_path(&path_clone);
                });
            }
        }
        let _ = std::fs::remove_file(temp_file);
    }
}

/// Handle a hotkey message.
fn handle_hotkey(id: i32) {
    // Screen record hotkey
    if id >= 9900 && id <= 9999 {
        overlay::screen_record::toggle_recording();
        return;
    }

    if id <= 0 {
        return;
    }

    // Debounce logic
    static mut LAST_HOTKEY_TIMESTAMP: Option<std::time::Instant> = None;
    let now = std::time::Instant::now();
    let is_repeat = unsafe {
        if let Some(t) = LAST_HOTKEY_TIMESTAMP {
            if now.duration_since(t).as_millis() < 150 {
                true
            } else {
                LAST_HOTKEY_TIMESTAMP = Some(now);
                false
            }
        } else {
            LAST_HOTKEY_TIMESTAMP = Some(now);
            false
        }
    };

    if !is_repeat {
        overlay::continuous_mode::reset_heartbeat();
    }
    overlay::continuous_mode::update_last_trigger_time();

    if is_repeat {
        return;
    }

    // Check continuous mode states
    let mut just_activated_continuous = false;
    let preset_idx_early = ((id - 1) / 1000) as usize;

    // Check image continuous mode
    if overlay::image_continuous_mode::is_active() {
        let active_idx = overlay::image_continuous_mode::get_preset_idx();
        let trigger_id = overlay::image_continuous_mode::get_trigger_id();

        crate::log_info!(
            "[Hotkey] ImageContinuous Active: active_idx={}, trigger_id={}, current_id={}, early_idx={}",
            active_idx, trigger_id, id, preset_idx_early
        );

        if preset_idx_early == active_idx {
            if id == trigger_id && overlay::image_continuous_mode::can_exit_now() {
                crate::log_info!("[Hotkey] Toggling ImageContinuous OFF (id matches)");
                overlay::image_continuous_mode::exit();
                return;
            }
            return;
        }
        crate::log_info!(
            "[Hotkey] ImageContinuous active but diff preset triggered. Allowing fallthrough."
        );
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
        _ => handle_image_preset(preset_idx, id),
    }
}

/// Get preset context information.
fn get_preset_context(id: i32, preset_idx: usize) -> (String, String, bool, String) {
    if let Ok(app) = APP.lock() {
        if preset_idx < app.config.presets.len() {
            let p = &app.config.presets[preset_idx];
            let p_type = p.preset_type.clone();
            let t_mode = p.text_input_mode.clone();
            let stopping = p_type == "audio" && overlay::is_recording_overlay_active();

            let hk_idx = ((id - 1) % 1000) as usize;
            let hk_name = if hk_idx < p.hotkeys.len() {
                let hk = &p.hotkeys[hk_idx];
                if overlay::continuous_mode::supports_continuous_mode(&p_type) {
                    crate::log_info!(
                        "[Hotkey] Setting current hotkey for hold detection: mods={}, code={}, name='{}'",
                        hk.modifiers, hk.code, hk.name
                    );
                    overlay::continuous_mode::set_current_hotkey(hk.modifiers, hk.code);
                    overlay::continuous_mode::set_latest_hotkey_name(hk.name.clone());
                }
                hk.name.clone()
            } else {
                String::new()
            };

            return (p_type, t_mode, stopping, hk_name);
        }
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
            std::thread::spawn(move || {
                overlay::show_realtime_overlay(preset_idx);
            });
        }
    } else {
        if overlay::is_recording_overlay_active() {
            overlay::stop_recording_and_submit();
        } else {
            std::thread::spawn(move || {
                overlay::show_recording_overlay(preset_idx);
            });
        }
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
    let ts_active = overlay::text_selection::is_active();
    let ts_warming = overlay::text_selection::is_warming_up();
    let ts_held = overlay::text_selection::is_hotkey_held();
    let cm_active = overlay::continuous_mode::is_active();

    crate::log_info!(
        "[TextHotkey] Entering text handling: ts_active={}, ts_warming={}, ts_held={}, cm_active={}, just_activated={}",
        ts_active, ts_warming, ts_held, cm_active, just_activated_continuous
    );

    let is_visible = overlay::text_selection::is_active();
    crate::log_info!("[TextHotkey] State check: visible={}", is_visible);

    if is_visible {
        if !overlay::text_selection::is_hotkey_held() {
            if cm_active {
                crate::log_info!("[TextHotkey] Continuous mode active - trying instant process");
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
                return;
            } else {
                // Don't toggle off while the preset wheel is showing (e.g. master preset
                // triggered processing which opened the wheel — key repeat gaps would
                // otherwise cancel it)
                if overlay::preset_wheel::is_wheel_active() {
                    return;
                }
                crate::log_info!("[TextHotkey] Toggle OFF - cancelling text selection");
                overlay::text_selection::cancel_selection();
                return;
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

                if is_master {
                    crate::log_info!(
                        "[TextHotkey] Held - but master preset, skipping continuous mode"
                    );
                } else {
                    crate::log_info!("[TextHotkey] Held - activating text continuous mode");
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
            } else {
                crate::log_info!(
                    "[TextHotkey] Held - updating heartbeat only (continuous already active)"
                );
            }
            overlay::continuous_mode::update_last_trigger_time();
            return;
        }
    } else if overlay::text_selection::is_warming_up() {
        crate::log_info!("[TextHotkey] Warming up - waiting");
        overlay::continuous_mode::update_last_trigger_time();
        return;
    } else if overlay::continuous_mode::is_active()
        && !just_activated_continuous
        && !overlay::image_continuous_mode::is_active()
    {
        overlay::continuous_mode::update_last_trigger_time();
        return;
    } else {
        let is_proc = overlay::text_selection::is_processing();
        crate::log_info!("[TextHotkey] is_processing={}", is_proc);
        if is_proc {
            return;
        }

        std::thread::spawn(move || {
            crate::log_info!("[TextHotkey] Spawned thread starting");
            overlay::show_text_selection_tag(preset_idx);
            crate::log_info!("[TextHotkey] Badge show called");

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
    } else {
        if let Ok(app) = APP.lock() {
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
}

/// Handle image preset hotkey.
fn handle_image_preset(preset_idx: usize, id: i32) {
    if overlay::is_busy() || overlay::is_selection_overlay_active() {
        overlay::continuous_mode::update_last_trigger_time();
        return;
    }

    overlay::set_is_busy(true);

    let app_clone = APP.clone();
    let p_idx = preset_idx;
    std::thread::spawn(move || {
        loop {
            match capture_screen_fast() {
                Ok(capture) => {
                    if let Ok(mut app) = app_clone.lock() {
                        app.screenshot_handle = Some(capture);
                    } else {
                        break;
                    }

                    overlay::show_selection_overlay(p_idx, id);
                }
                Err(e) => {
                    eprintln!("Capture Error: {}", e);
                    break;
                }
            }

            if !overlay::continuous_mode::is_active()
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
