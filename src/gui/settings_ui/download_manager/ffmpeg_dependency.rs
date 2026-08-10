use std::sync::atomic::AtomicBool;

use crate::component_registry::external_tools::{self, ExternalTool, ExternalToolUse};

pub(crate) fn acquire_ffmpeg_with_badge() -> Result<ExternalToolUse, String> {
    acquire_ffmpeg_with_badge_message("")
}

pub(crate) fn acquire_ffmpeg_with_badge_message(
    download_message: &str,
) -> Result<ExternalToolUse, String> {
    if let Ok(installed) = external_tools::acquire_installed(ExternalTool::Ffmpeg) {
        return Ok(installed);
    }

    let mut text = localized_badge_text();
    if !download_message.trim().is_empty() {
        text.downloading = download_message.to_string();
    }
    let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::with_text(
        &text.installing,
        &text.downloading,
    );
    let cancelled = AtomicBool::new(false);
    let result = external_tools::ensure(ExternalTool::Ffmpeg, &cancelled, |done, total| {
        badge.report(done, total);
    });
    badge.finish();
    match result {
        Ok(component) => {
            crate::overlay::auto_copy_badge::show_notification(&text.installed);
            Ok(component)
        }
        Err(error) => {
            crate::overlay::auto_copy_badge::show_error_notification(&text.failed);
            Err(error.to_string())
        }
    }
}

#[derive(Clone)]
struct FfmpegBadgeText {
    installing: String,
    downloading: String,
    installed: String,
    failed: String,
}

fn localized_badge_text() -> FfmpegBadgeText {
    let ui_language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let text = crate::gui::locale::LocaleText::get(&ui_language);
    FfmpegBadgeText {
        installing: text
            .tts_playground
            .tts_playground_ffmpeg_installing
            .to_string(),
        downloading: text
            .tts_playground
            .tts_playground_ffmpeg_downloading
            .to_string(),
        installed: text
            .tts_playground
            .tts_playground_ffmpeg_installed
            .to_string(),
        failed: text.tts_playground.tts_playground_ffmpeg_failed.to_string(),
    }
}
