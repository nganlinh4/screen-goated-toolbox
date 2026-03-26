// i18n labels, restore-option data helpers, and update script for the tray popup

use crate::APP;

use super::{
    BASE_POPUP_HEIGHT, RESTORE_FLYOUT_OPTION_HEIGHT, RESTORE_FLYOUT_PREFERRED_TOP,
    RESTORE_FLYOUT_TOP_INSET, RESTORE_FLYOUT_VERTICAL_PADDING,
};

#[derive(serde::Serialize)]
pub(super) struct PopupRestoreOption {
    pub batch_count: usize,
    pub label: String,
}

fn restore_flyout_height_logical(option_count: usize) -> i32 {
    if option_count == 0 {
        0
    } else {
        RESTORE_FLYOUT_VERTICAL_PADDING + option_count as i32 * RESTORE_FLYOUT_OPTION_HEIGHT
    }
}

pub(super) fn restore_flyout_top_logical(option_count: usize) -> i32 {
    if option_count == 0 {
        return RESTORE_FLYOUT_TOP_INSET;
    }

    let flyout_height = restore_flyout_height_logical(option_count);
    let max_top = (BASE_POPUP_HEIGHT - flyout_height - RESTORE_FLYOUT_TOP_INSET)
        .max(RESTORE_FLYOUT_TOP_INSET);
    RESTORE_FLYOUT_PREFERRED_TOP.clamp(RESTORE_FLYOUT_TOP_INSET, max_top)
}

fn format_restore_option_label(ui_language: &str, overlay_count: usize) -> String {
    match ui_language {
        "vi" => format!("Kh\u{00f4}i ph\u{1ee5}c {overlay_count} overlay v\u{1eeb}a \u{0111}\u{00f3}ng"),
        "ko" => format!("\u{bc29}\u{ae08} \u{b2eb}\u{c740} \u{c624}\u{bc84}\u{b808}\u{c774} {overlay_count}\u{ac1c} \u{bcd5}\u{c6d0}"),
        _ => {
            let noun = if overlay_count == 1 {
                "overlay"
            } else {
                "overlays"
            };
            format!("Restore {overlay_count} recently closed {noun}")
        }
    }
}

pub(super) fn get_restore_options(ui_language: &str) -> Vec<PopupRestoreOption> {
    crate::overlay::result::recent_restore_option_counts()
        .into_iter()
        .take(5)
        .enumerate()
        .map(|(index, overlay_count)| PopupRestoreOption {
            batch_count: index + 1,
            label: format_restore_option_label(ui_language, overlay_count),
        })
        .collect()
}

pub(super) fn render_restore_options_html(options: &[PopupRestoreOption]) -> String {
    options
        .iter()
        .map(|option| {
            format!(
                r#"<div class="restore-option" onclick="action('restore_recent:{batch_count}')"><div class="restore-option-label">{label}</div></div>"#,
                batch_count = option.batch_count,
                label = option.label,
            )
        })
        .collect::<Vec<_>>()
        .join("")
}

pub(super) fn get_popup_labels(
    ui_language: &str,
) -> (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
) {
    match ui_language {
        "vi" => (
            "C\u{00e0}i \u{0111}\u{1eb7}t",
            "Hi\u{1ec7}n bong b\u{00f3}ng",
            "D\u{1eeb}ng \u{0111}\u{1ecd}c",
            "Kh\u{00f4}i ph\u{1ee5}c overlay v\u{1eeb}a \u{0111}\u{00f3}ng",
            "Tho\u{00e1}t",
        ),
        "ko" => (
            "\u{c124}\u{c815}",
            "\u{c990}\u{aca8}\u{cc3e}\u{ae30} \u{bc84}\u{be14}",
            "\u{c7ac}\u{c0dd} \u{c911}\u{c778} \u{baa8}\u{b4e0} \u{c74c}\u{c131} \u{c911}\u{c9c0}",
            "\u{bc29}\u{ae08} \u{b2eb}\u{c740} \u{c624}\u{bc84}\u{b808}\u{c774} \u{bcd5}\u{c6d0}",
            "\u{c885}\u{b8cc}",
        ),
        _ => (
            "Settings",
            "Favorite Bubble",
            "Stop All Playing TTS",
            "Restore Last Closed Overlay",
            "Quit",
        ),
    }
}

/// Generate JavaScript to update popup state without reloading HTML
pub(super) fn generate_popup_update_script() -> String {
    use crate::config::ThemeMode;

    let mut ui_language = String::from("en");
    let (
        bubble_checked,
        is_dark_mode,
        settings_text,
        bubble_text,
        stop_tts_text,
        restore_overlay_text,
        quit_text,
    ) = if let Ok(app) = APP.lock() {
        ui_language = app.config.ui_language.clone();
        let is_dark = match app.config.theme_mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => crate::gui::utils::is_system_in_dark_mode(),
        };
        let (settings, bubble, stop_tts, restore_overlay, quit) =
            get_popup_labels(&app.config.ui_language);
        (
            app.config.show_favorite_bubble,
            is_dark,
            settings,
            bubble,
            stop_tts,
            restore_overlay,
            quit,
        )
    } else {
        (
            false,
            true,
            "Settings",
            "Favorite Bubble",
            "Stop All Playing TTS",
            "Restore Last Closed Overlay",
            "Quit",
        )
    };

    let has_tts_pending = crate::api::tts::TTS_MANAGER.has_pending_audio();
    let can_restore_last_closed = crate::overlay::result::can_restore_last_closed();
    let restore_options = get_restore_options(&ui_language);
    let restore_options_json =
        serde_json::to_string(&restore_options).unwrap_or_else(|_| "[]".into());
    let restore_flyout_top = restore_flyout_top_logical(restore_options.len());

    let (bg_color, text_color, hover_color, border_color, separator_color) = if is_dark_mode {
        (
            "#2c2c2c",
            "#ffffff",
            "#3c3c3c",
            "#454545",
            "rgba(255,255,255,0.08)",
        )
    } else {
        (
            "#f9f9f9",
            "#1a1a1a",
            "#eaeaea",
            "#dcdcdc",
            "rgba(0,0,0,0.06)",
        )
    };

    format!(
        r#"window.updatePopupState({{
            bgColor: '{}',
            textColor: '{}',
            hoverColor: '{}',
            borderColor: '{}',
            separatorColor: '{}',
            bubbleActive: {},
            ttsDisabled: {},
            restoreDisabled: {},
            restoreOptions: {},
            restoreFlyoutTop: {},
            settingsText: '{}',
            bubbleText: '{}',
            stopTtsText: '{}',
            restoreOverlayText: '{}',
            quitText: '{}'
        }});"#,
        bg_color,
        text_color,
        hover_color,
        border_color,
        separator_color,
        bubble_checked,
        !has_tts_pending,
        !can_restore_last_closed,
        restore_options_json,
        restore_flyout_top,
        settings_text,
        bubble_text,
        stop_tts_text,
        restore_overlay_text,
        quit_text
    )
}
