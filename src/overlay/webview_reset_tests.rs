use super::*;

struct TestRoot(PathBuf);

impl TestRoot {
    fn new() -> Self {
        let parent = std::env::temp_dir().canonicalize().unwrap();
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let path = parent.join(format!(
            "sgt-reset-test-{:016x}",
            u64::from_le_bytes(random)
        ));
        fs::create_dir(&path).unwrap();
        let resolved = path.canonicalize().unwrap();
        assert!(resolved.starts_with(&parent));
        Self(resolved)
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn removes_only_profile_and_accepts_missing_profile() {
    let root = TestRoot::new();
    let profile = root.0.join("SGT").join("webview_data");
    fs::create_dir_all(profile.join("nested")).unwrap();
    fs::write(profile.join("nested").join("file"), b"profile").unwrap();
    let sibling = root.0.join("SGT").join("keep");
    fs::write(&sibling, b"keep").unwrap();
    clear_tree(&profile).unwrap();
    assert!(!profile.exists());
    assert_eq!(fs::read(sibling).unwrap(), b"keep");
    clear_tree(&profile).unwrap();
}

#[test]
fn rejects_non_directory_and_keeps_its_contents() {
    let root = TestRoot::new();
    let profile = root.0.join("webview_data");
    fs::write(&profile, b"keep").unwrap();
    assert!(clear_tree(&profile).is_err());
    assert_eq!(fs::read(profile).unwrap(), b"keep");
}

#[test]
fn refuses_observed_reparse_points_without_following_them() {
    let root = TestRoot::new();
    let profile = root.0.join("webview_data");
    let protected = root.0.join("protected");
    fs::create_dir(&profile).unwrap();
    fs::create_dir(&protected).unwrap();
    fs::write(protected.join("sentinel"), b"keep").unwrap();
    if std::os::windows::fs::symlink_dir(&protected, profile.join("link")).is_err() {
        return; // Windows may not grant symlink creation to the test account.
    }
    assert_eq!(
        clear_tree(&profile).unwrap_err().kind(),
        io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read(protected.join("sentinel")).unwrap(), b"keep");
}

#[test]
fn opened_root_cannot_be_replaced_before_its_handle_is_deleted() {
    let root = TestRoot::new();
    let profile = root.0.join("webview_data");
    fs::create_dir(&profile).unwrap();
    fs::write(profile.join("profile"), b"owned").unwrap();
    let guarded = open_entry(&profile, true).unwrap();
    assert!(fs::rename(&profile, root.0.join("moved")).is_err());
    remove_opened(&profile, guarded, &mut 0, 0).unwrap();
    assert!(!profile.exists());
}
