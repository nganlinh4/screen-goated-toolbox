use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use crate::api::realtime_audio::model_loader::download_file;
use anyhow::{Result, anyhow};
use serde::Deserialize;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const QWEN3_REPO_TREE_URL: &str =
    "https://huggingface.co/api/models/Qwen/Qwen3-ASR-0.6B/tree/main?recursive=1";
const QWEN3_REPO_RESOLVE_BASE: &str = "https://huggingface.co/Qwen/Qwen3-ASR-0.6B/resolve/main";

const QWEN3_1_7B_REPO_TREE_URL: &str =
    "https://huggingface.co/api/models/Qwen/Qwen3-ASR-1.7B/tree/main?recursive=1";
const QWEN3_1_7B_REPO_RESOLVE_BASE: &str =
    "https://huggingface.co/Qwen/Qwen3-ASR-1.7B/resolve/main";

static LAST_QWEN3_MODEL_ACTION_ERROR: LazyLock<Mutex<Option<String>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Deserialize)]
struct HuggingFaceTreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    oid: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    lfs: Option<HuggingFaceLfsEntry>,
}

#[derive(Deserialize)]
struct HuggingFaceLfsEntry {
    oid: String,
    size: u64,
}

#[derive(Clone, Copy)]
enum RepoHashKind {
    Sha256,
    GitBlobSha1,
}

struct RepoFileSpec {
    path: String,
    size: u64,
    hash_kind: RepoHashKind,
    expected_hash: String,
}

fn set_qwen3_model_action_error(message: impl Into<String>) {
    *LAST_QWEN3_MODEL_ACTION_ERROR.lock().unwrap() = Some(message.into());
}

fn clear_qwen3_model_action_error() {
    *LAST_QWEN3_MODEL_ACTION_ERROR.lock().unwrap() = None;
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

fn qwen3_locale() -> crate::gui::locale::LocaleText {
    let app = crate::APP.lock().unwrap();
    crate::gui::locale::LocaleText::get(&app.config.ui_language)
}

fn wanted_huggingface_file(path: &str) -> bool {
    path.ends_with(".safetensors")
        || matches!(
            path,
            "config.json"
                | "generation_config.json"
                | "preprocessor_config.json"
                | "processor_config.json"
                | "special_tokens_map.json"
                | "tokenizer.json"
                | "tokenizer_config.json"
                | "model.safetensors.index.json"
                | "merges.txt"
                | "vocab.json"
                | "added_tokens.json"
        )
}

fn list_repo_files(tree_url: &str) -> Result<Vec<RepoFileSpec>> {
    let response = ureq::get(tree_url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|err| anyhow!("Failed to fetch Qwen3-ASR manifest: {err}"))?;
    let mut reader = response.into_body().into_reader();
    let mut body = String::new();
    reader.read_to_string(&mut body)?;

    let mut files: Vec<RepoFileSpec> = serde_json::from_str::<Vec<HuggingFaceTreeEntry>>(&body)?
        .into_iter()
        .filter(|entry| entry.kind == "file" && wanted_huggingface_file(&entry.path))
        .map(repo_file_spec_from_entry)
        .collect::<Result<Vec<_>>>()?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    if files.is_empty() {
        return Err(anyhow!(
            "Qwen3-ASR manifest did not contain any downloadable model files"
        ));
    }
    Ok(files)
}

fn repo_file_spec_from_entry(entry: HuggingFaceTreeEntry) -> Result<RepoFileSpec> {
    if let Some(lfs) = entry.lfs {
        return Ok(RepoFileSpec {
            path: entry.path,
            size: lfs.size,
            hash_kind: RepoHashKind::Sha256,
            expected_hash: lfs.oid.to_ascii_lowercase(),
        });
    }
    if entry.oid.trim().is_empty() {
        return Err(anyhow!(
            "Qwen3-ASR manifest entry '{}' did not include an oid",
            entry.path
        ));
    }
    Ok(RepoFileSpec {
        path: entry.path,
        size: entry.size,
        hash_kind: RepoHashKind::GitBlobSha1,
        expected_hash: entry.oid.to_ascii_lowercase(),
    })
}

fn verify_repo_file(path: &Path, spec: &RepoFileSpec) -> Result<()> {
    let metadata = fs::metadata(path)
        .map_err(|err| anyhow!("Failed to inspect '{}': {err}", path.display()))?;
    if !metadata.is_file() {
        return Err(anyhow!("'{}' is not a file", path.display()));
    }
    if spec.size > 0 && metadata.len() != spec.size {
        return Err(anyhow!(
            "Size mismatch for '{}': expected {} bytes, got {} bytes",
            path.display(),
            spec.size,
            metadata.len()
        ));
    }

    let actual_hash = match spec.hash_kind {
        RepoHashKind::Sha256 => sha256_hex(path)?,
        RepoHashKind::GitBlobSha1 => git_blob_sha1_hex(path, metadata.len())?,
    };
    if actual_hash != spec.expected_hash {
        return Err(anyhow!(
            "Checksum mismatch for '{}': expected {}, got {}",
            path.display(),
            spec.expected_hash,
            actual_hash
        ));
    }
    Ok(())
}

fn ensure_repo_file_downloaded(
    url: &str,
    path: &Path,
    spec: &RepoFileSpec,
    stop_signal: &AtomicBool,
    use_badge: bool,
) -> Result<()> {
    if path.exists() {
        match verify_repo_file(path, spec) {
            Ok(()) => return Ok(()),
            Err(_) => {
                let _ = fs::remove_file(path);
            }
        }
    }

    download_file(url, path, stop_signal, use_badge)?;
    if let Err(err) = verify_repo_file(path, spec) {
        let _ = fs::remove_file(path);
        return Err(err);
    }
    Ok(())
}

fn sha256_hex(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)
        .map_err(|err| anyhow!("Failed to open '{}' for hashing: {err}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn git_blob_sha1_hex(path: &Path, size: u64) -> Result<String> {
    let mut file = fs::File::open(path)
        .map_err(|err| anyhow!("Failed to open '{}' for hashing: {err}", path.display()))?;
    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", size).as_bytes());
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn has_nonempty_file(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.len() > 0)
        .unwrap_or(false)
}

fn has_nonempty_safetensors(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "safetensors")
                && entry
                    .metadata()
                    .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
        })
    })
}

fn qwen3_model_files_present(dir: &Path) -> bool {
    has_nonempty_file(&dir.join("config.json"))
        && has_nonempty_file(&dir.join("vocab.json"))
        && has_nonempty_file(&dir.join("merges.txt"))
        && has_nonempty_file(&dir.join("tokenizer_config.json"))
        && has_nonempty_safetensors(dir)
}

pub fn current_qwen3_model_notice() -> Option<String> {
    LAST_QWEN3_MODEL_ACTION_ERROR.lock().unwrap().clone()
}

pub fn get_qwen3_model_dir() -> PathBuf {
    crate::paths::app_models_dir().join("qwen3_asr_0_6b")
}

pub fn get_qwen3_1_7b_model_dir() -> PathBuf {
    crate::paths::app_models_dir().join("qwen3_asr_1_7b")
}

pub fn is_qwen3_1_7b_model_downloaded() -> bool {
    let dir = get_qwen3_1_7b_model_dir();
    qwen3_model_files_present(&dir)
}

pub fn download_qwen3_1_7b_model(stop_signal: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let dir = get_qwen3_1_7b_model_dir();
    let locale = qwen3_locale();

    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = locale.qwen3_1_7b_downloading_title.to_string();
        state.download_message = locale.qwen3_downloading_message.to_string();
        state.download_progress = 0.0;
    }
    clear_qwen3_model_action_error();

    post_download_state();

    let result: Result<()> = (|| {
        let files = list_repo_files(QWEN3_1_7B_REPO_TREE_URL)?;
        for file in files {
            if stop_signal.load(Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled"));
            }

            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = locale.qwen3_downloading_file.replace("{}", &file.path);
            }
            post_download_state();

            let url = format!("{QWEN3_1_7B_REPO_RESOLVE_BASE}/{}", file.path);
            ensure_repo_file_downloaded(
                &url,
                &dir.join(&file.path),
                &file,
                &stop_signal,
                use_badge,
            )?;
        }
        Ok(())
    })();

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    post_download_state();

    if let Err(err) = &result {
        if !err.to_string().contains("cancelled") {
            set_qwen3_model_action_error(err.to_string());
        }
    } else {
        clear_qwen3_model_action_error();
    }

    result
}

pub fn is_qwen3_model_downloaded() -> bool {
    let dir = get_qwen3_model_dir();
    qwen3_model_files_present(&dir)
}

pub fn remove_qwen3_model() -> Result<()> {
    let dir = get_qwen3_model_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|err| anyhow!("Failed to remove '{}': {}", dir.display(), err))?;
    }
    clear_qwen3_model_action_error();
    Ok(())
}

pub fn remove_qwen3_1_7b_model() -> Result<()> {
    let dir = get_qwen3_1_7b_model_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|err| anyhow!("Failed to remove '{}': {}", dir.display(), err))?;
    }
    clear_qwen3_model_action_error();
    Ok(())
}

pub fn download_qwen3_model(stop_signal: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    let dir = get_qwen3_model_dir();
    let locale = qwen3_locale();

    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = locale.qwen3_downloading_title.to_string();
        state.download_message = locale.qwen3_downloading_message.to_string();
        state.download_progress = 0.0;
    }
    clear_qwen3_model_action_error();

    post_download_state();

    let result: Result<()> = (|| {
        let files = list_repo_files(QWEN3_REPO_TREE_URL)?;
        for file in files {
            if stop_signal.load(Ordering::Relaxed) {
                return Err(anyhow!("Download cancelled"));
            }

            if let Ok(mut state) = REALTIME_STATE.lock() {
                state.download_message = locale.qwen3_downloading_file.replace("{}", &file.path);
            }
            post_download_state();

            let url = format!("{QWEN3_REPO_RESOLVE_BASE}/{}", file.path);
            ensure_repo_file_downloaded(
                &url,
                &dir.join(&file.path),
                &file,
                &stop_signal,
                use_badge,
            )?;
        }

        Ok(())
    })();

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    post_download_state();

    if let Err(err) = &result {
        if !err.to_string().contains("cancelled") {
            set_qwen3_model_action_error(err.to_string());
        }
    } else {
        clear_qwen3_model_action_error();
    }

    result
}
