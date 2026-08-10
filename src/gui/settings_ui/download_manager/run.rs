use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::thread;

#[cfg(windows)]
use std::os::windows::process::CommandExt as _;

use super::DownloadManager;
use super::types::{DownloadState, InstallStatus};
use super::utils::log;
use crate::component_registry::RemovalOutcome;
use crate::component_registry::external_tools::{
    ExternalTool, ExternalToolStatus, current_status, remove,
};

impl DownloadManager {
    pub fn check_status(&self) {
        let ytdlp = self.ytdlp_status.clone();
        let ffmpeg = self.ffmpeg_status.clone();
        let deno = self.deno_status.clone();
        let logs = self.install_logs.clone();
        thread::spawn(move || {
            for (tool, status) in [
                (ExternalTool::YtDlp, ytdlp),
                (ExternalTool::Ffmpeg, ffmpeg),
                (ExternalTool::Deno, deno),
            ] {
                let resolved = install_status(tool);
                if !matches!(resolved, InstallStatus::Installed) {
                    log(&logs, format!("{} is not ready", tool.id()));
                }
                *status.lock().unwrap() = resolved;
            }
        });
    }

    pub fn get_dependency_sizes(&self) -> (String, String, String) {
        (
            tool_size(ExternalTool::YtDlp),
            tool_size(ExternalTool::Ffmpeg),
            tool_size(ExternalTool::Deno),
        )
    }

    pub(crate) fn remove_tool(&self, tool: ExternalTool) -> anyhow::Result<()> {
        let outcome = remove(tool)?;
        match outcome {
            RemovalOutcome::Missing | RemovalOutcome::Removed => {
                *self.status_for(tool).lock().unwrap() = install_status(tool);
                Ok(())
            }
            RemovalOutcome::Pending => {
                *self.status_for(tool).lock().unwrap() = InstallStatus::Missing;
                log(
                    &self.install_logs,
                    format!(
                        "{} removal is pending and will finish after active use ends",
                        tool.id()
                    ),
                );
                Ok(())
            }
            RemovalOutcome::RequiredBy(dependents) => anyhow::bail!(
                "{} is required by installed components: {}",
                tool.id(),
                dependents.join(", ")
            ),
            RemovalOutcome::PreservedModified(paths) => anyhow::bail!(
                "{} contains {} modified or unknown file(s); they were preserved",
                tool.id(),
                paths.len()
            ),
        }
    }

    pub fn delete_dependencies(&self) {
        for tool in ExternalTool::ALL {
            if let Err(error) = self.remove_tool(tool) {
                log(
                    &self.install_logs,
                    format!("Could not remove {}: {error:#}", tool.id()),
                );
            }
        }
    }

    pub fn cancel_download(&self) {
        let idx = self.active_idx();
        if let Some(session) = self.sessions.get(idx) {
            session.cancel_flag.store(true, Ordering::Relaxed);
            if let Ok(mut state) = session.download_state.lock() {
                let progress = match &*state {
                    DownloadState::Downloading(progress, _) => Some(*progress),
                    _ => None,
                };
                if let Some(progress) = progress {
                    *state = DownloadState::Downloading(progress, "Cancelling...".to_string());
                }
            }
        }
    }

    pub fn change_download_folder(&mut self) {
        let mut command = std::process::Command::new("powershell");
        command.args([
            "-Command",
            "Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.FolderBrowserDialog; $f.ShowDialog() | Out-Null; $f.SelectedPath",
        ]);
        #[cfg(windows)]
        command.creation_flags(0x08000000);
        if let Ok(output) = command.output()
            && let Ok(path) = String::from_utf8(output.stdout)
        {
            let path = path.trim();
            if !path.is_empty() {
                self.custom_download_path = Some(PathBuf::from(path));
                self.save_settings();
            }
        }
    }

    pub(super) fn status_for(
        &self,
        tool: ExternalTool,
    ) -> &std::sync::Arc<std::sync::Mutex<InstallStatus>> {
        match tool {
            ExternalTool::YtDlp => &self.ytdlp_status,
            ExternalTool::Ffmpeg => &self.ffmpeg_status,
            ExternalTool::Deno => &self.deno_status,
        }
    }
}

pub(super) fn install_status(tool: ExternalTool) -> InstallStatus {
    match current_status(tool) {
        ExternalToolStatus::Installed { .. } => InstallStatus::Installed,
        ExternalToolStatus::Missing | ExternalToolStatus::Unavailable => InstallStatus::Missing,
        ExternalToolStatus::Error(error) => InstallStatus::Error(error),
    }
}

fn tool_size(tool: ExternalTool) -> String {
    let bytes = match current_status(tool) {
        ExternalToolStatus::Installed { bytes } => bytes,
        _ => 0,
    };
    format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
}
