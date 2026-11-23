use std::fs::OpenOptions;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::Path;

use super::{Fixture, reference_exists};

#[test]
fn an_owned_reference_can_exist_without_elevated_target_admission() {
    let fixture = Fixture::new();
    assert!(reference_exists(&fixture.file).unwrap());
    assert_eq!(
        std::fs::read(&fixture.file).unwrap(),
        b"fixture executable bytes"
    );
}

#[test]
fn a_missing_file_or_intermediate_component_is_confirmed_absent() {
    let fixture = Fixture::new();
    assert!(!reference_exists(&fixture.root.join("absent.exe")).unwrap());
    assert!(!reference_exists(&fixture.root.join("absent").join("child.exe")).unwrap());
    assert!(reference_exists(&fixture.file).unwrap());
}

#[test]
fn a_sharing_failure_is_unknown_presence_until_the_blocker_closes() {
    let fixture = Fixture::new();
    let blocker = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&fixture.file)
        .unwrap();
    let result = reference_exists(&fixture.file);
    assert!(
        result.is_err(),
        "blocked reference was classified as {result:?}"
    );
    drop(blocker);
    assert!(reference_exists(&fixture.file).unwrap());
}

#[test]
fn remote_and_device_references_are_rejected_before_native_access() {
    for raw in [
        r"\\server\share\target.exe",
        r"\\?\UNC\server\share\target.exe",
        r"\\.\C:\target.exe",
        r"\\.\PhysicalDrive0",
        r"\Device\HarddiskVolume1\target.exe",
        r"\\?\GLOBALROOT\Device\HarddiskVolume1\target.exe",
    ] {
        assert!(
            reference_exists(Path::new(raw)).is_err(),
            "unsupported reference was classified as local: {raw}"
        );
    }
}
