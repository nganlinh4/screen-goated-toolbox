//! Reveal only an app-known result or its output directory.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, ensure};

enum Target {
    Result(PathBuf),
    Directory(PathBuf),
}

fn same_path(left: &Path, right: &Path) -> Result<bool> {
    let left = left.to_str().context("Output path is unavailable.")?;
    let right = right.to_str().context("Output path is unavailable.")?;
    super::super::creation_path::same(left, right).map_err(anyhow::Error::msg)
}

fn select_target(
    requested: Option<&str>,
    default_directory: &Path,
    results: &[PathBuf],
) -> Result<Target> {
    ensure!(
        default_directory.is_absolute(),
        "Output folder is unavailable."
    );
    let Some(requested) = requested.filter(|value| !value.trim().is_empty()) else {
        return Ok(Target::Directory(default_directory.to_owned()));
    };
    let requested = Path::new(requested);
    ensure!(
        requested.is_absolute(),
        "Output path is not a known result."
    );
    if same_path(requested, default_directory)? {
        return Ok(Target::Directory(default_directory.to_owned()));
    }
    for result in results.iter().filter(|path| path.is_absolute()) {
        if same_path(requested, result)? {
            return Ok(Target::Result(result.clone()));
        }
        if let Some(parent) = result.parent()
            && same_path(requested, parent)?
        {
            return Ok(Target::Directory(parent.to_owned()));
        }
    }
    Err(anyhow!("Output path is not a known result."))
}

fn resolve_folder(
    requested: Option<&str>,
    default_directory: &Path,
    results: &[PathBuf],
) -> Result<PathBuf> {
    // Select a trusted stored path before any filesystem lookup on the request.
    // Unregistered network paths and shell targets must not trigger a lookup.
    let folder = match select_target(requested, default_directory, results)? {
        Target::Result(result) => {
            let result = result.canonicalize().context("Result is unavailable.")?;
            ensure!(result.is_file(), "Result is unavailable.");
            result
                .parent()
                .context("Result folder is unavailable.")?
                .to_owned()
        }
        Target::Directory(directory) => directory
            .canonicalize()
            .context("Output folder is unavailable.")?,
    };
    ensure!(folder.is_dir(), "Output folder is unavailable.");
    Ok(folder)
}

fn reveal_with(
    requested: Option<&str>,
    default_directory: &Path,
    results: &[PathBuf],
    reveal: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<()> {
    let folder = resolve_folder(requested, default_directory, results)?;
    reveal(&folder).context("Could not open result folder.")
}

pub(crate) fn open_result_folder(
    tool: &str,
    requested: Option<&str>,
    default_directory: &Path,
    mut session_results: Vec<PathBuf>,
) -> Result<()> {
    session_results.extend(
        super::super::generation_history::list(tool)
            .map_err(anyhow::Error::msg)?
            .into_iter()
            .map(|entry| PathBuf::from(entry.output_path)),
    );
    reveal_with(requested, default_directory, &session_results, |folder| {
        open::that(folder)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir()
                .join(crate::overlay::creation_identity::random_id("sgt-output-folder-").unwrap());
            std::fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn directory(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::create_dir(&path).unwrap();
            path
        }
        fn file(&self, directory: &Path, name: &str) -> PathBuf {
            let path = directory.join(name);
            std::fs::write(&path, b"result").unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn known_result_and_its_folder_resolve_to_the_same_directory() {
        let fixture = Fixture::new();
        let default = fixture.directory("default");
        let output = fixture.directory("output & space");
        let result = fixture.file(&output, "result & percent%.svg");
        let known = vec![result.clone()];
        for path in [&result, &output] {
            assert_eq!(
                resolve_folder(path.to_str(), &default, &known).unwrap(),
                output.canonicalize().unwrap()
            );
        }
    }

    #[test]
    fn default_folder_supports_missing_empty_and_explicit_requests() {
        let fixture = Fixture::new();
        let default = fixture.directory("default");
        for requested in [None, Some(""), Some("  "), default.to_str()] {
            assert_eq!(
                resolve_folder(requested, &default, &[]).unwrap(),
                default.canonicalize().unwrap()
            );
        }
    }

    #[test]
    fn unrelated_files_and_folders_are_rejected_even_beside_a_known_result() {
        let fixture = Fixture::new();
        let default = fixture.directory("default");
        let output = fixture.directory("output");
        let known = fixture.file(&output, "known.svg");
        let unrelated = fixture.file(&output, "unrelated.exe");
        for path in [&unrelated, &fixture.0] {
            assert!(resolve_folder(path.to_str(), &default, std::slice::from_ref(&known)).is_err());
        }
    }

    #[test]
    fn reveal_receives_only_a_validated_folder_and_is_never_called_for_rejection() {
        let fixture = Fixture::new();
        let default = fixture.directory("default");
        let result = fixture.file(&default, "result.svg");
        assert!(
            reveal_with(Some("unregistered"), &default, &[], |_| {
                panic!("Unregistered output reached the shell boundary")
            })
            .is_err()
        );
        let mut calls = 0;
        reveal_with(
            result.to_str(),
            &default,
            std::slice::from_ref(&result),
            |folder| {
                calls += 1;
                assert_eq!(folder, default.canonicalize().unwrap());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(calls, 1);
    }

    #[test]
    fn shell_targets_and_unregistered_network_paths_fail_before_filesystem_resolution() {
        let default = std::env::temp_dir();
        for requested in [
            "application",
            "file:///C:/result.svg",
            "https://example.invalid/",
            "C:result.svg",
            "\\\\unregistered.invalid\\share\\result.svg",
        ] {
            assert!(select_target(Some(requested), &default, &[]).is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn explicitly_recorded_network_outputs_are_selected_without_probing_the_network() {
        let default = std::env::temp_dir();
        let stored = PathBuf::from(r"\\recorded.invalid\export\result.svg");
        let selected =
            select_target(stored.to_str(), &default, std::slice::from_ref(&stored)).unwrap();
        assert!(matches!(selected, Target::Result(path) if path == stored));
    }

    #[test]
    fn stored_missing_or_non_file_results_do_not_become_shell_targets() {
        let fixture = Fixture::new();
        let default = fixture.directory("default");
        let directory = fixture.directory("not-a-file");
        let missing = fixture.0.join("missing.svg");
        for path in [directory, missing] {
            assert!(resolve_folder(path.to_str(), &default, std::slice::from_ref(&path)).is_err());
        }
        let file = fixture.file(&default, "not-a-directory");
        assert!(resolve_folder(None, &file, &[]).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn stored_path_comparison_uses_windows_ordinal_case_rules() {
        let fixture = Fixture::new();
        let default = fixture.directory("default");
        let result = fixture.file(&default, "Result.svg");
        let requested = result.to_str().unwrap().to_ascii_uppercase();
        assert_eq!(
            resolve_folder(Some(&requested), &default, &[result]).unwrap(),
            default.canonicalize().unwrap()
        );
    }
}
