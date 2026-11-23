use std::collections::BTreeMap;

use super::protocol::HostCommand;

pub(super) fn strings_for(language: &str) -> BTreeMap<String, String> {
    let locale = crate::gui::locale::LocaleText::get(language);
    [
        ("copy", locale.overlay.overlay_copy_tooltip),
        ("copy_image", locale.overlay.overlay_copy_image_tooltip),
        (
            "copy_image_failed",
            locale.overlay.overlay_copy_image_failed,
        ),
        ("undo", locale.overlay.overlay_undo_tooltip),
        ("redo", locale.overlay.overlay_redo_tooltip),
        ("edit", locale.overlay.overlay_edit_tooltip),
        ("download", locale.overlay.overlay_download_tooltip),
        ("speaker", locale.overlay.overlay_speaker_tooltip),
        (
            "result_handle",
            locale.overlay.overlay_result_handle_tooltip,
        ),
        ("back", locale.overlay.overlay_back_tooltip),
        ("forward", locale.overlay.overlay_forward_tooltip),
        ("opacity", locale.overlay.overlay_opacity_tooltip),
        ("cancel", locale.overlay.overlay_cancel_tooltip),
        (
            "overlay_refine_placeholder",
            locale.overlay.overlay_refine_placeholder,
        ),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .collect()
}

pub(super) fn command_for(language: &str) -> HostCommand {
    HostCommand::Locale {
        strings: strings_for(language),
    }
}

pub(super) fn current_command() -> HostCommand {
    command_for(&crate::APP.lock().unwrap().config.ui_language)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_locales_keep_the_complete_control_contract_across_ipc() {
        let keys = strings_for("en").into_keys().collect::<Vec<_>>();
        for language in ["en", "vi", "ko"] {
            let strings = strings_for(language);
            assert_eq!(strings.keys().cloned().collect::<Vec<_>>(), keys);
            assert!(strings.values().all(|value| !value.is_empty()));
            assert_eq!(
                strings["overlay_refine_placeholder"],
                crate::gui::locale::LocaleText::get(language)
                    .overlay
                    .overlay_refine_placeholder
            );
            let command = command_for(language);
            let wire = serde_json::to_string(&command).unwrap();
            assert_eq!(serde_json::from_str::<HostCommand>(&wire).unwrap(), command);
        }
    }
}
