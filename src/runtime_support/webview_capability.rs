//! WebView2 availability for the features that render through it.
//!
//! The host and the recorder worker share this one module, so every feature
//! that needs WebView2 is named by a typed variant and localized the same way
//! in both processes. Host-only features are compiled out of the worker.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityStatus {
    Supported,
    MissingDependency,
}

#[derive(Clone, Debug)]
pub struct FeatureCapability {
    pub status: CapabilityStatus,
    pub title: String,
    pub details: String,
}

impl FeatureCapability {
    pub fn supported() -> Self {
        Self {
            status: CapabilityStatus::Supported,
            title: String::new(),
            details: String::new(),
        }
    }

    pub fn is_supported(&self) -> bool {
        self.status == CapabilityStatus::Supported
    }
}

/// Every feature that needs WebView2. The exhaustive match in
/// [`WebViewFeature::localized_name`] forces each one to have a localized name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebViewFeature {
    WindowSelector,
    // Only the recorder worker opens the Screen Record WebView.
    #[cfg(feature = "recorder-worker")]
    ScreenRecord,
    #[cfg(not(feature = "recorder-worker"))]
    RealtimeOverlay,
    #[cfg(not(feature = "recorder-worker"))]
    PresetWheel,
    #[cfg(not(feature = "recorder-worker"))]
    TtsPlayground,
    #[cfg(not(feature = "recorder-worker"))]
    TextInput,
    #[cfg(not(feature = "recorder-worker"))]
    TranslationGummy,
    #[cfg(not(feature = "recorder-worker"))]
    ImageToSvg,
    #[cfg(not(feature = "recorder-worker"))]
    ThreeDGenerator,
    #[cfg(not(feature = "recorder-worker"))]
    PromptDj,
}

impl WebViewFeature {
    pub fn localized_name(self, text: &crate::gui::locale::LocaleText) -> &'static str {
        let badge = &text.badge;
        match self {
            Self::WindowSelector => badge.feature_window_selector,
            #[cfg(feature = "recorder-worker")]
            Self::ScreenRecord => badge.feature_screen_record,
            #[cfg(not(feature = "recorder-worker"))]
            Self::RealtimeOverlay => badge.feature_realtime_overlay,
            #[cfg(not(feature = "recorder-worker"))]
            Self::PresetWheel => badge.feature_preset_wheel,
            #[cfg(not(feature = "recorder-worker"))]
            Self::TtsPlayground => badge.feature_tts_playground,
            #[cfg(not(feature = "recorder-worker"))]
            Self::TextInput => badge.feature_text_input,
            #[cfg(not(feature = "recorder-worker"))]
            Self::TranslationGummy => text.translation_gummy.translation_gummy_title,
            #[cfg(not(feature = "recorder-worker"))]
            Self::ImageToSvg => text.shell.image_to_svg_title,
            #[cfg(not(feature = "recorder-worker"))]
            Self::ThreeDGenerator => text.shell.three_d_generator_title,
            #[cfg(not(feature = "recorder-worker"))]
            Self::PromptDj => text.shell.prompt_dj_btn,
        }
    }
}

/// Whether `feature` can open now; when WebView2 is missing, the localized
/// notice that tells the user how to install it.
pub fn require_webview2(feature: WebViewFeature) -> FeatureCapability {
    if webview2_runtime_installed() {
        return FeatureCapability::supported();
    }
    let ui_language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    missing_webview2(feature, &crate::gui::locale::LocaleText::get(&ui_language))
}

fn missing_webview2(
    feature: WebViewFeature,
    text: &crate::gui::locale::LocaleText,
) -> FeatureCapability {
    FeatureCapability {
        status: CapabilityStatus::MissingDependency,
        title: crate::overlay::auto_copy_badge::format_locale(
            text.badge.feature_needs_webview2_fmt,
            &[("name", feature.localized_name(text))],
        ),
        details: text.badge.install_webview2_hint.to_string(),
    }
}

pub fn notify_capability_issue(capability: &FeatureCapability) {
    if capability.is_supported() {
        return;
    }
    crate::overlay::auto_copy_badge::show_detailed_notification(
        &capability.title,
        &capability.details,
        crate::overlay::auto_copy_badge::NotificationType::Info,
    );
}

pub fn webview2_runtime_installed() -> bool {
    find_webview2_executable().is_some()
}

fn find_webview2_executable() -> Option<PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)", "LocalAppData"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(PathBuf::from)
        .find_map(|root| {
            find_webview2_under(
                &root
                    .join("Microsoft")
                    .join("EdgeWebView")
                    .join("Application"),
            )
        })
}

fn find_webview2_under(path: &Path) -> Option<PathBuf> {
    let direct = path.join("msedgewebview2.exe");
    if direct.exists() {
        return Some(direct);
    }
    fs::read_dir(path)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|entry| entry.is_dir())
        .map(|entry| entry.join("msedgewebview2.exe"))
        .find(|candidate| candidate.exists())
}

#[cfg(test)]
mod tests {
    use super::{CapabilityStatus, WebViewFeature, missing_webview2};
    use crate::gui::locale::LocaleText;

    fn features() -> Vec<WebViewFeature> {
        [
            WebViewFeature::WindowSelector,
            #[cfg(feature = "recorder-worker")]
            WebViewFeature::ScreenRecord,
            #[cfg(not(feature = "recorder-worker"))]
            WebViewFeature::RealtimeOverlay,
            #[cfg(not(feature = "recorder-worker"))]
            WebViewFeature::PresetWheel,
            #[cfg(not(feature = "recorder-worker"))]
            WebViewFeature::TtsPlayground,
            #[cfg(not(feature = "recorder-worker"))]
            WebViewFeature::TextInput,
            #[cfg(not(feature = "recorder-worker"))]
            WebViewFeature::TranslationGummy,
            #[cfg(not(feature = "recorder-worker"))]
            WebViewFeature::ImageToSvg,
            #[cfg(not(feature = "recorder-worker"))]
            WebViewFeature::ThreeDGenerator,
            #[cfg(not(feature = "recorder-worker"))]
            WebViewFeature::PromptDj,
        ]
        .to_vec()
    }

    #[test]
    fn the_missing_webview2_notice_names_each_feature_in_the_ui_language() {
        let english = LocaleText::get("en");
        for language in ["ko", "vi"] {
            let text = LocaleText::get(language);
            for feature in features() {
                let notice = missing_webview2(feature, &text);
                let localized = feature.localized_name(&text);
                let untranslated = feature.localized_name(&english);
                assert_eq!(notice.status, CapabilityStatus::MissingDependency);
                assert!(!localized.is_empty(), "{language} {feature:?}");
                assert_eq!(
                    notice.title,
                    text.badge
                        .feature_needs_webview2_fmt
                        .replace("{name}", localized),
                    "{language} {feature:?}"
                );
                if localized != untranslated {
                    assert!(
                        !notice.title.contains(untranslated),
                        "{language} {feature:?}: {}",
                        notice.title
                    );
                }
                assert_eq!(notice.details, text.badge.install_webview2_hint);
            }
        }
    }
}
