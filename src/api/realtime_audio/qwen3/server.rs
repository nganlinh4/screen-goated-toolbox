use crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS;
use crate::api::realtime_audio::model_loader::download_file;
use anyhow::{Result, anyhow};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

const SGT_RELEASES_API_URL: &str =
    "https://api.github.com/repos/nganlinh4/screen-goated-toolbox/releases?per_page=8&prerelease=false";
const QWEN3_SERVER_ASSET_NAME: &str = "qwen3-asr-reference-windows-x64.zip";

lazy_static::lazy_static! {
    static ref LAST_QWEN3_SERVER_NOTICE: Mutex<Option<String>> = Mutex::new(None);
}

#[derive(Deserialize)]
struct GitHubRelease {
    assets: Vec<GitHubReleaseAsset>,
}

#[derive(Deserialize)]
struct GitHubReleaseAsset {
    name: String,
    browser_download_url: String,
}

fn set_qwen3_server_notice(message: impl Into<String>) {
    *LAST_QWEN3_SERVER_NOTICE.lock().unwrap() = Some(message.into());
}

fn clear_qwen3_server_notice() {
    *LAST_QWEN3_SERVER_NOTICE.lock().unwrap() = None;
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

pub fn current_qwen3_server_notice() -> Option<String> {
    LAST_QWEN3_SERVER_NOTICE.lock().unwrap().clone()
}

pub fn get_qwen3_server_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("screen-goated-toolbox")
        .join("bin")
        .join("qwen3_asr_reference")
}

pub fn get_qwen3_server_path() -> PathBuf {
    get_qwen3_server_dir().join("asr-server.exe")
}

pub fn get_qwen3_server_runtime_dir() -> PathBuf {
    get_qwen3_server_dir().join("libtorch").join("lib")
}

fn cache_runtime_root(cache_dir: &Path, name: &str) -> Option<PathBuf> {
    let variant_dir = cache_dir.join(format!("libtorch-{name}"));
    let nested_root = variant_dir.join("libtorch");
    if nested_root.join("lib").exists() {
        return Some(nested_root);
    }
    if variant_dir.join("lib").exists() {
        return Some(variant_dir);
    }
    None
}

pub fn get_local_qwen3_cached_runtime_dir() -> Option<PathBuf> {
    let repo_root = repo_root().ok()?;
    let cache_dir = repo_root.join("tools").join("qwen3-reference-cache");
    let variant = fs::read_to_string(cache_dir.join("runtime-variant.txt"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let candidates = variant
        .into_iter()
        .filter_map(|name| cache_runtime_root(&cache_dir, &name).map(|root| root.join("lib")))
        .chain(std::iter::once(cache_dir.join("libtorch").join("lib")));
    candidates.into_iter().find(|path| path.exists())
}

pub fn is_qwen3_server_downloaded() -> bool {
    get_active_qwen3_server_path().is_some()
}

pub fn is_qwen3_server_managed() -> bool {
    get_qwen3_server_path().exists()
}

pub fn get_active_qwen3_server_path() -> Option<PathBuf> {
    local_sidecar_candidate_paths()
        .into_iter()
        .find(|path| path.exists())
        .or_else(|| get_qwen3_server_path().exists().then(get_qwen3_server_path))
}

pub fn get_active_qwen3_server_root() -> Option<PathBuf> {
    get_active_qwen3_server_path()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

pub fn remove_qwen3_server() -> Result<()> {
    let dir = get_qwen3_server_dir();
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|err| anyhow!("Failed to remove '{}': {}", dir.display(), err))?;
    }
    clear_qwen3_server_notice();
    Ok(())
}

pub fn download_qwen3_server(stop_signal: Arc<AtomicBool>, use_badge: bool) -> Result<()> {
    if is_qwen3_server_downloaded() {
        return Ok(());
    }

    let locale = qwen3_locale();
    use crate::overlay::realtime_webview::state::REALTIME_STATE;
    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = true;
        state.download_title = locale.qwen3_server_downloading_title.to_string();
        state.download_message = locale.qwen3_server_downloading_message.to_string();
        state.download_progress = 0.0;
    }
    clear_qwen3_server_notice();

    if use_badge {
        crate::overlay::auto_copy_badge::show_progress_notification(
            locale.qwen3_server_downloading_title,
            locale.qwen3_server_downloading_message,
            0.0,
        );
    }

    post_download_state();

    let result: Result<()> = (|| {
        let release = find_release_with_asset()?;
        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == QWEN3_SERVER_ASSET_NAME)
            .ok_or_else(|| {
                anyhow!(
                    "Latest Screen Goated Toolbox releases do not contain the '{}' sidecar asset yet.",
                    QWEN3_SERVER_ASSET_NAME
                )
            })?;
        if let Ok(mut state) = REALTIME_STATE.lock() {
            state.download_message = locale
                .qwen3_server_downloading_file
                .replace("{}", &asset.name);
        }
        post_download_state();

        let archive_path = get_qwen3_server_dir().join(&asset.name);
        download_file(
            &asset.browser_download_url,
            &archive_path,
            &stop_signal,
            use_badge,
        )?;

        extract_qwen3_server_archive(&archive_path, &get_qwen3_server_dir())?;
        let _ = fs::remove_file(archive_path);

        if !is_qwen3_server_downloaded() {
            return Err(anyhow!(
                "Downloaded Qwen3 reference server bundle did not contain asr-server.exe"
            ));
        }

        Ok(())
    })();

    if let Ok(mut state) = REALTIME_STATE.lock() {
        state.is_downloading = false;
    }
    if use_badge {
        crate::overlay::auto_copy_badge::hide_progress_notification();
    }
    post_download_state();

    if let Err(err) = &result {
        if !err.to_string().contains("cancelled") {
            set_qwen3_server_notice(err.to_string());
        }
    } else {
        clear_qwen3_server_notice();
    }

    result
}

pub fn local_sidecar_candidate_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(repo_root) = repo_root() {
        candidates.push(
            repo_root
                .join("third_party")
                .join("qwen3-asr-rs")
                .join("target")
                .join("release")
                .join("asr-server.exe"),
        );
        candidates.push(
            repo_root
                .join("dist")
                .join(QWEN3_SERVER_ASSET_NAME.trim_end_matches(".zip"))
                .join("asr-server.exe"),
        );
    }

    candidates
}

fn repo_root() -> Result<PathBuf> {
    let mut seeds = Vec::new();
    if let Ok(dir) = std::env::current_dir() {
        seeds.push(dir);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        seeds.push(parent.to_path_buf());
    }

    for seed in seeds {
        let mut dir = seed;
        loop {
            if dir.join("Cargo.toml").exists() && dir.join(".claude").exists() {
                return Ok(dir);
            }
            if !dir.pop() {
                break;
            }
        }
    }

    Err(anyhow!("Could not locate Screen Goated Toolbox repository root"))
}

fn find_release_with_asset() -> Result<GitHubRelease> {
    let releases = fetch_releases()?;
    releases
        .into_iter()
        .find(|release| release.assets.iter().any(|asset| asset.name == QWEN3_SERVER_ASSET_NAME))
        .ok_or_else(|| {
            anyhow!(
                "No Screen Goated Toolbox release currently publishes '{}'. Build it from 'third_party/qwen3-asr-rs' with scripts/build_qwen3_reference_sidecar.ps1 and upload the bundle to a repo release.",
                QWEN3_SERVER_ASSET_NAME
            )
        })
}

fn fetch_releases() -> Result<Vec<GitHubRelease>> {
    ureq::get(SGT_RELEASES_API_URL)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .map_err(|err| anyhow!("Failed to fetch Screen Goated Toolbox release metadata: {err}"))?
        .into_body()
        .read_json::<Vec<GitHubRelease>>()
        .map_err(|err| anyhow!("Failed to parse Screen Goated Toolbox release metadata: {err}"))
}

fn extract_qwen3_server_archive(archive_path: &Path, output_dir: &Path) -> Result<()> {
    if archive_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        fs::create_dir_all(output_dir)?;
        fs::copy(archive_path, get_qwen3_server_path())?;
        return Ok(());
    }

    let file = fs::File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|err| anyhow!("Failed to open Qwen3 reference server archive: {err}"))?;
    fs::create_dir_all(output_dir)?;

    for idx in 0..archive.len() {
        let mut entry = archive
            .by_index(idx)
            .map_err(|err| anyhow!("Failed to read Qwen3 reference server archive entry: {err}"))?;
        let relative_path = match entry.enclosed_name() {
            Some(path) => path.to_path_buf(),
            None => continue,
        };
        let output_path = output_dir.join(relative_path);

        if entry.is_dir() {
            fs::create_dir_all(&output_path)?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut output = fs::File::create(&output_path)?;
        std::io::copy(&mut entry, &mut output)?;
    }

    Ok(())
}
