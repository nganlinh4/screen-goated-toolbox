use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::DownloadManager;
use super::run::install_status;
use super::types::InstallStatus;
use super::utils::log;
use crate::component_registry::external_tools::{self, ExternalTool};

impl DownloadManager {
    pub fn start_download_ytdlp(&self) {
        self.start_tool_install(ExternalTool::YtDlp);
    }

    pub fn start_download_ffmpeg(&self) {
        self.start_tool_install(ExternalTool::Ffmpeg);
    }

    pub fn start_download_deno(&self) {
        self.start_tool_install(ExternalTool::Deno);
    }

    fn start_tool_install(&self, tool: ExternalTool) {
        let status = self.status_for(tool).clone();
        {
            let mut current = status.lock().unwrap();
            if matches!(
                *current,
                InstallStatus::Downloading(_) | InstallStatus::Extracting
            ) {
                return;
            }
            *current = InstallStatus::Downloading(0.0);
        }
        self.install_cancel_flag.store(false, Ordering::Relaxed);
        let cancel = self.install_cancel_flag.clone();
        let logs = self.install_logs.clone();
        std::thread::spawn(move || install_in_background(tool, status, cancel, logs));
    }
}

fn install_in_background(
    tool: ExternalTool,
    status: Arc<Mutex<InstallStatus>>,
    cancel: Arc<AtomicBool>,
    logs: Arc<Mutex<Vec<String>>>,
) {
    log(&logs, format!("Installing pinned {} component", tool.id()));
    let component_name = localized_tool_name(tool);
    let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&component_name);
    let progress_status = status.clone();
    let result = external_tools::ensure(tool, &cancel, move |done, total| {
        badge.report(done, total);
        let progress = done as f32 / total.max(1) as f32;
        *progress_status.lock().unwrap() = InstallStatus::Downloading(progress);
    });
    match result {
        Ok(component) => {
            log(
                &logs,
                format!(
                    "{} ready at {}",
                    tool.id(),
                    component.executable().display()
                ),
            );
            drop(component);
            *status.lock().unwrap() = InstallStatus::Installed;
        }
        Err(_) if cancel.load(Ordering::Relaxed) => {
            log(&logs, format!("{} installation cancelled", tool.id()));
            *status.lock().unwrap() = install_status(tool);
        }
        Err(error) => {
            log(
                &logs,
                format!("{} installation failed: {error:#}", tool.id()),
            );
            *status.lock().unwrap() = InstallStatus::Error(error.to_string());
        }
    }
}

fn localized_tool_name(tool: ExternalTool) -> String {
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let text = crate::gui::locale::LocaleText::get(&language);
    match tool {
        ExternalTool::YtDlp => text.auxiliary.managed_tools.tool_ytdlp,
        ExternalTool::Ffmpeg => text.auxiliary.managed_tools.tool_ffmpeg,
        ExternalTool::Deno => text.auxiliary.managed_tools.tool_deno,
    }
    .to_string()
}
