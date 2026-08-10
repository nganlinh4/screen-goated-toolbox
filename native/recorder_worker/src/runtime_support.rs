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
    fn supported() -> Self {
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

pub fn require_webview2(feature_name: &str) -> FeatureCapability {
    if find_webview2_executable().is_some() {
        FeatureCapability::supported()
    } else {
        let badge = crate::overlay::auto_copy_badge::locale_text();
        let name = if feature_name == "Screen record" {
            badge.feature_screen_record
        } else {
            feature_name
        };
        FeatureCapability {
            status: CapabilityStatus::MissingDependency,
            title: crate::overlay::auto_copy_badge::format_locale(
                badge.feature_needs_webview2_fmt,
                &[("name", name)],
            ),
            details: badge.install_webview2_hint.to_string(),
        }
    }
}

pub fn notify_capability_issue(capability: &FeatureCapability) {
    if !capability.is_supported() {
        crate::overlay::auto_copy_badge::show_detailed_notification(
            &capability.title,
            &capability.details,
            crate::overlay::auto_copy_badge::NotificationType::Info,
        );
    }
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
        .map(|entry| entry.path().join("msedgewebview2.exe"))
        .find(|candidate| candidate.exists())
}
