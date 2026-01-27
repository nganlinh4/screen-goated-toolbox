use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

static KEYVIZ_RUNNING: AtomicBool = AtomicBool::new(false);
static KEYVIZ_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn set_enabled(enabled: bool) {
    KEYVIZ_ENABLED.store(enabled, Ordering::SeqCst);
}

pub fn is_enabled() -> bool {
    KEYVIZ_ENABLED.load(Ordering::SeqCst)
}

pub fn get_keyviz_path() -> Option<PathBuf> {
    let local_app_data = std::env::var("LOCALAPPDATA").ok()?;
    let path = PathBuf::from(local_app_data).join("Programs\\Keyviz\\Keyviz.exe");
    if path.exists() {
        return Some(path);
    }
    // Check common Program Files locations
    let program_files = std::env::var("ProgramFiles").ok();
    if let Some(pf) = program_files {
        let path = PathBuf::from(pf).join("Keyviz\\Keyviz.exe");
        if path.exists() {
            return Some(path);
        }
    }

    // Check where command
    if let Ok(output) = Command::new("where").arg("Keyviz").output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout);
            let p = PathBuf::from(s.trim());
            if p.exists() {
                return Some(p);
            }
        }
    }

    None
}

pub fn is_installed() -> bool {
    get_keyviz_path().is_some()
}

pub fn ensure_config() -> anyhow::Result<()> {
    let app_data = std::env::var("APPDATA")?;
    let config_dir = PathBuf::from(app_data).join("org.keyviz");
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)?;
    }

    let config_path = config_dir.join("store.json");

    // Read embedded config
    let config_content = include_str!("keyviz_config.json");

    // Optional: Only overwrite if it matches expected structure or user requested force reset?
    // User asked "with config file preconfigured". I'll overwrite it to ensure it matches.
    fs::write(config_path, config_content)?;

    Ok(())
}

pub fn start() -> anyhow::Result<()> {
    if !is_enabled() {
        return Ok(());
    }

    if let Some(path) = get_keyviz_path() {
        ensure_config()?;

        // Check if already running
        let output = Command::new("tasklist")
            .args(&["/FI", "IMAGENAME eq Keyviz.exe", "/NH"])
            .output()?;
        let output_str = String::from_utf8_lossy(&output.stdout);

        if !output_str.contains("Keyviz.exe") {
            crate::log_info!("Starting Keyviz from {:?}", path);
            Command::new(path).spawn()?;
            KEYVIZ_RUNNING.store(true, Ordering::SeqCst);
        } else {
            crate::log_info!("Keyviz already running");
            KEYVIZ_RUNNING.store(true, Ordering::SeqCst);
        }
    } else {
        crate::log_info!("Keyviz not found, cannot start");
    }
    Ok(())
}

pub fn stop() -> anyhow::Result<()> {
    // Only stop if we started it or we want to clean up?
    // User said "done recording turn the app off".
    if KEYVIZ_RUNNING.load(Ordering::SeqCst) || is_enabled() {
        crate::log_info!("Stopping Keyviz");
        Command::new("taskkill")
            .args(&["/IM", "Keyviz.exe", "/F"])
            .output()?;
        KEYVIZ_RUNNING.store(false, Ordering::SeqCst);
    }
    Ok(())
}

pub fn install_keyviz() -> anyhow::Result<()> {
    // 1. Get latest release
    let url = "https://api.github.com/repos/mulaRahul/keyviz/releases/latest";
    let resp = ureq::get(url)
        .header("User-Agent", "screen-goated-toolbox")
        .call()?;

    // Body needs to be converted to a reader
    let json: serde_json::Value = serde_json::from_reader(resp.into_body().into_reader())?;

    // Find asset
    let assets = json["assets"]
        .as_array()
        .ok_or(anyhow::anyhow!("No assets found"))?;

    // Look for setup exe
    let asset = assets
        .iter()
        .find(|a| {
            let name = a["name"].as_str().unwrap_or("");
            name.ends_with(".exe") && !name.contains("portable")
        })
        .ok_or(anyhow::anyhow!("No installer found"))?;

    let download_url = asset["browser_download_url"]
        .as_str()
        .ok_or(anyhow::anyhow!("No download URL"))?;
    let file_name = asset["name"].as_str().unwrap_or("keyviz_setup.exe");

    let temp_dir = std::env::temp_dir();
    let installer_path = temp_dir.join(file_name);

    crate::log_info!("Downloading Keyviz from {}", download_url);

    let response = ureq::get(download_url).call()?;
    let mut reader = response.into_body().into_reader();
    let mut file = fs::File::create(&installer_path)?;
    std::io::copy(&mut reader, &mut file)?;

    crate::log_info!("Running installer {:?}", installer_path);

    // Run installer
    // /S for silent? Many installers support it. NSIS usually supports /S. Electron-builder usually /S or /silent.
    // Try silent install first.
    let status = Command::new(&installer_path).arg("/S").status();

    if status.is_err() || !status.unwrap().success() {
        // If silent fails, try normal
        Command::new(&installer_path).spawn()?;
    }

    Ok(())
}
