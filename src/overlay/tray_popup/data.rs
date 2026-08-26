use crate::APP;

#[derive(Clone)]
pub(super) struct PopupRestoreOption {
    pub batch_count: usize,
    pub label: String,
}

#[derive(Clone, Copy)]
pub(super) struct PopupLabels {
    pub settings: &'static str,
    pub bubble: &'static str,
    pub stop_tts: &'static str,
    pub restore: &'static str,
    pub quit: &'static str,
}

pub(super) struct PopupSnapshot {
    pub labels: PopupLabels,
    pub bubble_active: bool,
    pub tts_disabled: bool,
    pub restore_options: Vec<PopupRestoreOption>,
}

pub(super) fn restore_option_count() -> usize {
    crate::overlay::result::recent_restore_option_counts()
        .len()
        .min(5)
}

pub(super) fn snapshot() -> PopupSnapshot {
    let (language, bubble_active) = APP
        .lock()
        .map(|app| {
            (
                app.config.ui_language.clone(),
                app.config.show_favorite_bubble,
            )
        })
        .unwrap_or_else(|_| ("en".to_owned(), false));
    let restore_options = restore_options(&language);
    PopupSnapshot {
        labels: popup_labels(&language),
        bubble_active,
        tts_disabled: !crate::api::tts::TTS_MANAGER.has_pending_audio(),
        restore_options,
    }
}

fn restore_options(language: &str) -> Vec<PopupRestoreOption> {
    crate::overlay::result::recent_restore_option_counts()
        .into_iter()
        .take(5)
        .enumerate()
        .map(|(index, overlay_count)| PopupRestoreOption {
            batch_count: index + 1,
            label: restore_label(language, overlay_count),
        })
        .collect()
}

fn restore_label(language: &str, overlay_count: usize) -> String {
    match language {
        "vi" => format!("Khôi phục {overlay_count} overlay vừa đóng"),
        "ko" => format!("방금 닫은 오버레이 {overlay_count}개 복원"),
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

fn popup_labels(language: &str) -> PopupLabels {
    match language {
        "vi" => PopupLabels {
            settings: "Cài đặt",
            bubble: "Hiện bong bóng",
            stop_tts: "Dừng đọc",
            restore: "Khôi phục overlay vừa đóng",
            quit: "Thoát",
        },
        "ko" => PopupLabels {
            settings: "설정",
            bubble: "즐겨찾기 버블",
            stop_tts: "재생 중인 모든 음성 중지",
            restore: "방금 닫은 오버레이 복원",
            quit: "종료",
        },
        _ => PopupLabels {
            settings: "Settings",
            bubble: "Favorite Bubble",
            stop_tts: "Stop All Playing TTS",
            restore: "Restore Last Closed Overlay",
            quit: "Quit",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_restore_label_pluralizes() {
        assert_eq!(restore_label("en", 1), "Restore 1 recently closed overlay");
        assert_eq!(restore_label("en", 3), "Restore 3 recently closed overlays");
    }

    #[test]
    fn supported_locales_keep_the_five_actions() {
        for language in ["en", "vi", "ko"] {
            let labels = popup_labels(language);
            assert!(!labels.settings.is_empty());
            assert!(!labels.bubble.is_empty());
            assert!(!labels.stop_tts.is_empty());
            assert!(!labels.restore.is_empty());
            assert!(!labels.quit.is_empty());
        }
    }
}
