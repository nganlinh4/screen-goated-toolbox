//! A single native folder dialog with downloader-owned completion.
use eframe::egui;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, mpsc};

use super::DownloadManager;

type PickResult = Result<Option<PathBuf>, String>;
static DIALOG_BUSY: LazyLock<Arc<AtomicBool>> = LazyLock::new(|| Arc::new(AtomicBool::new(false)));

struct Permit(Arc<AtomicBool>);

impl Drop for Permit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(super) struct FolderPicker {
    gate: Arc<AtomicBool>,
    next_generation: u64,
    active: Option<u64>,
    receiver: Option<mpsc::Receiver<(u64, PickResult)>>,
}

impl FolderPicker {
    pub fn new() -> Self {
        Self::with_gate(DIALOG_BUSY.clone())
    }

    fn with_gate(gate: Arc<AtomicBool>) -> Self {
        Self {
            gate,
            next_generation: 0,
            active: None,
            receiver: None,
        }
    }

    pub fn available(&self) -> bool {
        self.active.is_none() && !self.gate.load(Ordering::Acquire)
    }

    pub fn start(&mut self, initial: Option<PathBuf>, ctx: egui::Context) -> Result<(), String> {
        self.start_with(move || pick_folder(initial), move || ctx.request_repaint())
    }

    fn start_with(
        &mut self,
        pick: impl FnOnce() -> PickResult + Send + 'static,
        wake: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        if self.active.is_some() {
            return Err("folder picker already active".into());
        }
        let generation = self
            .next_generation
            .checked_add(1)
            .ok_or("folder picker generation exhausted")?;
        self.gate
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "folder picker already active")?;
        let permit = Permit(self.gate.clone());
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("downloader-folder-picker".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(pick))
                    .unwrap_or_else(|_| Err("folder picker failed".into()));
                drop(permit);
                let _ = sender.send((generation, result));
                wake();
            })
            .map_err(|error| format!("could not start folder picker: {error}"))?;
        self.next_generation = generation;
        self.active = Some(generation);
        self.receiver = Some(receiver);
        Ok(())
    }

    pub fn retire(&mut self) {
        self.active = None;
        self.receiver = None;
    }

    fn poll(&mut self) -> Option<PickResult> {
        let received = self.receiver.as_ref()?.try_recv();
        match received {
            Ok((generation, result)) if self.active == Some(generation) => {
                self.retire();
                Some(result)
            }
            Ok(_) => None,
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.retire();
                Some(Err("folder picker stopped unexpectedly".into()))
            }
        }
    }
}

impl DownloadManager {
    pub(super) fn poll_folder_picker(&mut self, text: &crate::gui::locale::LocaleText) {
        match self.folder_picker.poll() {
            Some(Ok(Some(path))) => {
                let mut config = self.settings_snapshot();
                if let Err(error) = commit_choice(&mut self.custom_download_path, path, |chosen| {
                    config.custom_download_path = Some(chosen.clone());
                    super::persistence::try_save_config(&config)
                }) {
                    crate::log_info!("[Downloader] Could not save folder choice: {error}");
                    crate::overlay::auto_copy_badge::show_error_notification(
                        text.shell.settings_save_failed_title,
                    );
                }
            }
            Some(Err(error)) => {
                crate::log_info!("[Downloader] Folder picker failed: {error}");
                crate::overlay::auto_copy_badge::show_error_notification(
                    text.auxiliary.download.download_status_error,
                );
            }
            Some(Ok(None)) | None => {}
        }
    }
}

fn commit_choice(
    current: &mut Option<PathBuf>,
    selected: PathBuf,
    save: impl FnOnce(&PathBuf) -> std::io::Result<()>,
) -> std::io::Result<()> {
    save(&selected)?;
    *current = Some(selected);
    Ok(())
}

fn pick_folder(initial: Option<PathBuf>) -> PickResult {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
        CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog,
        IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::{HRESULT, PCWSTR, PWSTR};

    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() }
        }
    }
    struct OwnedPath(PWSTR);
    impl Drop for OwnedPath {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(Some(self.0.0.cast())) }
        }
    }

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|e| e.to_string())?;
        let _apartment = Apartment;
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).map_err(|e| e.to_string())?;
        dialog
            .SetOptions(FOS_PICKFOLDERS | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM)
            .map_err(|e| e.to_string())?;
        if let Some(initial) = initial {
            let wide: Vec<u16> = initial.as_os_str().encode_wide().chain(Some(0)).collect();
            if let Ok(folder) =
                SHCreateItemFromParsingName::<PCWSTR, _, IShellItem>(PCWSTR(wide.as_ptr()), None)
            {
                let _ = dialog.SetFolder(&folder);
            }
        }
        if let Err(error) = dialog.Show(None) {
            if error.code() == HRESULT(0x8007_04c7_u32 as i32) {
                return Ok(None);
            }
            return Err(error.to_string());
        }
        let item = dialog.GetResult().map_err(|e| e.to_string())?;
        let value = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|e| e.to_string())?;
        if value.is_null() {
            return Err("folder picker returned no path".into());
        }
        let path = OwnedPath(value);
        let name = std::ffi::OsString::from_wide(path.0.as_wide());
        if name.is_empty() {
            return Err("folder picker returned an empty path".into());
        }
        Ok(Some(PathBuf::from(name)))
    }
}

#[cfg(test)]
#[path = "folder_picker_tests.rs"]
mod tests;
