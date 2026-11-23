use super::types::CookieBrowser;
use std::collections::HashSet;
use std::path::PathBuf;
use winreg::{
    RegKey,
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
};

pub fn detect_installed_browsers() -> Vec<CookieBrowser> {
    let mut found = vec![CookieBrowser::None];
    let mut found_set = HashSet::new();
    found_set.insert(CookieBrowser::None);

    let add_if_new = |browser: CookieBrowser,
                      f_list: &mut Vec<CookieBrowser>,
                      f_set: &mut HashSet<CookieBrowser>| {
        if !f_set.contains(&browser) {
            f_set.insert(browser.clone());
            f_list.push(browser);
        }
    };

    // Map Browser -> Executable Name(s) to query in Registry "App Paths"
    let registry_targets = [
        (CookieBrowser::Chrome, "chrome.exe"),
        (CookieBrowser::Firefox, "firefox.exe"),
        (CookieBrowser::Edge, "msedge.exe"),
        (CookieBrowser::Brave, "brave.exe"),
        (CookieBrowser::Opera, "opera.exe"), // or launcher.exe
        (CookieBrowser::Vivaldi, "vivaldi.exe"),
        (CookieBrowser::Chromium, "chromium.exe"),
        (CookieBrowser::Whale, "whale.exe"),
    ];

    // 1. Scan "App Paths" (HKLM and HKCU) - Most robust way to find installed apps by exe name
    for (browser, exe_name) in &registry_targets {
        // Check HKLM
        let hklm_exists = check_registry_key("HKLM", exe_name);
        if hklm_exists {
            add_if_new(browser.clone(), &mut found, &mut found_set);
            continue;
        }
        // Check HKCU
        let hkcu_exists = check_registry_key("HKCU", exe_name);
        if hkcu_exists {
            add_if_new(browser.clone(), &mut found, &mut found_set);
        }
    }

    // 2. Scan StartMenuInternet (Existing Logic - Good for defaults)
    if let Ok(key) =
        RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\Clients\\StartMenuInternet")
    {
        for line in key.enum_keys().flatten() {
            let lower = line.to_lowercase();
            if lower.contains("chrome") {
                add_if_new(CookieBrowser::Chrome, &mut found, &mut found_set);
            } else if lower.contains("firefox") {
                add_if_new(CookieBrowser::Firefox, &mut found, &mut found_set);
            } else if lower.contains("edge") {
                add_if_new(CookieBrowser::Edge, &mut found, &mut found_set);
            } else if lower.contains("brave") {
                add_if_new(CookieBrowser::Brave, &mut found, &mut found_set);
            } else if lower.contains("opera") {
                add_if_new(CookieBrowser::Opera, &mut found, &mut found_set);
            } else if lower.contains("vivaldi") {
                add_if_new(CookieBrowser::Vivaldi, &mut found, &mut found_set);
            } else if lower.contains("chromium") {
                add_if_new(CookieBrowser::Chromium, &mut found, &mut found_set);
            } else if lower.contains("whale") {
                add_if_new(CookieBrowser::Whale, &mut found, &mut found_set);
            }
        }
    }

    // 3. Fallback to common file paths (Manual Fallback)
    let mut check_exe = |browser: CookieBrowser, paths: &[&str]| {
        if found_set.contains(&browser) {
            return;
        }
        for sub_path in paths {
            let roots = [
                std::env::var("ProgramFiles").ok(),
                std::env::var("ProgramFiles(x86)").ok(),
                std::env::var("LocalAppData").ok(),
            ];
            for root in roots.iter().flatten() {
                if PathBuf::from(root).join(sub_path).exists() {
                    add_if_new(browser.clone(), &mut found, &mut found_set);
                    return;
                }
            }
        }
    };

    check_exe(
        CookieBrowser::Chrome,
        &["Google\\Chrome\\Application\\chrome.exe"],
    );
    check_exe(CookieBrowser::Firefox, &["Mozilla Firefox\\firefox.exe"]);
    check_exe(
        CookieBrowser::Edge,
        &["Microsoft\\Edge\\Application\\msedge.exe"],
    );
    check_exe(
        CookieBrowser::Brave,
        &["BraveSoftware\\Brave-Browser\\Application\\brave.exe"],
    );
    check_exe(
        CookieBrowser::Opera,
        &["Opera\\launcher.exe", "Programs\\Opera\\launcher.exe"],
    );
    check_exe(
        CookieBrowser::Vivaldi,
        &["Vivaldi\\Application\\vivaldi.exe"],
    );

    found
}

fn check_registry_key(root: &str, exe_name: &str) -> bool {
    let key = format!(
        "{}\\Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\{}",
        root, exe_name
    );
    let hive = match root {
        "HKLM" => HKEY_LOCAL_MACHINE,
        "HKCU" => HKEY_CURRENT_USER,
        _ => return false,
    };
    let Some(subkey) = key.split_once('\\').map(|(_, subkey)| subkey) else {
        return false;
    };
    RegKey::predef(hive)
        .open_subkey(subkey)
        .and_then(|key| key.get_raw_value(""))
        .is_ok()
}
