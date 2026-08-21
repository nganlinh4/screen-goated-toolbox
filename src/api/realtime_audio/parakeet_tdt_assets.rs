use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use crate::api::realtime_audio::model_loader::{FileContract, contract_file_present};
use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const PARAKEET_TDT_REVISION: &str = "17793985b4bda8fbceb90ec85c1d94caa0c1b197";
const REQUIRED_FILES: &[FileContract] = &[
    FileContract {
        name: "encoder-model.onnx",
        url: "https://huggingface.co/maxkulish/parakeet-tdt-0.6b-v3/resolve/17793985b4bda8fbceb90ec85c1d94caa0c1b197/encoder-model.onnx",
        size_bytes: 41_770_866,
        sha256: "98a74b21b4cc0017c1e7030319a4a96f4a9506e50f0708f3a516d02a77c96bb1",
    },
    FileContract {
        name: "encoder-model.onnx.data",
        url: "https://huggingface.co/maxkulish/parakeet-tdt-0.6b-v3/resolve/17793985b4bda8fbceb90ec85c1d94caa0c1b197/encoder-model.onnx.data",
        size_bytes: 2_435_420_160,
        sha256: "9a22d372c51455c34f13405da2520baefb7125bd16981397561423ed32d24f36",
    },
    FileContract {
        name: "decoder_joint-model.onnx",
        url: "https://huggingface.co/maxkulish/parakeet-tdt-0.6b-v3/resolve/17793985b4bda8fbceb90ec85c1d94caa0c1b197/decoder_joint-model.onnx",
        size_bytes: 72_520_893,
        sha256: "e978ddf6688527182c10fde2eb4b83068421648985ef23f7a86be732be8706c1",
    },
    FileContract {
        name: "vocab.txt",
        url: "https://huggingface.co/maxkulish/parakeet-tdt-0.6b-v3/resolve/17793985b4bda8fbceb90ec85c1d94caa0c1b197/vocab.txt",
        size_bytes: 93_939,
        sha256: "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
    },
];

pub(crate) fn parakeet_tdt_model_contracts() -> &'static [FileContract] {
    REQUIRED_FILES
}

static LAST_PARAKEET_TDT_ACTION_ERROR: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

fn set_parakeet_tdt_action_error(message: impl Into<String>) {
    *LAST_PARAKEET_TDT_ACTION_ERROR.lock().unwrap() = Some(message.into());
}

fn clear_parakeet_tdt_action_error() {
    *LAST_PARAKEET_TDT_ACTION_ERROR.lock().unwrap() = None;
}

fn post_download_state() {
    use crate::overlay::realtime_webview::state::REALTIME_HWND;

    unsafe {
        if !std::ptr::addr_of!(REALTIME_HWND).read().is_invalid() {
            let _ = PostMessageW(
                Some(REALTIME_HWND),
                WM_DOWNLOAD_PROGRESS,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

fn locale() -> crate::gui::locale::LocaleText {
    let app = crate::APP.lock().unwrap();
    crate::gui::locale::LocaleText::get(&app.config.ui_language)
}

fn model_files_present(dir: &Path) -> bool {
    REQUIRED_FILES
        .iter()
        .all(|contract| contract_file_present(&dir.join(contract.name), *contract))
}

#[cfg(not(feature = "recorder-worker"))]
pub fn current_parakeet_tdt_model_notice() -> Option<String> {
    LAST_PARAKEET_TDT_ACTION_ERROR
        .lock()
        .unwrap()
        .clone()
        .or_else(|| {
            super::local_asr_worker::model_notice(super::local_asr_worker::ModelKind::SubtitleTdt)
        })
}

pub fn get_parakeet_tdt_model_dir() -> PathBuf {
    crate::paths::app_models_dir().join("parakeet_tdt_0_6b_v3")
}

pub fn is_parakeet_tdt_model_downloaded() -> bool {
    model_files_present(&get_parakeet_tdt_model_dir())
}

#[cfg(not(feature = "recorder-worker"))]
pub fn remove_parakeet_tdt_model() -> Result<()> {
    let _owners = crate::overlay::component_removal::stop_audio_owners()?;
    let dir = get_parakeet_tdt_model_dir();
    clear_parakeet_tdt_action_error();
    super::local_asr_worker::request_model_remove(
        super::local_asr_worker::ModelKind::SubtitleTdt,
        &dir,
    )?;
    Ok(())
}

pub fn download_parakeet_tdt_model(stop_signal: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let dir = get_parakeet_tdt_model_dir();
    let locale = locale();

    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = locale
            .tool_runtime
            .parakeet_tdt_downloading_title
            .to_string();
        state.download_message = locale.tool_runtime.parakeet_downloading_message.to_string();
        state.download_progress = 0.0;
    }
    clear_parakeet_tdt_action_error();
    post_download_state();
    let badge = use_badge.then(|| {
        crate::overlay::auto_copy_badge::DownloadProgressBadge::with_text(
            locale.tool_runtime.parakeet_tdt_downloading_title,
            locale.tool_runtime.parakeet_downloading_message,
        )
    });

    let result: Result<()> = (|| {
        crate::log_info!("[ParakeetTDT] model revision {PARAKEET_TDT_REVISION}");
        let total_bytes = REQUIRED_FILES
            .iter()
            .map(|contract| contract.size_bytes)
            .sum::<u64>();
        let mut completed_bytes = 0_u64;
        for contract in REQUIRED_FILES {
            if stop_signal.load(Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled"));
            }

            let filename = contract.name;

            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = locale
                    .tool_runtime
                    .parakeet_downloading_file
                    .replace("{}", filename);
            }
            post_download_state();

            super::model_loader::download_verified_file_with_progress(
                *contract,
                contract.url,
                &dir.join(filename),
                &stop_signal,
                |done, _| {
                    if let Some(badge) = &badge {
                        badge.report(completed_bytes.saturating_add(done), total_bytes);
                    }
                },
            )?;
            completed_bytes = completed_bytes.saturating_add(contract.size_bytes);
        }

        if !model_files_present(&dir) {
            return Err(anyhow!(
                "Parakeet TDT model download finished with missing files"
            ));
        }
        Ok(())
    })();

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    if let Some(badge) = &badge {
        badge.finish();
    }
    post_download_state();

    if let Err(err) = &result {
        if !err.to_string().contains("cancelled") {
            set_parakeet_tdt_action_error(err.to_string());
        }
    } else {
        clear_parakeet_tdt_action_error();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tdt_model_urls_are_commit_pinned_and_integrity_bounded() {
        assert_eq!(PARAKEET_TDT_REVISION.len(), 40);
        for file in REQUIRED_FILES {
            assert!(file.url.contains(PARAKEET_TDT_REVISION));
            assert!(file.size_bytes > 0);
            assert_eq!(file.sha256.len(), 64);
        }
    }
}
