//! The cursor registry values a pack application writes, computed without
//! touching the registry so the mapping can be checked on its own.

use super::catalog::{REGISTRY_FILE_MAP, SCHEME_FILE_ORDER};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// `(value name, registry path)` for each cursor role to write. A live preview
/// changes only the arrow; a scaled file wins over the pack's original.
pub(super) fn cursor_values(
    files: &HashMap<String, PathBuf>,
    effective: &HashMap<String, PathBuf>,
    live_preview_only: bool,
) -> Result<Vec<(&'static str, String)>, String> {
    REGISTRY_FILE_MAP
        .iter()
        .filter(|(value_name, _)| !live_preview_only || *value_name == "Arrow")
        .map(|(value_name, file_name)| {
            effective
                .get(*file_name)
                .or_else(|| files.get(*file_name))
                .map(|path| (*value_name, to_registry_path(path)))
                .ok_or_else(|| format!("Cannot apply pack, missing file '{file_name}'"))
        })
        .collect()
}

/// The comma-separated scheme entry, in the order Windows expects.
pub(super) fn scheme_list(effective: &HashMap<String, PathBuf>) -> Result<String, String> {
    SCHEME_FILE_ORDER
        .iter()
        .map(|name| {
            effective
                .get(*name)
                .map(|path| to_registry_path(path))
                .ok_or_else(|| format!("Missing file '{name}'"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|paths| paths.join(","))
}

pub(super) fn to_registry_path(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\")
}

#[cfg(test)]
mod tests {
    use super::super::catalog::REQUIRED_FILES;
    use super::*;

    fn pack(root: &str) -> HashMap<String, PathBuf> {
        REQUIRED_FILES
            .iter()
            .map(|name| (name.to_string(), PathBuf::from(format!("{root}/{name}"))))
            .collect()
    }

    #[test]
    fn every_role_is_written_and_scaled_files_win() {
        let files = pack("C:/packs/a");
        let mut effective = files.clone();
        effective.insert("pointer.cur".into(), PathBuf::from("C:/scaled/pointer.cur"));
        let values = cursor_values(&files, &effective, false).unwrap();
        assert_eq!(values.len(), REGISTRY_FILE_MAP.len());
        let arrow = values.iter().find(|(name, _)| *name == "Arrow").unwrap();
        assert_eq!(arrow.1, r"C:\scaled\pointer.cur");
        assert!(values.iter().all(|(_, path)| !path.contains('/')));
    }

    #[test]
    fn a_live_preview_changes_only_the_arrow() {
        let files = pack("C:/packs/a");
        let values = cursor_values(&files, &HashMap::new(), true).unwrap();
        assert_eq!(
            values,
            vec![("Arrow", r"C:\packs\a\pointer.cur".to_string())]
        );
    }

    #[test]
    fn a_missing_file_refuses_the_whole_pack() {
        let mut files = pack("C:/packs/a");
        files.remove("help.cur");
        let error = cursor_values(&files, &HashMap::new(), false).unwrap_err();
        assert!(error.contains("help.cur"), "{error}");
        assert!(scheme_list(&files).unwrap_err().contains("help.cur"));
    }

    #[test]
    fn the_scheme_lists_every_file_in_windows_order() {
        let files = pack("C:/packs/a");
        let scheme = scheme_list(&files).unwrap();
        let entries: Vec<_> = scheme.split(',').collect();
        assert_eq!(entries.len(), SCHEME_FILE_ORDER.len());
        for (entry, name) in entries.iter().zip(SCHEME_FILE_ORDER) {
            assert_eq!(*entry, format!(r"C:\packs\a\{name}"));
        }
    }
}
