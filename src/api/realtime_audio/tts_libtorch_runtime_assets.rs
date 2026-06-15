use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

// -------------------------------------------------------------------------
// Downloadable runtime DLLs for libtorch-backed TTS providers.
// -------------------------------------------------------------------------

const LIBTORCH_URL: &str =
    "https://download.pytorch.org/libtorch/cu128/libtorch-win-shared-with-deps-2.7.1%2Bcu128.zip";
const LIBTORCH_REQUIRED_DLLS: &[&str] =
    &["torch_cpu.dll", "torch_cuda.dll", "c10.dll", "c10_cuda.dll"];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TtsLibtorchProvider {
    Voxtral,
}

#[derive(Clone, Copy)]
pub struct TtsRuntimeSpec {
    pub provider: TtsLibtorchProvider,
    pub label: &'static str,
    pub dll_filename: &'static str,
    pub dll_url: &'static str,
    pub download_title: &'static str,
}

pub const VOXTRAL_RUNTIME: TtsRuntimeSpec = TtsRuntimeSpec {
    provider: TtsLibtorchProvider::Voxtral,
    label: "Voxtral runtime DLL",
    dll_filename: "sgt_voxtral_runtime.dll",
    dll_url: "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/native/voxtral_runtime/dist/sgt_voxtral_runtime.dll",
    download_title: "Downloading Voxtral runtime",
};

static LAST_TTS_RUNTIME_NOTICE: LazyLock<
    std::sync::Mutex<std::collections::HashMap<TtsLibtorchProvider, String>>,
> = LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

static TTS_RUNTIME_DOWNLOAD_IN_PROGRESS: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn tts_runtime_dll_path(spec: TtsRuntimeSpec) -> PathBuf {
    crate::unpack_dlls::private_bin_dir().join(spec.dll_filename)
}

pub fn is_tts_runtime_downloading() -> bool {
    TTS_RUNTIME_DOWNLOAD_IN_PROGRESS.load(std::sync::atomic::Ordering::Relaxed)
}

fn set_tts_runtime_notice(provider: TtsLibtorchProvider, message: impl Into<String>) {
    if let Ok(mut notices) = LAST_TTS_RUNTIME_NOTICE.lock() {
        notices.insert(provider, message.into());
    }
}

fn clear_tts_runtime_notice(provider: TtsLibtorchProvider) {
    if let Ok(mut notices) = LAST_TTS_RUNTIME_NOTICE.lock() {
        notices.remove(&provider);
    }
}

fn post_tts_runtime_state() {
    use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
    use crate::overlay::realtime_webview::state::REALTIME_HWND;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

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

fn libtorch_required_files_present(dir: &Path) -> bool {
    LIBTORCH_REQUIRED_DLLS
        .iter()
        .all(|name| dir.join(name).is_file())
}

pub fn is_tts_runtime_installed(spec: TtsRuntimeSpec) -> bool {
    let bin_dir = crate::unpack_dlls::private_bin_dir();
    bin_dir.join(spec.dll_filename).is_file() && libtorch_required_files_present(&bin_dir)
}

pub fn ensure_tts_runtime(
    spec: TtsRuntimeSpec,
    stop_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    use_badge: bool,
) -> Result<()> {
    if is_tts_runtime_installed(spec) {
        return Ok(());
    }
    download_tts_runtime(spec, stop_signal, use_badge)
}

pub fn download_tts_runtime(
    spec: TtsRuntimeSpec,
    stop_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
    _use_badge: bool,
) -> Result<()> {
    if TTS_RUNTIME_DOWNLOAD_IN_PROGRESS
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        )
        .is_err()
    {
        while is_tts_runtime_downloading() {
            std::thread::sleep(std::time::Duration::from_millis(300));
            if stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled while waiting"));
            }
        }
        return if is_tts_runtime_installed(spec) {
            Ok(())
        } else {
            Err(anyhow!(
                "TTS runtime download did not complete successfully"
            ))
        };
    }

    let result = download_tts_runtime_inner(spec, stop_signal);
    TTS_RUNTIME_DOWNLOAD_IN_PROGRESS.store(false, std::sync::atomic::Ordering::SeqCst);
    post_tts_runtime_state();
    if let Err(err) = &result {
        if !err.to_string().contains("cancelled") {
            set_tts_runtime_notice(spec.provider, err.to_string());
        }
    } else {
        clear_tts_runtime_notice(spec.provider);
    }
    result
}

fn download_tts_runtime_inner(
    spec: TtsRuntimeSpec,
    stop_signal: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<()> {
    use crate::overlay::realtime_webview::state::REALTIME_STATE;

    let bin_dir = crate::unpack_dlls::private_bin_dir();
    std::fs::create_dir_all(&bin_dir)?;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = spec.download_title.to_string();
        state.download_message = format!("Downloading {}...", spec.dll_filename);
        state.download_progress = 0.0;
    }
    post_tts_runtime_state();

    let dll_path = tts_runtime_dll_path(spec);
    if !dll_path.is_file() {
        crate::api::realtime_audio::model_loader::download_file_with_progress(
            spec.dll_url,
            &dll_path,
            &stop_signal,
            |downloaded, total| {
                if let Ok(mut state) = REALTIME_STATE.lock() {
                    state.download_progress = if total > 0 {
                        (downloaded as f32 / total as f32) * 5.0
                    } else {
                        0.0
                    };
                }
                post_tts_runtime_state();
            },
        )?;
    }

    if stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(anyhow!("Download cancelled"));
    }

    if !libtorch_required_files_present(&bin_dir) {
        if let Ok(mut state) = REALTIME_STATE.lock() {
            state.download_message =
                "Downloading shared libtorch CUDA runtime (~2.5 GB)...".to_string();
            state.download_progress = 5.0;
        }
        post_tts_runtime_state();
        download_and_extract_libtorch(&bin_dir, &stop_signal)?;
    }

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
        state.download_progress = 100.0;
    }
    if !is_tts_runtime_installed(spec) {
        return Err(anyhow!(
            "{} install is incomplete after download",
            spec.label
        ));
    }
    Ok(())
}

fn download_and_extract_libtorch(
    bin_dir: &Path,
    stop_signal: &std::sync::atomic::AtomicBool,
) -> Result<()> {
    use crate::overlay::realtime_webview::state::REALTIME_STATE;

    let zip_path = bin_dir.join("libtorch-download.zip");
    let _ = std::fs::remove_file(&zip_path);
    let mut curl_child = std::process::Command::new("curl.exe")
        .args([
            "--fail",
            "--location",
            "--continue-at",
            "-",
            "--output",
            &zip_path.to_string_lossy(),
            LIBTORCH_URL,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|err| anyhow!("Failed to start curl for libtorch download: {err}"))?;

    let expected_size = 2_660_000_000_f64;
    loop {
        match curl_child.try_wait()? {
            Some(status) if !status.success() => {
                return Err(anyhow!(
                    "libtorch download failed (curl exit code {status})"
                ));
            }
            Some(_) => break,
            None => {
                if stop_signal.load(std::sync::atomic::Ordering::Relaxed) {
                    let _ = curl_child.kill();
                    let _ = std::fs::remove_file(&zip_path);
                    return Err(anyhow!("Download cancelled"));
                }
                let bytes = std::fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0);
                if let Ok(mut state) = REALTIME_STATE.lock() {
                    state.download_message = format!(
                        "Downloading shared libtorch CUDA runtime ({:.0} MB)...",
                        bytes as f64 / 1_048_576.0
                    );
                    state.download_progress = 5.0 + ((bytes as f64 / expected_size) * 75.0) as f32;
                }
                post_tts_runtime_state();
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.download_message = "Extracting shared libtorch DLLs...".to_string();
        state.download_progress = 80.0;
    }
    post_tts_runtime_state();

    let stage_dir = bin_dir.join("_tts_libtorch_extract_tmp");
    let _ = std::fs::remove_dir_all(&stage_dir);
    std::fs::create_dir_all(&stage_dir)?;
    let file = std::fs::File::open(&zip_path)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|err| anyhow!("Failed to open libtorch archive: {err}"))?;
    for idx in 0..zip.len() {
        let mut entry = zip.by_index(idx)?;
        let Some(name) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        let name_str = name.to_string_lossy();
        if (name_str.contains("/lib/") || name_str.contains("\\lib\\"))
            && let Some(file_name) = name.file_name()
            && file_name.to_string_lossy().ends_with(".dll")
        {
            let output_path = stage_dir.join(file_name);
            let mut output = std::fs::File::create(&output_path)?;
            std::io::copy(&mut entry, &mut output)?;
        }
    }
    if !libtorch_required_files_present(&stage_dir) {
        let _ = std::fs::remove_dir_all(&stage_dir);
        return Err(anyhow!(
            "Extracted libtorch archive is missing required DLLs"
        ));
    }
    for entry in std::fs::read_dir(&stage_dir)? {
        let entry = entry?;
        let destination = bin_dir.join(entry.file_name());
        std::fs::rename(entry.path(), &destination).or_else(|_| {
            std::fs::copy(entry.path(), &destination)?;
            std::fs::remove_file(entry.path())
        })?;
    }
    let _ = std::fs::remove_dir_all(stage_dir);
    let _ = std::fs::remove_file(zip_path);
    Ok(())
}
