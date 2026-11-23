//! Recorder state roots, preserving the optional system-directory behavior.

use std::path::PathBuf;

pub(super) fn optional_local_data_dir() -> Option<PathBuf> {
    if crate::paths::runtime_state_root_from(std::env::var_os(crate::paths::RUNTIME_STATE_ROOT_ENV))
        .is_some()
    {
        return Some(crate::paths::app_runtime_local_data_dir());
    }
    dirs::data_local_dir().map(|base| base.join("screen-goated-toolbox"))
}

/// Arguments of the last export, saved by the worker and read back by the
/// host's `--replay-export`; both builds must agree on this one location.
pub(crate) fn export_replay_args_path() -> Option<PathBuf> {
    optional_local_data_dir().map(|base| base.join("export-debug").join("last_export_args.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};
    use std::time::{SystemTime, UNIX_EPOCH};

    const CHILD_ENV: &str = "SGT_RECORDER_STATE_ISOLATION_TEST_CHILD";

    #[test]
    fn isolated_recorder_paths_child() {
        if std::env::var_os(CHILD_ENV).is_none() {
            return;
        }
        let state = PathBuf::from(std::env::var_os(crate::paths::RUNTIME_STATE_ROOT_ENV).unwrap());
        let expected = state.join("local-app-data").join("screen-goated-toolbox");
        assert_eq!(optional_local_data_dir().unwrap(), expected);
        assert_eq!(crate::paths::app_runtime_local_data_dir(), expected);

        assert_eq!(
            export_replay_args_path().unwrap(),
            expected.join("export-debug").join("last_export_args.json")
        );

        #[cfg(feature = "recorder-worker")]
        {
            let recordings = super::super::ipc::media_server::recordings_dir();
            assert_eq!(recordings, expected.join("recordings"));
            std::fs::create_dir_all(&recordings).unwrap();
            let managed = recordings.join("recording.mp4");
            std::fs::write(&managed, b"synthetic managed data").unwrap();
            let outside = state.parent().unwrap().join("user-export.mp4");
            std::fs::write(&outside, b"synthetic user export").unwrap();
            assert!(
                super::super::handle_ipc_command(
                    "delete_file".to_string(),
                    serde_json::json!({"path": outside.to_string_lossy()})
                )
                .is_err()
            );
            assert_eq!(std::fs::read(&outside).unwrap(), b"synthetic user export");
            super::super::handle_ipc_command(
                "delete_file".to_string(),
                serde_json::json!({"path": managed.to_string_lossy()}),
            )
            .unwrap();
            assert!(!managed.exists());

            let backgrounds = expected.join("backgrounds");
            std::fs::create_dir_all(&backgrounds).unwrap();
            std::fs::write(backgrounds.join("fixture.png"), b"synthetic background").unwrap();
            let (bytes, _) =
                super::super::try_read_downloaded_bg("/bg-downloaded/fixture.png").unwrap();
            assert_eq!(bytes, b"synthetic background");
        }
    }

    #[test]
    fn host_and_worker_resolve_the_selected_runtime_root() {
        let root = std::env::temp_dir().join(format!(
            "sgt-recorder-state-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let module = module_path!().split_once("::").unwrap().1;
        let output = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(format!("{module}::isolated_recorder_paths_child"))
            .arg("--nocapture")
            .env(CHILD_ENV, "1")
            .env(crate::paths::RUNTIME_STATE_ROOT_ENV, root.join("runtime"))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        let resolved = std::fs::canonicalize(&root).unwrap();
        let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        assert!(resolved.starts_with(&temp) && resolved != temp);
        std::fs::remove_dir_all(resolved).unwrap();
        assert!(
            output.status.success(),
            "recorder isolation child failed: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
