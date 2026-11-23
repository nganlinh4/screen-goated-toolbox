use super::*;
use serde_json::{Value, json};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        Self::in_directory(&std::env::temp_dir())
    }

    fn in_directory(parent: &Path) -> Self {
        let path = parent.join(format!(
            "sgt-program-lookup-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(std::fs::canonicalize(path).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn program_lookup_shared_cases_preserve_explicit_paths_without_implicit_cwd() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/phone-control/program-lookup.json"
    )))
    .unwrap();
    let owned = Fixture::new();
    for case in fixture["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let root = owned.0.join(id);
        for directory in ["cwd", "path", "first", "second", "other"] {
            std::fs::create_dir_all(root.join(directory)).unwrap();
        }
        for file in case["files"].as_array().unwrap() {
            let path = root.join(file.as_str().unwrap());
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, []).unwrap();
        }
        let raw_program = case["program"].as_str().unwrap();
        let program = raw_program
            .strip_prefix("<root>/")
            .map(|suffix| root.join(suffix).to_string_lossy().into_owned())
            .unwrap_or_else(|| raw_program.to_string());
        let directories = case["path"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| root.join(value.as_str().unwrap()))
            .collect::<Vec<_>>();
        let extensions = case["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        let actual =
            resolve_program_with(&program, &root.join("cwd"), &directories, &extensions).unwrap();
        if let Some(expected) = case["expected"].as_str() {
            assert_eq!(PathBuf::from(actual), root.join(expected), "{id}");
        } else {
            assert_eq!(actual, program, "{id}");
        }
    }
}

#[test]
fn explicit_missing_paths_remain_anchored_and_drive_relative_paths_are_rejected() {
    let owned = Fixture::new();
    let result = resolve_program_with("./missing.exe", &owned.0, &[], &["exe".into()]).unwrap();
    assert!(Path::new(&result).is_absolute());
    assert_eq!(PathBuf::from(result), owned.0.join("missing.exe"));
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/phone-control/program-lookup.json"
    )))
    .unwrap();
    for program in fixture["windowsInvalidPrograms"].as_array().unwrap() {
        assert!(resolve_program_with(program.as_str().unwrap(), &owned.0, &[], &[]).is_err());
    }
}

#[test]
fn configured_relative_search_directory_resolves_before_child_cwd_is_applied() {
    let current = std::env::current_dir().unwrap();
    let owned = Fixture::in_directory(&current);
    let child_cwd = owned.0.join("child");
    let bin = owned.0.join("bin");
    std::fs::create_dir(&child_cwd).unwrap();
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(bin.join("tool.cmd"), []).unwrap();
    let relative = PathBuf::from(owned.0.file_name().unwrap()).join("bin");
    let result = resolve_program_with("tool", &child_cwd, &[relative], &["cmd".into()]).unwrap();
    assert!(Path::new(&result).is_absolute());
    assert_eq!(std::fs::canonicalize(result).unwrap(), bin.join("tool.cmd"));
}

#[test]
fn bare_program_cannot_execute_a_working_directory_shim_through_native_fallback() {
    let owned = Fixture::new();
    let name = format!(
        "lookup-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    std::fs::write(
        owned.0.join(format!("{name}.cmd")),
        "@echo off\r\necho unexpected local lookup\r\n",
    )
    .unwrap();
    let result = super::super::run_command_with_timeout(
        &json!({"program":name,"args":[],"cwd":owned.0}),
        Duration::from_secs(5),
    );
    assert!(
        result.is_err(),
        "bare program executed cwd shim: {result:?}"
    );
}

#[test]
fn bare_names_missing_from_path_use_the_native_fallback_directories_in_order() {
    let fixture = Fixture::new();
    let first = fixture.0.join("first");
    let second = fixture.0.join("second");
    for directory in [&first, &second] {
        std::fs::create_dir(directory).unwrap();
    }
    std::fs::write(second.join("tool.exe"), []).unwrap();
    std::fs::write(second.join("script.cmd"), []).unwrap();
    std::fs::write(first.join("script.cmd"), []).unwrap();
    let directories = [first.clone(), second.clone()];
    assert_eq!(
        native_fallback_in("tool", &directories),
        Some(second.join("tool.exe"))
    );
    assert_eq!(
        native_fallback_in("script.cmd", &directories),
        Some(first.join("script.cmd"))
    );
    for program in ["missing", "script", "first/script.cmd"] {
        assert_eq!(native_fallback_in(program, &directories), None, "{program}");
    }
    let absolute = second.join("tool.exe");
    assert_eq!(
        native_fallback_in(absolute.to_str().unwrap(), &directories),
        None
    );
}
