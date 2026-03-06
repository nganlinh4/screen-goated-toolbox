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

const PROCESS_WITH_SGT_LABEL: &str = "Process with SGT";
const PROCESS_WITH_SGT_VERB: &str = "SGT_Process";
const LEGACY_PROCESS_WITH_SGT_VERB: &str = "Process with SGT";

fn supported_extensions() -> impl Iterator<Item = &'static str> {
    IMAGE_EXTENSIONS
        .iter()
        .chain(AUDIO_EXTENSIONS.iter())
        .chain(TEXT_EXTENSIONS.iter())
        .copied()
}

fn cleanup_legacy_context_menu_entries(hkcu: &RegKey) {
    let legacy_perceived_types = ["text", "image", "audio"];

    for verb in [PROCESS_WITH_SGT_VERB, LEGACY_PROCESS_WITH_SGT_VERB] {
        let _ = hkcu.delete_subkey_all(format!("Software\\Classes\\*\\shell\\{verb}"));

        for perceived_type in legacy_perceived_types {
            let _ = hkcu.delete_subkey_all(format!(
                "Software\\Classes\\SystemFileAssociations\\{}\\shell\\{}",
                perceived_type, verb
            ));
        }

        for extension in supported_extensions() {
            let _ = hkcu.delete_subkey_all(format!(
                "Software\\Classes\\SystemFileAssociations\\{}\\shell\\{}",
                extension, verb
            ));
            let _ = hkcu
                .delete_subkey_all(format!("Software\\Classes\\{}\\shell\\{}", extension, verb));
        }
    }
}

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

    cleanup_legacy_context_menu_entries(&hkcu);

    // Register only explicit supported extensions.
    // Using perceived types like "text"/"image"/"audio" is too broad and can
    // make SGT appear as the effective handler for unrelated/unassociated files.
    for extension in supported_extensions() {
        let path = format!(
            "Software\\Classes\\SystemFileAssociations\\{}\\shell\\SGT_Process",
            extension
        );

        if let Ok((key, _)) = hkcu.create_subkey(&path) {
            let _ = key.set_value("", &PROCESS_WITH_SGT_LABEL);
            let _ = key.set_value("Icon", &exe_path_str);
            let _ = key.set_value("NeverDefault", &"");

            if let Ok((cmd_key, _)) = key.create_subkey("command") {
                let cmd_str = format!("\"{}\" --process-with-sgt \"%1\"", exe_path_str);
                let _ = cmd_key.set_value("", &cmd_str);
            }
        }
    }
}
