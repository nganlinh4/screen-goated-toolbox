use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const RUNTIME_MANIFEST_URL: &str = "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/sgt-runtime-bundles/sgt_vieneu_runtime.manifest.json";
const MANAGED_MANIFEST_FILE: &str = "sgt_vieneu_runtime.manifest.json";
const MIN_RUNTIME_ABI: u32 = 1;

lazy_static::lazy_static! {
    static ref LAST_VIENEU_RUNTIME_NOTICE: Mutex<Option<String>> = Mutex::new(None);
}

static VIENEU_RUNTIME_DOWNLOADING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VieneuRuntimeManifest {
    pub version: String,
    pub abi_version: u32,
    pub entrypoint: String,
    #[serde(default)]
    pub installed_size: u64,
    pub chunks: Vec<VieneuRuntimeChunk>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VieneuRuntimeChunk {
    pub filename: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

pub fn get_vieneu_runtime_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("bin")
        .join("vieneu_runtime")
}

fn manifest_path() -> PathBuf {
    get_vieneu_runtime_dir().join(MANAGED_MANIFEST_FILE)
}

pub fn current_vieneu_runtime_notice() -> Option<String> {
    LAST_VIENEU_RUNTIME_NOTICE.lock().ok()?.clone()
}

fn set_notice(message: impl Into<String>) {
    *LAST_VIENEU_RUNTIME_NOTICE.lock().unwrap() = Some(message.into());
}

fn clear_notice() {
    *LAST_VIENEU_RUNTIME_NOTICE.lock().unwrap() = None;
}

pub fn get_vieneu_runtime_entrypoint() -> Result<PathBuf> {
    if let Some(path) = local_sidecar_candidate().filter(|path| path.is_file()) {
        return Ok(path);
    }

    let direct = default_managed_entrypoint();
    if direct.is_file() {
        return Ok(direct);
    }

    match read_installed_manifest() {
        Ok(manifest) => {
            let path = get_vieneu_runtime_dir().join(manifest.entrypoint);
            path.is_file().then_some(path.clone()).ok_or_else(|| {
                anyhow!(
                    "VieNeu runtime manifest points to missing entrypoint '{}'. Expected direct entrypoint '{}'.",
                    path.display(),
                    direct.display()
                )
            })
        }
        Err(err) => Err(anyhow!(
            "VieNeu runtime is not installed. Expected '{}'. Manifest check failed: {err}",
            direct.display()
        )),
    }
}

pub fn get_vieneu_python_path() -> PathBuf {
    if let Some(entrypoint) = local_sidecar_candidate().filter(|path| path.is_file()) {
        return python_for_entrypoint(&entrypoint);
    }
    python_for_entrypoint(&default_managed_entrypoint())
}

fn python_for_entrypoint(entrypoint: &Path) -> PathBuf {
    entrypoint
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("python_runtime")
        .join(if cfg!(windows) {
            "Scripts/python.exe"
        } else {
            "bin/python"
        })
}

pub fn is_vieneu_runtime_downloading() -> bool {
    VIENEU_RUNTIME_DOWNLOADING.load(Ordering::Relaxed)
}

pub fn is_vieneu_runtime_installed_for_variant(_variant_id: &str) -> bool {
    let Ok(entrypoint) = get_vieneu_runtime_entrypoint() else {
        return false;
    };
    python_for_entrypoint(&entrypoint).is_file()
}

pub fn vieneu_runtime_installed_size() -> u64 {
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
    dir_size(&get_vieneu_runtime_dir())
}

pub fn read_installed_manifest() -> Result<VieneuRuntimeManifest> {
    let body = fs::read_to_string(manifest_path()).context("VieNeu runtime manifest is missing")?;
    let manifest: VieneuRuntimeManifest =
        serde_json::from_str(&body).context("VieNeu runtime manifest is invalid")?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn remove_vieneu_runtime() -> Result<()> {
    let dir = get_vieneu_runtime_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|err| anyhow!("Failed to remove '{}': {err}", dir.display()))?;
    }
    clear_notice();
    Ok(())
}

pub fn download_vieneu_runtime(
    stop_signal: Arc<AtomicBool>,
    use_badge: bool,
    variant_id: String,
) -> Result<()> {
    if is_vieneu_runtime_installed_for_variant(&variant_id) {
        return Ok(());
    }
    if VIENEU_RUNTIME_DOWNLOADING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        while is_vieneu_runtime_downloading() {
            if stop_signal.load(Ordering::Relaxed) {
                bail!("Download cancelled while waiting");
            }
            std::thread::sleep(std::time::Duration::from_millis(300));
        }
        return if is_vieneu_runtime_installed_for_variant(&variant_id) {
            Ok(())
        } else {
            Err(anyhow!(
                "VieNeu runtime download did not complete successfully"
            ))
        };
    }

    let result = download_vieneu_runtime_inner(&stop_signal, use_badge, &variant_id);
    VIENEU_RUNTIME_DOWNLOADING.store(false, Ordering::SeqCst);
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

fn download_vieneu_runtime_inner(
    stop_signal: &AtomicBool,
    use_badge: bool,
    variant_id: &str,
) -> Result<()> {
    let loc = locale();
    set_progress(
        loc.vieneu_runtime_downloading_title,
        loc.vieneu_runtime_fetching_manifest,
        0.0,
        use_badge,
    );

    let result = (|| {
        let manifest = fetch_manifest()?;
        validate_manifest(&manifest)?;
        let dir = get_vieneu_runtime_dir();
        let stage = dir.with_extension("download_tmp");
        let archive = stage.join("vieneu-runtime.zip");
        let _ = fs::remove_dir_all(&stage);
        fs::create_dir_all(&stage).with_context(|| {
            format!("Failed to create VieNeu staging dir '{}'", stage.display())
        })?;

        let total: u64 = manifest.chunks.iter().map(|chunk| chunk.size).sum();
        let mut downloaded_total = 0_u64;
        let mut chunk_paths = Vec::new();
        for chunk in &manifest.chunks {
            if stop_signal.load(Ordering::Relaxed) {
                bail!("Download cancelled");
            }
            let chunk_path = stage.join(&chunk.filename);
            let message = loc
                .vieneu_runtime_downloading_file_fmt
                .replace("{}", &chunk.filename);
            set_progress(
                loc.vieneu_runtime_downloading_title,
                &message,
                if total > 0 {
                    downloaded_total as f32 / total as f32 * 75.0
                } else {
                    0.0
                },
                use_badge,
            );
            download_verified_chunk(chunk, &chunk_path, stop_signal, |downloaded| {
                let progress = if total > 0 {
                    ((downloaded_total + downloaded) as f32 / total as f32) * 75.0
                } else {
                    0.0
                };
                set_progress(
                    loc.vieneu_runtime_downloading_title,
                    &message,
                    progress,
                    use_badge,
                );
            })?;
            downloaded_total = downloaded_total.saturating_add(chunk.size);
            chunk_paths.push(chunk_path);
        }

        set_progress(
            loc.vieneu_runtime_downloading_title,
            loc.vieneu_runtime_preparing_runtime,
            80.0,
            use_badge,
        );
        concatenate_chunks(&chunk_paths, &archive).with_context(|| {
            format!("Failed to assemble VieNeu archive '{}'", archive.display())
        })?;
        set_progress(
            loc.vieneu_runtime_downloading_title,
            loc.vieneu_runtime_extracting,
            88.0,
            use_badge,
        );
        extract_runtime_archive(&archive, &stage).with_context(|| {
            format!(
                "Failed to extract VieNeu archive into '{}'",
                stage.display()
            )
        })?;
        for chunk_path in &chunk_paths {
            let _ = fs::remove_file(chunk_path);
        }
        let entrypoint = stage.join(&manifest.entrypoint);
        if !entrypoint.is_file() {
            bail!(
                "VieNeu runtime archive is missing entrypoint '{}'",
                manifest.entrypoint
            );
        }
        let python = python_for_entrypoint(&entrypoint);
        if !python.is_file() {
            bail!("VieNeu runtime archive is missing bundled Python runtime");
        }

        fs::create_dir_all(dir.parent().unwrap_or_else(|| Path::new("."))).with_context(|| {
            format!(
                "Failed to create VieNeu runtime parent dir for '{}'",
                dir.display()
            )
        })?;
        fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create VieNeu runtime dir '{}'", dir.display()))?;
        install_managed_runtime(&stage, &dir, &manifest.entrypoint)?;
        fs::remove_dir_all(&stage).with_context(|| {
            format!("Failed to remove VieNeu staging dir '{}'", stage.display())
        })?;
        let manifest_path = manifest_path();
        fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?).with_context(|| {
            format!(
                "Failed to write VieNeu runtime manifest '{}'",
                manifest_path.display()
            )
        })?;
        if !is_vieneu_runtime_installed_for_variant(variant_id) {
            bail!("VieNeu runtime install is incomplete after extraction");
        }
        Ok(())
    })();

    if result.is_ok() {
        clear_progress(
            loc.vieneu_runtime_downloading_title,
            loc.vieneu_runtime_ready,
            100.0,
            use_badge,
        );
    } else {
        clear_progress(loc.vieneu_runtime_downloading_title, "", 0.0, use_badge);
    }
    result
}

fn install_managed_runtime(stage: &Path, dir: &Path, entrypoint: &str) -> Result<()> {
    let managed_root_name = entrypoint
        .split(['/', '\\'])
        .next()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("vieneu-sidecar");
    let stage_managed_root = stage.join(managed_root_name);
    let dest_managed_root = dir.join(managed_root_name);
    if !stage_managed_root.is_dir() {
        bail!(
            "VieNeu runtime archive is missing managed root '{}'",
            stage_managed_root.display()
        );
    }
    if dest_managed_root.exists() {
        fs::remove_dir_all(&dest_managed_root).with_context(|| {
            format!(
                "Failed to remove previous managed VieNeu runtime '{}'",
                dest_managed_root.display()
            )
        })?;
    }
    if fs::rename(&stage_managed_root, &dest_managed_root).is_err() {
        copy_dir_all(&stage_managed_root, &dest_managed_root).with_context(|| {
            format!(
                "Failed to copy VieNeu runtime from '{}' to '{}'",
                stage_managed_root.display(),
                dest_managed_root.display()
            )
        })?;
        fs::remove_dir_all(&stage_managed_root).with_context(|| {
            format!(
                "Failed to remove copied VieNeu staging root '{}'",
                stage_managed_root.display()
            )
        })?;
    }
    Ok(())
}

fn fetch_manifest() -> Result<VieneuRuntimeManifest> {
    let response = ureq::get(RUNTIME_MANIFEST_URL)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|err| anyhow!("Failed to fetch VieNeu runtime manifest: {err}"))?;
    let mut body = String::new();
    response
        .into_body()
        .into_reader()
        .read_to_string(&mut body)?;
    serde_json::from_str(&body)
        .map_err(|err| anyhow!("Failed to parse VieNeu runtime manifest: {err}"))
}

fn validate_manifest(manifest: &VieneuRuntimeManifest) -> Result<()> {
    if manifest.abi_version < MIN_RUNTIME_ABI {
        bail!(
            "VieNeu runtime ABI {} is older than required ABI {}",
            manifest.abi_version,
            MIN_RUNTIME_ABI
        );
    }
    if manifest.entrypoint.trim().is_empty() || manifest.entrypoint.contains("..") {
        bail!("VieNeu runtime manifest has an unsafe entrypoint");
    }
    if manifest.chunks.is_empty() {
        bail!("VieNeu runtime manifest has no downloadable chunks");
    }
    for chunk in &manifest.chunks {
        if chunk.filename.trim().is_empty()
            || chunk.filename.contains("..")
            || chunk.sha256.trim().len() != 64
            || chunk.size == 0
        {
            bail!("VieNeu runtime manifest has an invalid chunk entry");
        }
    }
    Ok(())
}

fn download_verified_chunk(
    chunk: &VieneuRuntimeChunk,
    path: &Path,
    stop_signal: &AtomicBool,
    on_progress: impl Fn(u64),
) -> Result<()> {
    crate::api::realtime_audio::model_loader::download_file_with_progress(
        &chunk.url,
        path,
        stop_signal,
        |downloaded, _total| on_progress(downloaded),
    )
    .with_context(|| {
        format!(
            "Failed to download VieNeu runtime chunk '{}' to '{}'",
            chunk.filename,
            path.display()
        )
    })?;
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to stat VieNeu runtime chunk '{}'", path.display()))?;
    if metadata.len() != chunk.size {
        let _ = fs::remove_file(path);
        bail!(
            "VieNeu runtime chunk '{}' size mismatch: expected {}, got {}",
            chunk.filename,
            chunk.size,
            metadata.len()
        );
    }
    let actual = sha256_hex(path)?;
    if actual != chunk.sha256.to_ascii_lowercase() {
        let _ = fs::remove_file(path);
        bail!(
            "VieNeu runtime chunk '{}' checksum mismatch",
            chunk.filename
        );
    }
    Ok(())
}

fn concatenate_chunks(chunks: &[PathBuf], output: &Path) -> Result<()> {
    let mut out = fs::File::create(output)
        .with_context(|| format!("Failed to create VieNeu archive '{}'", output.display()))?;
    for chunk in chunks {
        let mut input = fs::File::open(chunk)
            .with_context(|| format!("Failed to open VieNeu chunk '{}'", chunk.display()))?;
        std::io::copy(&mut input, &mut out).with_context(|| {
            format!(
                "Failed to append VieNeu chunk '{}' into '{}'",
                chunk.display(),
                output.display()
            )
        })?;
    }
    out.flush()
        .with_context(|| format!("Failed to flush VieNeu archive '{}'", output.display()))?;
    Ok(())
}

fn extract_runtime_archive(archive: &Path, stage: &Path) -> Result<()> {
    fs::create_dir_all(stage).with_context(|| {
        format!(
            "Failed to create VieNeu extraction dir '{}'",
            stage.display()
        )
    })?;
    let tar = if cfg!(windows) { "tar.exe" } else { "tar" };
    let output = Command::new(tar)
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(stage)
        .output()
        .with_context(|| format!("Failed to start {tar} for '{}'", archive.display()))?;
    if !output.status.success() {
        bail!(
            "{tar} failed to extract VieNeu runtime archive '{}' into '{}' (status: {}). stdout: {} stderr: {}",
            archive.display(),
            stage.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let _ = fs::remove_file(archive);
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

fn locale() -> crate::gui::locale::LocaleText {
    let ui_language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    crate::gui::locale::LocaleText::get(&ui_language)
}

fn set_progress(title: &str, message: &str, progress: f32, use_badge: bool) {
    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = title.to_string();
        state.download_message = message.to_string();
        state.download_progress = progress;
    }
    if use_badge {
        crate::overlay::auto_copy_badge::show_progress_notification(title, message, progress);
    }
    post_download_state();
}

fn clear_progress(title: &str, message: &str, progress: f32, use_badge: bool) {
    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
        state.download_title = title.to_string();
        state.download_message = message.to_string();
        state.download_progress = progress;
    }
    if use_badge {
        crate::overlay::auto_copy_badge::hide_progress_notification();
    }
    post_download_state();
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

fn local_sidecar_candidate() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        for base in [
            dir.join("native")
                .join("vieneu_runtime")
                .join("dist")
                .join("vieneu-sidecar"),
            dir.join("native")
                .join("vieneu_runtime")
                .join("build")
                .join("package")
                .join("vieneu-sidecar"),
        ] {
            let candidate = base.join("vieneu_sidecar.py");
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
    get_vieneu_runtime_dir()
        .join("vieneu-sidecar")
        .join("vieneu_sidecar.py")
}
