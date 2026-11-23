use winreg::RegKey;
use winreg::enums::*;

// Image extensions supported by input handler
const IMAGE_EXTENSIONS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp", ".ico", ".tiff", ".tif", ".svg",
];

// Audio extensions supported by input handler
const AUDIO_EXTENSIONS: &[&str] = &[
    ".wav", ".mp3", ".flac", ".ogg", ".m4a", ".aac", ".alac", ".aiff", ".aif", ".opus", ".m4b",
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
const CLASSES_ROOT: &str = "Software\\Classes\\";
const SYSTEM_FILE_ASSOCIATIONS_ROOT: &str = "Software\\Classes\\SystemFileAssociations\\";
const SHELL_SEGMENT: &str = "\\shell\\";

fn supported_extensions() -> impl Iterator<Item = &'static str> {
    IMAGE_EXTENSIONS
        .iter()
        .chain(AUDIO_EXTENSIONS.iter())
        .chain(TEXT_EXTENSIONS.iter())
        .copied()
}

fn build_shell_verb_path(root: &str, subject: &str, verb: &str) -> String {
    let mut path = String::from(root);
    path.push_str(subject);
    path.push_str(SHELL_SEGMENT);
    path.push_str(verb);
    path
}

const LEGACY_PERCEIVED_TYPES: &[&str] = &["text", "image", "audio"];

/// Every location an earlier release registered and the current one does not:
/// the old display-name verb anywhere, and either verb on the catch-all,
/// perceived-type and per-class keys. The current per-extension
/// SystemFileAssociations verb is deliberately absent.
fn legacy_verb_paths() -> Vec<String> {
    let mut paths = Vec::new();
    for verb in [PROCESS_WITH_SGT_VERB, LEGACY_PROCESS_WITH_SGT_VERB] {
        paths.push(build_shell_verb_path(CLASSES_ROOT, "*", verb));
        for perceived_type in LEGACY_PERCEIVED_TYPES {
            paths.push(build_shell_verb_path(
                SYSTEM_FILE_ASSOCIATIONS_ROOT,
                perceived_type,
                verb,
            ));
        }
        for extension in supported_extensions() {
            paths.push(build_shell_verb_path(CLASSES_ROOT, extension, verb));
        }
    }
    for extension in supported_extensions() {
        paths.push(build_shell_verb_path(
            SYSTEM_FILE_ASSOCIATIONS_ROOT,
            extension,
            LEGACY_PROCESS_WITH_SGT_VERB,
        ));
    }
    paths
}

/// Delete only legacy keys that still exist, so a clean profile costs reads.
fn cleanup_legacy_context_menu_entries(root: &RegKey) {
    for path in legacy_verb_paths() {
        if root.open_subkey(&path).is_ok() {
            let _ = root.delete_subkey_all(&path);
        }
    }
}

fn open_or_create(parent: &RegKey, path: &str) -> Option<RegKey> {
    parent
        .open_subkey_with_flags(path, KEY_READ | KEY_WRITE)
        .or_else(|_| parent.create_subkey(path).map(|(key, _)| key))
        .ok()
}

/// Write a value only when it differs, leaving an up-to-date verb untouched.
fn ensure_string_value(key: &RegKey, name: &str, value: &str) {
    if key.get_value::<String, _>(name).ok().as_deref() != Some(value) {
        let _ = key.set_value(name, &value);
    }
}

fn ensure_context_menu_entries(root: &RegKey, exe_path: &str) {
    cleanup_legacy_context_menu_entries(root);

    // Register only explicit supported extensions.
    // Using perceived types like "text"/"image"/"audio" is too broad and can
    // make SGT appear as the effective handler for unrelated/unassociated files.
    let command = format!("\"{exe_path}\" --process-with-sgt \"%1\"");
    for extension in supported_extensions() {
        let path = build_shell_verb_path(
            SYSTEM_FILE_ASSOCIATIONS_ROOT,
            extension,
            PROCESS_WITH_SGT_VERB,
        );
        let Some(key) = open_or_create(root, &path) else {
            continue;
        };
        ensure_string_value(&key, "", PROCESS_WITH_SGT_LABEL);
        ensure_string_value(&key, "Icon", exe_path);
        ensure_string_value(&key, "NeverDefault", "");
        if let Some(command_key) = open_or_create(&key, "command") {
            ensure_string_value(&command_key, "", &command);
        }
    }
}

pub fn ensure_context_menu_entry() {
    let Ok(exe_path) = std::env::current_exe() else {
        return;
    };
    let Some(exe_path) = exe_path.to_str().filter(|path| !path.is_empty()) else {
        return;
    };
    ensure_context_menu_entries(&RegKey::predef(HKEY_CURRENT_USER), exe_path);
}

#[cfg(test)]
mod tests;
