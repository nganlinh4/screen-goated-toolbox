use std::fs;
use std::path::{Path, PathBuf};

const MAX_SOURCE_LINES: usize = 600;

fn manifest_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn rust_sources_below(root: &Path, sources: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()))
    {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources_below(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
}

fn read_source(relative: &str) -> String {
    fs::read_to_string(manifest_path(relative))
        .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
}

#[test]
fn every_rust_source_stays_within_the_project_size_limit() {
    let mut sources = Vec::new();
    rust_sources_below(&manifest_path("src"), &mut sources);

    for path in sources {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let line_count = source.lines().count();
        assert!(
            line_count <= MAX_SOURCE_LINES,
            "{} has {line_count} lines; limit is {MAX_SOURCE_LINES}",
            path.display()
        );
    }
}

#[test]
fn restore_kernel_event_has_one_wait_owner() {
    let activation = read_source("src/app_activation.rs");
    assert_eq!(activation.matches("WaitForSingleObject(").count(), 1);

    for relative in ["src/hotkey/mod.rs", "src/gui/app/init.rs"] {
        let source = read_source(relative);
        assert!(
            !source.contains("WaitForSingleObject("),
            "{relative} must not consume the restore kernel event"
        );
        assert!(
            !source.contains("ResetEvent("),
            "{relative} must not reset the restore kernel event"
        );
    }
}
