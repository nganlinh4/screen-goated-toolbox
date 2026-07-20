use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const RUNTIME_MANIFEST_URL: &str = "https://raw.githubusercontent.com/nganlinh4/screen-goated-toolbox/main/native/step_audio_runtime/dist/sgt_step_audio_runtime.manifest.json";
const MANAGED_MANIFEST_FILE: &str = "sgt_step_audio_runtime.manifest.json";
const MIN_RUNTIME_ABI: u32 = 1;

static LAST_STEP_AUDIO_RUNTIME_NOTICE: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

static STEP_AUDIO_RUNTIME_DOWNLOADING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepAudioRuntimeManifest {
    pub version: String,
    pub abi_version: u32,
    pub entrypoint: String,
    #[serde(default)]
    pub installed_size: u64,
    pub chunks: Vec<StepAudioRuntimeChunk>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepAudioRuntimeChunk {
    pub filename: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

fn set_notice(message: impl Into<String>) {
    *LAST_STEP_AUDIO_RUNTIME_NOTICE.lock().unwrap() = Some(message.into());
}

fn clear_notice() {
    *LAST_STEP_AUDIO_RUNTIME_NOTICE.lock().unwrap() = None;
}

pub fn current_step_audio_runtime_notice() -> Option<String> {
    LAST_STEP_AUDIO_RUNTIME_NOTICE.lock().ok()?.clone()
}

pub fn is_step_audio_runtime_downloading() -> bool {
    STEP_AUDIO_RUNTIME_DOWNLOADING.load(Ordering::Relaxed)
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
    let ui_language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&ui_language)
}

pub fn get_step_audio_runtime_dir() -> PathBuf {
    crate::paths::app_local_data_dir()
        .join("bin")
        .join("step_audio_runtime")
}

fn manifest_path() -> PathBuf {
    get_step_audio_runtime_dir().join(MANAGED_MANIFEST_FILE)
}

pub fn step_audio_runtime_installed_size() -> u64 {
    fn dir_size(path: &Path) -> u64 {
        let Ok(entries) = fs::read_dir(path) else {
            return 0;
        };
        entries
            .flatten()
            .map(|entry| {
                let path = entry.path();
                entry
                    .metadata()
                    .map(|metadata| {
                        if metadata.is_dir() {
                            dir_size(&path)
                        } else {
                            metadata.len()
                        }
                    })
                    .unwrap_or(0)
            })
            .sum()
    }
    dir_size(&get_step_audio_runtime_dir())
}

pub fn read_step_audio_installed_manifest() -> Result<StepAudioRuntimeManifest> {
    let body =
        fs::read_to_string(manifest_path()).context("Step Audio runtime manifest is missing")?;
    let manifest: StepAudioRuntimeManifest =
        serde_json::from_str(&body).context("Step Audio runtime manifest is invalid")?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn get_step_audio_runtime_entrypoint() -> Result<PathBuf> {
    if let Some(path) = local_sidecar_candidate().filter(|path| path.is_file()) {
        return Ok(path);
    }

    let direct = default_managed_entrypoint();
    if direct.is_file() {
        return Ok(direct);
    }

    match read_step_audio_installed_manifest() {
        Ok(manifest) => {
            let path = get_step_audio_runtime_dir().join(manifest.entrypoint);
            if path.is_file() {
                Ok(path)
            } else {
                Err(anyhow!(
                    "Step Audio runtime manifest points to missing entrypoint '{}'. Expected direct entrypoint '{}'.",
                    path.display(),
                    direct.display()
                ))
            }
        }
        Err(err) => Err(anyhow!(
            "Step Audio runtime is not installed. Expected '{}'. Manifest check failed: {err}",
            direct.display()
        )),
    }
}

pub fn is_step_audio_runtime_installed() -> bool {
    get_step_audio_runtime_entrypoint().is_ok()
}

pub fn remove_step_audio_runtime() -> Result<()> {
    let dir = get_step_audio_runtime_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|err| anyhow!("Failed to remove '{}': {err}", dir.display()))?;
    }
    clear_notice();
    Ok(())
}

pub fn download_step_audio_runtime(stop_signal: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    if is_step_audio_runtime_installed() {
        return Ok(());
    }
    if STEP_AUDIO_RUNTIME_DOWNLOADING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        while is_step_audio_runtime_downloading() {
            if stop_signal.load(Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled while waiting"));
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        return if is_step_audio_runtime_installed() {
            Ok(())
        } else {
            Err(anyhow!(
                "Step Audio runtime download did not complete successfully"
            ))
        };
    }

    let result = download_step_audio_runtime_inner(&stop_signal, use_badge);
    STEP_AUDIO_RUNTIME_DOWNLOADING.store(false, Ordering::SeqCst);
    post_download_state();
    if let Err(err) = &result {
        if !err.to_string().contains("cancelled") {
            set_notice(err.to_string());
        }
    } else {
        clear_notice();
    }
    result
}

fn download_step_audio_runtime_inner(stop_signal: &AtomicBool, use_badge: bool) -> Result<()> {
    use crate::overlay::realtime_webview::state::REALTIME_STATE;

    let loc = locale();
    let download_title = crate::overlay::auto_copy_badge::format_locale(
        loc.badge.downloading_runtime_fmt,
        &[("name", "Step Audio")],
    );
    let preparing = crate::overlay::auto_copy_badge::format_locale(
        loc.badge.preparing_runtime_fmt,
        &[("name", "Step Audio")],
    );
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = download_title.clone();
        state.download_message = loc.badge.fetching_runtime_manifest.to_string();
        state.download_progress = 0.0;
    }
    post_download_state();
    if use_badge {
        crate::overlay::auto_copy_badge::show_progress_notification(
            &download_title,
            loc.badge.fetching_runtime_manifest,
            0.0,
        );
    }

    let result = (|| {
        let manifest = fetch_manifest()?;
        validate_manifest(&manifest)?;
        let dir = get_step_audio_runtime_dir();
        let stage = dir.with_extension("download_tmp");
        let archive = stage.join("step-audio-runtime.zip");
        let _ = fs::remove_dir_all(&stage);
        fs::create_dir_all(&stage)?;

        let total: u64 = manifest.chunks.iter().map(|chunk| chunk.size).sum();
        let mut downloaded_total = 0_u64;
        let mut chunk_paths = Vec::new();
        for chunk in &manifest.chunks {
            if stop_signal.load(Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled"));
            }
            let chunk_path = stage.join(&chunk.filename);
            let downloading_file = crate::overlay::auto_copy_badge::format_locale(
                loc.badge.downloading_file_fmt,
                &[("name", &chunk.filename)],
            );
            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = downloading_file.clone();
            }
            post_download_state();
            download_verified_chunk(chunk, &chunk_path, stop_signal, |downloaded| {
                let progress = if total > 0 {
                    ((downloaded_total + downloaded) as f32 / total as f32) * 75.0
                } else {
                    0.0
                };
                if let Ok(mut state) = REALTIME_STATE.lock() {
                    state.download_progress = progress;
                }
                if use_badge {
                    crate::overlay::auto_copy_badge::show_progress_notification(
                        &download_title,
                        &downloading_file,
                        progress,
                    );
                }
                post_download_state();
            })?;
            downloaded_total = downloaded_total.saturating_add(chunk.size);
            chunk_paths.push(chunk_path);
        }

        if let Ok(mut state) = REALTIME_STATE.lock() {
            state.download_message = preparing.clone();
            state.download_progress = 80.0;
        }
        post_download_state();
        concatenate_chunks(&chunk_paths, &archive)?;
        extract_runtime_archive(&archive, &stage)?;
        for chunk_path in &chunk_paths {
            let _ = fs::remove_file(chunk_path);
        }
        let entrypoint = stage.join(&manifest.entrypoint);
        if !entrypoint.is_file() {
            bail!(
                "Step Audio runtime archive is missing entrypoint '{}'",
                manifest.entrypoint
            );
        }

        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.parent().unwrap_or_else(|| Path::new(".")))?;
        if fs::rename(&stage, &dir).is_err() {
            copy_dir_all(&stage, &dir)?;
            fs::remove_dir_all(&stage)?;
        }
        fs::write(manifest_path(), serde_json::to_vec_pretty(&manifest)?)?;
        if !is_step_audio_runtime_installed() {
            bail!("Step Audio runtime install is incomplete after extraction");
        }
        Ok(())
    })();

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
        state.download_progress = if result.is_ok() { 100.0 } else { 0.0 };
        if result.is_err() {
            state.download_message = loc.tool_runtime.step_audio_downloading_message.to_string();
        }
    }
    if use_badge {
        crate::overlay::auto_copy_badge::hide_progress_notification();
    }
    result
}

fn fetch_manifest() -> Result<StepAudioRuntimeManifest> {
    let response = ureq::get(RUNTIME_MANIFEST_URL)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|err| anyhow!("Failed to fetch Step Audio runtime manifest: {err}"))?;
    let mut body = String::new();
    response
        .into_body()
        .into_reader()
        .read_to_string(&mut body)?;
    serde_json::from_str(&body)
        .map_err(|err| anyhow!("Failed to parse Step Audio runtime manifest: {err}"))
}

fn validate_manifest(manifest: &StepAudioRuntimeManifest) -> Result<()> {
    if manifest.abi_version < MIN_RUNTIME_ABI {
        bail!(
            "Step Audio runtime ABI {} is older than required ABI {}",
            manifest.abi_version,
            MIN_RUNTIME_ABI
        );
    }
    if manifest.entrypoint.trim().is_empty() || manifest.entrypoint.contains("..") {
        bail!("Step Audio runtime manifest has an unsafe entrypoint");
    }
    if manifest.chunks.is_empty() {
        bail!("Step Audio runtime manifest has no downloadable chunks");
    }
    for chunk in &manifest.chunks {
        if chunk.filename.trim().is_empty()
            || chunk.filename.contains("..")
            || chunk.sha256.trim().len() != 64
            || chunk.size == 0
        {
            bail!("Step Audio runtime manifest has an invalid chunk entry");
        }
    }
    Ok(())
}

fn download_verified_chunk(
    chunk: &StepAudioRuntimeChunk,
    path: &Path,
    stop_signal: &AtomicBool,
    on_progress: impl Fn(u64),
) -> Result<()> {
    crate::api::realtime_audio::model_loader::download_file_with_progress(
        &chunk.url,
        path,
        stop_signal,
        |downloaded, _total| on_progress(downloaded),
    )?;
    let metadata = fs::metadata(path)?;
    if metadata.len() != chunk.size {
        let _ = fs::remove_file(path);
        bail!(
            "Step Audio runtime chunk '{}' size mismatch: expected {}, got {}",
            chunk.filename,
            chunk.size,
            metadata.len()
        );
    }
    let actual = sha256_hex(path)?;
    if actual != chunk.sha256.to_ascii_lowercase() {
        let _ = fs::remove_file(path);
        bail!(
            "Step Audio runtime chunk '{}' checksum mismatch",
            chunk.filename
        );
    }
    Ok(())
}

fn concatenate_chunks(chunks: &[PathBuf], output: &Path) -> Result<()> {
    let mut out = fs::File::create(output)?;
    for chunk in chunks {
        let mut input = fs::File::open(chunk)?;
        std::io::copy(&mut input, &mut out)?;
    }
    out.flush()?;
    Ok(())
}

fn extract_runtime_archive(archive: &Path, stage: &Path) -> Result<()> {
    let file = fs::File::open(archive)?;
    let mut zip =
        zip::ZipArchive::new(file).context("Failed to open Step Audio runtime archive")?;
    for idx in 0..zip.len() {
        let mut entry = zip.by_index(idx)?;
        let Some(name) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        let output = stage.join(name);
        if entry.is_dir() {
            fs::create_dir_all(&output)?;
        } else {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = fs::File::create(&output)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    let _ = fs::remove_file(archive);
    for chunk in fs::read_dir(stage)?.flatten() {
        if chunk
            .path()
            .extension()
            .is_some_and(|extension| extension == "part")
        {
            let _ = fs::remove_file(chunk.path());
        }
    }
    Ok(())
}

fn copy_dir_all(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if entry.metadata()?.is_dir() {
            copy_dir_all(&src, &dst)?;
        } else {
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

fn sha256_hex(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn local_sidecar_candidate() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        for base in [
            dir.join("native")
                .join("step_audio_runtime")
                .join("dist")
                .join("step-audio-sidecar"),
            dir.join("native")
                .join("step_audio_runtime")
                .join("build")
                .join("package")
                .join("step-audio-sidecar"),
        ] {
            let candidate = base.join(sidecar_exe_name());
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn default_managed_entrypoint() -> PathBuf {
    get_step_audio_runtime_dir()
        .join("step-audio-sidecar")
        .join(sidecar_exe_name())
}

fn sidecar_exe_name() -> &'static str {
    if cfg!(windows) {
        "step-audio-sidecar.exe"
    } else {
        "step-audio-sidecar"
    }
}
