use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::overlay::window_selector::{
    self, SelectorCallbacks, SelectorEntry, SelectorNotice, SelectorOwner, SelectorText,
};

use super::{SR_HWND, WM_APP_RUN_SCRIPT};

fn json_u32(value: &serde_json::Value, key: &str, default: u32) -> u32 {
    value[key]
        .as_u64()
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(default)
}

fn to_selector_entry(
    window: &serde_json::Value,
    display_only_badge: &str,
    display_only_notice: &SelectorNotice,
) -> Option<SelectorEntry> {
    let id = window["id"].as_str()?.to_string();
    let title = window["title"].as_str()?.to_string();
    let subtitle = window["processName"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let disabled = window["isAdmin"].as_bool().unwrap_or(false);
    let display_only = window["displayOnly"].as_bool().unwrap_or(false);
    let badge_text = if disabled {
        Some("ADMIN".to_string())
    } else {
        display_only.then(|| display_only_badge.to_string())
    };

    Some(SelectorEntry {
        id,
        title,
        subtitle,
        icon_data_url: window["iconDataUrl"].as_str().map(ToOwned::to_owned),
        preview_data_url: window["previewDataUrl"].as_str().map(ToOwned::to_owned),
        width: json_u32(window, "winW", 16),
        height: json_u32(window, "winH", 9),
        badge_text,
        selection_notice: display_only.then(|| display_only_notice.clone()),
        disabled,
    })
}

fn post_screen_record_script(script: String) {
    let sr_hwnd_val = unsafe { std::ptr::addr_of!(SR_HWND).read().0.0 as isize };
    if sr_hwnd_val == 0 {
        return;
    }

    let script_ptr = Box::into_raw(Box::new(script));
    unsafe {
        let _ = PostMessageW(
            Some(HWND(sr_hwnd_val as *mut _)),
            WM_APP_RUN_SCRIPT,
            WPARAM(0),
            LPARAM(script_ptr as isize),
        );
    }
}

pub fn selector_is_closed() -> bool {
    !window_selector::is_owner_active(SelectorOwner::ScreenRecord)
}

pub fn post_thumbnail_update(window_id: usize, data_url: String) {
    window_selector::post_preview_update_for_owner(
        SelectorOwner::ScreenRecord,
        &window_id.to_string(),
        data_url,
    );
}

pub fn show_window_selector(windows_data: Vec<serde_json::Value>, is_dark: bool, lang: String) {
    let locale = crate::gui::locale::LocaleText::get(&lang);
    let display_only_notice = SelectorNotice {
        title: locale.auxiliary.win_select_display_only_title.to_string(),
        message: locale.auxiliary.win_select_display_only_message.to_string(),
        action_label: locale.auxiliary.win_select_display_only_action.to_string(),
    };
    let entries: Vec<SelectorEntry> = windows_data
        .iter()
        .filter_map(|window| {
            to_selector_entry(
                window,
                locale.auxiliary.win_select_display_only_badge,
                &display_only_notice,
            )
        })
        .collect();
    if entries.is_empty() {
        return;
    }
    let entry_count = entries.len();

    let callbacks = SelectorCallbacks::new(
        |window_id| {
            let window_id: String = window_id.chars().filter(|ch| ch.is_ascii_digit()).collect();
            if window_id.is_empty() {
                return;
            }

            post_screen_record_script(format!(
                "window.dispatchEvent(new CustomEvent('external-window-selected',{{detail:{{windowId:'{}'}}}}))",
                window_id
            ));
        },
        || {
            post_screen_record_script(
                "window.dispatchEvent(new CustomEvent('external-window-selection-cancelled'))"
                    .to_string(),
            );
        },
    );

    window_selector::show_selector(
        SelectorOwner::ScreenRecord,
        entries,
        is_dark,
        SelectorText {
            title: locale.auxiliary.win_select_title.to_string(),
            subtitle: locale.auxiliary.win_select_subtitle.to_string(),
            count_label: locale
                .auxiliary
                .win_select_count
                .replace("{}", &entry_count.to_string()),
            cancel_label: locale.preset_basics.cancel_label.to_string(),
        },
        callbacks,
    );
}
