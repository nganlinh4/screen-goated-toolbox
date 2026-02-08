use winreg::RegKey;
use winreg::enums::*;

// Image extensions supported by input handler
const IMAGE_EXTENSIONS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".ico", ".tiff", ".tif", ".svg",
];

// Audio extensions supported by input handler
const AUDIO_EXTENSIONS: &[&str] = &[
    ".wav", ".mp3", ".flac", ".ogg", ".m4a", ".aac", ".alac", ".aiff", ".aif", ".wma", ".opus",
    ".m4b",
];

// Expanded Text/Code extensions list
const TEXT_EXTENSIONS: &[&str] = &[
    ".txt",
    ".md",
    ".json",
    ".xml",
    ".rs",
    ".py",
    ".js",
    ".ts",
    ".tsx",
    ".jsx",
    ".html",
    ".css",
    ".scss",
    ".less",
    ".sql",
    ".java",
    ".cpp",
    ".c",
    ".h",
    ".hpp",
    ".cs",
    ".go",
    ".rb",
    ".php",
    ".sh",
    ".bat",
    ".ps1",
    ".cmd",
    ".yaml",
    ".yml",
    ".toml",
    ".ini",
    ".cfg",
    ".conf",
    ".gradle",
    ".properties",
    ".lua",
    ".swift",
    ".kt",
    ".kts",
    ".dart",
    ".R",
    ".pl",
    ".asm",
    ".vim",
    ".gitignore",
    ".env",
];

// Perceived types
const PERCEIVED_TYPES: &[&str] = &["text", "image", "audio"];

pub fn ensure_context_menu_entry() {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);

    // Current EXE path
    let exe_path = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return,
    };
    let exe_path_str = exe_path.to_str().unwrap_or("");
    if exe_path_str.is_empty() {
        return;
    }

    // 1. Remove the old global entry if it exists (Cleanup)
    if let Ok(classes) = hkcu.open_subkey("Software\\Classes") {
        if let Ok(star) = classes.open_subkey("*") {
            if let Ok(shell) = star.open_subkey("shell") {
                let _ = shell.delete_subkey_all("SGT_Process");
                let _ = shell.delete_subkey_all("Process with SGT");
            }
        }
    }

    // 2. Register for specific extensions AND Perceived Types via SystemFileAssociations
    // Path: HKCU\Software\Classes\SystemFileAssociations\<Key>\shell\SGT_Process

    let all_keys: Vec<&str> = PERCEIVED_TYPES
        .iter()
        .chain(IMAGE_EXTENSIONS.iter())
        .chain(AUDIO_EXTENSIONS.iter())
        .chain(TEXT_EXTENSIONS.iter())
        .cloned()
        .collect();

    for key_name in all_keys {
        // Use SystemFileAssociations for robust context menu addition
        let path = format!(
            "Software\\Classes\\SystemFileAssociations\\{}\\shell\\SGT_Process",
            key_name
        );

        // We need to create the full path. create_subkey creates parents if missing.
        if let Ok((key, _)) = hkcu.create_subkey(&path) {
            let _ = key.set_value("", &"Process with SGT");
            let _ = key.set_value("Icon", &exe_path_str);
            // Prevent this verb from ever becoming the default double-click action
            // when a file type has no other default handler
            let _ = key.set_value("NeverDefault", &"");

            // Command
            if let Ok((cmd_key, _)) = key.create_subkey("command") {
                let cmd_str = format!("\"{}\" \"%1\"", exe_path_str);
                let _ = cmd_key.set_value("", &cmd_str);
            }
        }
    }
}
