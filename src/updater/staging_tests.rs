use super::*;
use sha2::{Digest as _, Sha256};
use std::cell::Cell;

fn fixture(bytes: &[u8]) -> (PathBuf, UpdateCandidate) {
    let mut nonce = [0_u8; 8];
    getrandom::fill(&mut nonce).unwrap();
    let directory = std::env::temp_dir().join(format!(
        "sgt-update-staging-test-{:016x}",
        u64::from_le_bytes(nonce)
    ));
    std::fs::create_dir(&directory).unwrap();
    let candidate = UpdateCandidate {
        version: semver::Version::new(9, 9, 9),
        body: String::new(),
        asset_name: "ScreenGoatedToolbox_v9.9.9.exe".into(),
        download_url: "https://github.com/nganlinh4/screen-goated-toolbox/releases/download/v9.9.9/ScreenGoatedToolbox_v9.9.9.exe".into(),
        size_bytes: bytes.len() as u64,
        sha256: format!("{:x}", Sha256::digest(bytes)),
    };
    (directory, candidate)
}

#[test]
fn existing_mismatched_target_is_preserved_and_download_is_not_started() {
    let (directory, candidate) = fixture(b"verified");
    let target = directory.join(&candidate.asset_name);
    let partial = directory.join("update_download.part");
    std::fs::write(&target, b"user executable").unwrap();
    std::fs::write(&partial, b"user partial").unwrap();
    let invoked = Cell::new(false);
    assert!(
        stage_with(&candidate, &directory, |_| {
            invoked.set(true);
            Ok(())
        })
        .is_err()
    );
    assert!(!invoked.get());
    assert_eq!(std::fs::read(&target).unwrap(), b"user executable");
    assert_eq!(std::fs::read(&partial).unwrap(), b"user partial");
    std::fs::remove_file(target).unwrap();
    std::fs::remove_file(partial).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn verified_download_is_published_without_touching_other_files() {
    let bytes = b"verified";
    let (directory, candidate) = fixture(bytes);
    let partial = directory.join("update_download.part");
    std::fs::write(&partial, b"keep").unwrap();
    let staged = stage_with(&candidate, &directory, |file| {
        copy_verified(&bytes[..], file, &candidate, || false)
    })
    .unwrap();
    let target = directory.join(&candidate.asset_name);
    assert_eq!(
        staged
            .with_verified_path_beside(&directory.join("current.exe"), |path| Ok(path.to_owned()))
            .unwrap(),
        target.canonicalize().unwrap()
    );
    assert_eq!(std::fs::read(&partial).unwrap(), b"keep");
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
    std::fs::remove_file(target).unwrap();
    std::fs::remove_file(partial).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn interrupted_download_removes_only_its_owned_scratch() {
    let (directory, candidate) = fixture(b"verified");
    let partial = directory.join("update_download.part");
    std::fs::write(&partial, b"keep").unwrap();
    assert!(
        stage_with(&candidate, &directory, |file| {
            file.write_all(b"partial")?;
            bail!("interrupted")
        })
        .is_err()
    );
    assert_eq!(std::fs::read(&partial).unwrap(), b"keep");
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    std::fs::remove_file(partial).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn racing_target_is_never_overwritten() {
    let bytes = b"verified";
    let (directory, candidate) = fixture(bytes);
    let target = directory.join(&candidate.asset_name);
    assert!(
        stage_with(&candidate, &directory, |file| {
            copy_verified(&bytes[..], file, &candidate, || false)?;
            std::fs::write(&target, b"racing user file")?;
            Ok(())
        })
        .is_err()
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"racing user file");
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    std::fs::remove_file(target).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn exact_existing_target_is_reused_without_downloading() {
    let bytes = b"verified";
    let (directory, candidate) = fixture(bytes);
    let target = directory.join(&candidate.asset_name);
    std::fs::write(&target, bytes).unwrap();
    let invoked = Cell::new(false);
    assert!(
        stage_with(&candidate, &directory, |_| {
            invoked.set(true);
            Ok(())
        })
        .is_ok()
    );
    assert!(!invoked.get());
    assert_eq!(std::fs::read(&target).unwrap(), bytes);
    std::fs::remove_file(target).unwrap();
    std::fs::remove_dir(directory).unwrap();
}

#[test]
fn racing_matching_target_is_verified_and_reused() {
    let bytes = b"verified";
    let (directory, candidate) = fixture(bytes);
    let target = directory.join(&candidate.asset_name);
    let staged = stage_with(&candidate, &directory, |file| {
        copy_verified(&bytes[..], file, &candidate, || false)?;
        std::fs::write(&target, bytes)?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        staged
            .with_verified_path_beside(&directory.join("current.exe"), |path| Ok(path.to_owned()))
            .unwrap(),
        target.canonicalize().unwrap()
    );
    assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
    std::fs::remove_file(target).unwrap();
    std::fs::remove_dir(directory).unwrap();
}
