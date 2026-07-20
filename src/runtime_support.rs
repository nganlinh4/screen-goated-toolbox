use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Result, anyhow};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::GetCurrentProcess;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeArch {
    X64,
    Arm64,
}

impl RuntimeArch {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X64 => "x64",
            Self::Arm64 => "arm64",
        }
    }
}

impl fmt::Display for RuntimeArch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityStatus {
    Supported,
    MissingDependency,
    UnsupportedPlatform,
    UnsupportedHardware,
}

#[derive(Clone, Debug)]
pub struct FeatureCapability {
    pub status: CapabilityStatus,
    pub title: String,
    pub details: String,
}

impl FeatureCapability {
    pub fn supported() -> Self {
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

#[derive(Clone, Debug)]
pub struct EnvironmentInfo {
    pub process_arch: RuntimeArch,
    pub native_arch: RuntimeArch,
}

#[derive(Clone, Debug)]
pub enum WebView2InstallStatus {
    Installed,
    Installing,
    Missing,
    Error(String),
}

const WEBVIEW2_BOOTSTRAPPER_URL: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";
const WEBVIEW2_BOOTSTRAPPER_NAME: &str = "MicrosoftEdgeWebview2Setup.exe";

static WEBVIEW2_STATUS: LazyLock<Mutex<WebView2InstallStatus>> =
    LazyLock::new(|| Mutex::new(WebView2InstallStatus::Missing));
static STARTUP_NOTICE_SHOWN: LazyLock<Mutex<bool>> = LazyLock::new(|| Mutex::new(false));

pub fn current_process_arch() -> RuntimeArch {
    #[cfg(target_arch = "x86_64")]
    {
        RuntimeArch::X64
    }
    #[cfg(target_arch = "aarch64")]
    {
        RuntimeArch::Arm64
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        // Windows-only app — the supported targets are x86_64 and aarch64.
        // Default to X64 on anything else just to keep the type total.
        RuntimeArch::X64
    }
}

pub fn environment_info() -> EnvironmentInfo {
    let process_arch = current_process_arch();
    let native_arch = detect_native_arch().unwrap_or(process_arch);
    EnvironmentInfo {
        process_arch,
        native_arch,
    }
}

pub fn supports_qwen3_local_runtime() -> FeatureCapability {
    let env = environment_info();
    let badge = crate::overlay::auto_copy_badge::locale_text();
    let unavailable = crate::overlay::auto_copy_badge::format_locale(
        badge.feature_unavailable_fmt,
        &[("name", "Qwen3-ASR CUDA Runtime")],
    );
    if env.process_arch != RuntimeArch::X64 {
        return FeatureCapability {
            status: CapabilityStatus::UnsupportedPlatform,
            title: unavailable,
            details: badge.qwen_x64_only.to_string(),
        };
    }
    if env.native_arch == RuntimeArch::Arm64 {
        return FeatureCapability {
            status: CapabilityStatus::UnsupportedHardware,
            title: unavailable,
            details: badge.qwen_arm_unsupported.to_string(),
        };
    }

    FeatureCapability::supported()
}

pub fn require_webview2(feature_name: &str) -> FeatureCapability {
    if webview2_runtime_installed() {
        FeatureCapability::supported()
    } else {
        let badge = crate::overlay::auto_copy_badge::locale_text();
        let feature_name = match feature_name {
            "Window selector" => badge.feature_window_selector,
            "Realtime overlay" => badge.feature_realtime_overlay,
            "Preset wheel" => badge.feature_preset_wheel,
            "TTS Playground" => badge.feature_tts_playground,
            "Text input overlay" => badge.feature_text_input,
            "Screen record" => badge.feature_screen_record,
            "Markdown view" => badge.feature_markdown_view,
            name => name,
        };
        FeatureCapability {
            status: CapabilityStatus::MissingDependency,
            title: crate::overlay::auto_copy_badge::format_locale(
                badge.feature_needs_webview2_fmt,
                &[("name", feature_name)],
            ),
            details: badge.install_webview2_hint.to_string(),
        }
    }
}

pub fn notify_capability_issue(capability: &FeatureCapability) {
    if capability.is_supported() {
        return;
    }
    let notification_type = match capability.status {
        CapabilityStatus::MissingDependency => {
            crate::overlay::auto_copy_badge::NotificationType::Info
        }
        CapabilityStatus::UnsupportedPlatform | CapabilityStatus::UnsupportedHardware => {
            crate::overlay::auto_copy_badge::NotificationType::Error
        }
        CapabilityStatus::Supported => crate::overlay::auto_copy_badge::NotificationType::Success,
    };
    crate::overlay::auto_copy_badge::show_detailed_notification(
        &capability.title,
        &capability.details,
        notification_type,
    );
}

pub fn unsupported_feature_names() -> Vec<&'static str> {
    let mut unsupported = Vec::new();

    if !supports_qwen3_local_runtime().is_supported() {
        unsupported.push("Qwen3 local AI");
    }

    unsupported
}

pub fn show_startup_compatibility_notice_if_needed() {
    {
        let mut shown = STARTUP_NOTICE_SHOWN.lock().unwrap();
        if *shown {
            return;
        }
        *shown = true;
    }

    let unsupported = unsupported_feature_names();
    if unsupported.is_empty() {
        return;
    }

    let arch = environment_info().native_arch.to_string();
    let unsupported = unsupported.join(", ");
    let badge = crate::overlay::auto_copy_badge::locale_text();
    let title = crate::overlay::auto_copy_badge::format_locale(
        badge.unsupported_features_fmt,
        &[("name", &unsupported), ("arch", &arch)],
    );
    crate::overlay::auto_copy_badge::show_timed_detailed_notification(
        &title,
        badge.unavailable_features_here,
        crate::overlay::auto_copy_badge::NotificationType::Info,
        2500,
    );
}

pub fn webview2_runtime_installed() -> bool {
    find_webview2_executable().is_some()
}

pub fn current_webview2_status() -> WebView2InstallStatus {
    let current = WEBVIEW2_STATUS.lock().unwrap().clone();
    match current {
        WebView2InstallStatus::Installing => current,
        WebView2InstallStatus::Error(message) => {
            if webview2_runtime_installed() {
                WebView2InstallStatus::Installed
            } else {
                WebView2InstallStatus::Error(message)
            }
        }
        _ if webview2_runtime_installed() => WebView2InstallStatus::Installed,
        _ => WebView2InstallStatus::Missing,
    }
}

pub fn start_webview2_runtime_install() -> bool {
    match current_webview2_status() {
        WebView2InstallStatus::Installed | WebView2InstallStatus::Installing => false,
        _ => {
            std::thread::spawn(|| {
                if let Err(err) = install_webview2_runtime() {
                    crate::log_info!("[WebView2] Install failed: {err}");
                }
            });
            true
        }
    }
}

pub fn tool_download_arch() -> RuntimeArch {
    current_process_arch()
}

#[cfg(target_os = "windows")]
fn detect_native_arch() -> Option<RuntimeArch> {
    use windows::Win32::System::SystemInformation::{
        IMAGE_FILE_MACHINE, IMAGE_FILE_MACHINE_AMD64, IMAGE_FILE_MACHINE_ARM64,
        IMAGE_FILE_MACHINE_UNKNOWN,
    };
    use windows::Win32::System::Threading::IsWow64Process2;

    let process: HANDLE = unsafe { GetCurrentProcess() };
    let mut process_machine = IMAGE_FILE_MACHINE(IMAGE_FILE_MACHINE_UNKNOWN.0);
    let mut native_machine = IMAGE_FILE_MACHINE(IMAGE_FILE_MACHINE_UNKNOWN.0);
    let ok = unsafe { IsWow64Process2(process, &mut process_machine, Some(&mut native_machine)) }
        .is_ok();
    if !ok {
        return None;
    }

    match native_machine {
        value if value == IMAGE_FILE_MACHINE_AMD64 => Some(RuntimeArch::X64),
        value if value == IMAGE_FILE_MACHINE_ARM64 => Some(RuntimeArch::Arm64),
        _ => None,
    }
}

#[cfg(not(target_os = "windows"))]
fn detect_native_arch() -> Option<RuntimeArch> {
    None
}

fn find_webview2_executable() -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(value) = std::env::var("ProgramFiles") {
        roots.push(PathBuf::from(value));
    }
    if let Ok(value) = std::env::var("ProgramFiles(x86)") {
        roots.push(PathBuf::from(value));
    }
    if let Ok(value) = std::env::var("LocalAppData") {
        roots.push(PathBuf::from(value));
    }

    for root in roots {
        let app_root = root
            .join("Microsoft")
            .join("EdgeWebView")
            .join("Application");
        if let Some(found) = find_webview2_under(&app_root) {
            return Some(found);
        }
    }

    None
}

fn find_webview2_under(path: &Path) -> Option<PathBuf> {
    let direct = path.join("msedgewebview2.exe");
    if direct.exists() {
        return Some(direct);
    }
    let entries = fs::read_dir(path).ok()?;
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }
        let candidate = entry_path.join("msedgewebview2.exe");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn install_webview2_runtime() -> Result<()> {
    {
        let mut status = WEBVIEW2_STATUS.lock().unwrap();
        *status = WebView2InstallStatus::Installing;
    }
    let badge = crate::overlay::auto_copy_badge::locale_text();
    crate::overlay::auto_copy_badge::show_progress_notification(
        badge.installing_webview2,
        badge.downloading_webview2_installer,
        5.0,
    );

    let installer_path = crate::unpack_dlls::private_bin_dir().join(WEBVIEW2_BOOTSTRAPPER_NAME);
    let _ = fs::create_dir_all(crate::unpack_dlls::private_bin_dir());
    let response = ureq::get(WEBVIEW2_BOOTSTRAPPER_URL)
        .call()
        .map_err(|err| anyhow!("Failed to download WebView2 installer: {err}"))?;
    let mut reader = response.into_body().into_reader();
    let mut file = fs::File::create(&installer_path)
        .map_err(|err| anyhow!("Failed to create '{}': {err}", installer_path.display()))?;
    std::io::copy(&mut reader, &mut file)
        .map_err(|err| anyhow!("Failed to write '{}': {err}", installer_path.display()))?;

    crate::overlay::auto_copy_badge::show_progress_notification(
        badge.installing_webview2,
        badge.running_webview2_installer,
        55.0,
    );

    let status = std::process::Command::new(&installer_path)
        .args(["/silent", "/install"])
        .status()
        .map_err(|err| anyhow!("Failed to launch WebView2 installer: {err}"))?;

    if !status.success() && !webview2_runtime_installed() {
        let message = format!("WebView2 installer exited with status {status}");
        *WEBVIEW2_STATUS.lock().unwrap() = WebView2InstallStatus::Error(message.clone());
        crate::overlay::auto_copy_badge::hide_progress_notification();
        crate::overlay::auto_copy_badge::show_error_notification(badge.webview2_install_failed);
        return Err(anyhow!(message));
    }

    let _ = fs::remove_file(&installer_path);
    *WEBVIEW2_STATUS.lock().unwrap() = WebView2InstallStatus::Installed;
    crate::overlay::auto_copy_badge::hide_progress_notification();
    crate::overlay::auto_copy_badge::show_detailed_notification(
        badge.webview2_ready,
        badge.webview2_installed_restarting,
        crate::overlay::auto_copy_badge::NotificationType::Success,
    );

    // Auto-restart the app so the new WebView2 runtime is loaded fresh and
    // every overlay that fell back to native menus (tray popup, etc.) picks
    // up the full web UI on the next launch.
    if let Ok(exe) = std::env::current_exe() {
        // Give the notification a brief moment to render before replacing
        // the process.
        std::thread::sleep(std::time::Duration::from_millis(900));
        let _ = std::process::Command::new(&exe)
            .args(std::env::args().skip(1))
            .spawn();
        std::process::exit(0);
    }
    Ok(())
}
