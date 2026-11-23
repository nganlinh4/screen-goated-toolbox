use super::*;
use sha2::{Digest as _, Sha256};

#[test]
fn verified_restart_retains_file_and_directory_guards_until_launch_returns() {
    let directory = std::env::temp_dir().join(format!(
        "sgt-update-relaunch-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let bytes = b"verified update";
    let path = directory.join("ScreenGoatedToolbox_v9.9.9.exe");
    std::fs::write(&path, bytes).unwrap();
    let staged = StagedUpdate {
        version: "9.9.9".into(),
        asset_name: path.file_name().unwrap().to_str().unwrap().into(),
        size_bytes: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(bytes)),
    };
    let current = directory.join("current.exe");
    let failed: Result<()> = staged.with_verified_path_beside(&current, |_| {
        anyhow::bail!("process creation failed");
    });
    assert!(failed.is_err());
    // A failed launch releases verification guards and leaves the same bytes retryable.
    std::fs::write(&path, bytes).unwrap();
    staged
        .with_verified_path_beside(&current, |verified| {
            assert_eq!(verified, path.canonicalize()?.as_path());
            assert!(std::fs::write(&path, b"tampered").is_err());
            assert!(std::fs::remove_file(&path).is_err());
            assert!(std::fs::rename(&directory, directory.with_extension("moved")).is_err());
            Ok(())
        })
        .unwrap();
    std::fs::write(&path, b"tampered").unwrap();
    assert!(
        staged
            .with_verified_path_beside(&current, |_| -> Result<()> {
                panic!("Tampered update must not launch")
            })
            .is_err()
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&directory).unwrap();
}

#[test]
fn verified_update_starts_directly_from_the_guarded_file_without_a_script() {
    let directory = std::env::temp_dir().join(format!(
        "sgt-update-direct-launch-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("ScreenGoatedToolbox_v9.9.8.exe");
    let system = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap());
    std::fs::copy(system.join("System32").join("hostname.exe"), &path).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let staged = StagedUpdate {
        version: "9.9.8".into(),
        asset_name: path.file_name().unwrap().to_str().unwrap().into(),
        size_bytes: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(&bytes)),
    };

    // The image is created while the verification guards are still held, so
    // the bytes that run are the bytes that were just re-hashed.
    let status = staged
        .with_verified_path_beside(&directory.join("current.exe"), |verified| {
            let mut command =
                crate::runtime_support::restart_command(verified, std::iter::empty::<&str>())?;
            assert_eq!(command.get_program(), verified.as_os_str());
            assert_eq!(command.get_args().count(), 0);
            let mut child = command.spawn()?;
            assert!(std::fs::write(&path, b"tampered").is_err());
            Ok(child.wait()?)
        })
        .unwrap();
    assert!(status.success());
    let entries = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    assert_eq!(entries, vec![path.file_name().unwrap().to_owned()]);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&directory).unwrap();
}
