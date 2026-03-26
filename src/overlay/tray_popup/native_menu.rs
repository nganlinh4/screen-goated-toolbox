// Fallback native Win32 context menu when WebView initialization fails

use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

use crate::APP;

use super::html::{get_popup_labels, get_restore_options};

/// Fallback native context menu when WebView fails
pub(super) unsafe fn show_native_context_menu() {
    unsafe {
        use crate::config::ThemeMode;
        use windows::core::{HSTRING, PCWSTR};

        let mut ui_language = String::from("en");
        let (
            settings_text,
            bubble_text,
            stop_tts_text,
            restore_overlay_text,
            quit_text,
            bubble_checked,
            _is_dark,
        ) = if let Ok(app) = APP.lock() {
            ui_language = app.config.ui_language.clone();
            let (settings, bubble, stop_tts, restore_overlay, quit) =
                get_popup_labels(&app.config.ui_language);
            let checked = app.config.show_favorite_bubble;

            let is_dark = match app.config.theme_mode {
                ThemeMode::Dark => true,
                ThemeMode::Light => false,
                ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
            };

            (
                settings,
                bubble,
                stop_tts,
                restore_overlay,
                quit,
                checked,
                is_dark,
            )
        } else {
            (
                "Settings",
                "Favorite Bubble",
                "Stop All TTS",
                "Restore Last Closed Overlay",
                "Quit",
                false,
                true,
            )
        };

        let has_tts_pending = crate::api::tts::TTS_MANAGER.has_pending_audio();
        let restore_options = get_restore_options(&ui_language);

        // Create a dummy window to handle menu messages
        let instance = GetModuleHandleW(None).unwrap_or_default();
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("STATIC"),
            w!("SGTNativeMenu"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        if hwnd.is_invalid() {
            return;
        }

        let _ = SetForegroundWindow(hwnd);

        let hmenu = CreatePopupMenu().unwrap_or_default();

        fn add_item(hmenu: HMENU, id: usize, text: &str, checked: bool, disabled: bool) {
            let mut flags = MF_STRING;
            if checked {
                flags |= MF_CHECKED;
            }
            if disabled {
                flags |= MF_DISABLED | MF_GRAYED;
            }

            let h_text = HSTRING::from(text);
            unsafe {
                let _ = AppendMenuW(hmenu, flags, id, PCWSTR(h_text.as_ptr()));
            }
        }

        add_item(hmenu, 1, settings_text, false, false);
        add_item(hmenu, 2, bubble_text, bubble_checked, false);
        add_item(hmenu, 3, stop_tts_text, false, !has_tts_pending);
        if restore_options.is_empty() {
            add_item(hmenu, 4, restore_overlay_text, false, true);
        } else {
            let restore_menu = CreatePopupMenu().unwrap_or_default();
            for option in &restore_options {
                add_item(
                    restore_menu,
                    40 + option.batch_count,
                    &option.label,
                    false,
                    false,
                );
            }

            let h_text = HSTRING::from(restore_overlay_text);
            let _ = AppendMenuW(
                hmenu,
                MF_POPUP,
                restore_menu.0 as usize,
                PCWSTR(h_text.as_ptr()),
            );
        }
        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
        add_item(hmenu, 5, quit_text, false, false);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);

        let cmd_id = TrackPopupMenu(
            hmenu,
            TPM_RETURNCMD | TPM_NONOTIFY | TPM_BOTTOMALIGN | TPM_LEFTALIGN,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );

        let _ = DestroyMenu(hmenu);
        let _ = DestroyWindow(hwnd);

        match cmd_id.0 as u32 {
            1 => {
                // Settings
                crate::gui::signal_restore_window();
            }
            2 => {
                // Toggle Bubble
                if let Ok(mut app) = APP.lock() {
                    app.config.show_favorite_bubble = !app.config.show_favorite_bubble;
                    let enabled = app.config.show_favorite_bubble;
                    crate::config::save_config(&app.config);

                    if enabled {
                        crate::overlay::favorite_bubble::show_favorite_bubble();
                        std::thread::spawn(|| {
                            std::thread::sleep(std::time::Duration::from_millis(150));
                            crate::overlay::favorite_bubble::trigger_blink_animation();
                        });
                    } else {
                        crate::overlay::favorite_bubble::hide_favorite_bubble();
                    }
                }
            }
            3 => {
                // Stop TTS
                crate::api::tts::TTS_MANAGER.stop();
            }
            41..=45 => {
                let batch_count = cmd_id.0 as usize - 40;
                std::thread::spawn(move || {
                    let _ = crate::overlay::result::restore_recent(batch_count);
                });
            }
            5 => {
                // Quit
                std::process::exit(0);
            }
            _ => {}
        }
    }
}
