use std::fs;
use std::process::Command;

#[test]
fn source_assets_require_an_explicit_debug_root() {
    const CHILD_MARKER: &str = "SGT_TEST_RECORDER_SOURCE_CHILD";
    if let Some(mode) = std::env::var_os(CHILD_MARKER) {
        let root = super::repo_root();
        if mode == "explicit" {
            assert_eq!(
                root,
                Some(std::env::current_dir().unwrap().canonicalize().unwrap())
            );
        } else {
            assert!(root.is_none());
        }
        return;
    }
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).unwrap();
    let suffix = nonce
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let root = std::env::temp_dir().join(format!("sgt-cursor-source-test-{suffix}"));
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("screen-record")).unwrap();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("Cargo.toml"), "").unwrap();
    for mode in ["ambient", "explicit"] {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child.args(["--exact", "overlay::screen_record::worker_source_tests::source_assets_require_an_explicit_debug_root"])
            .current_dir(&root)
            .env(CHILD_MARKER, mode)
            .env_remove("SGT_RECORDER_DEV_ASSET_ROOT");
        if mode == "explicit" {
            child.env("SGT_RECORDER_DEV_ASSET_ROOT", &root);
        }
        assert!(child.status().unwrap().success());
    }
    fs::remove_file(root.join("Cargo.toml")).unwrap();
    fs::remove_dir(root.join("screen-record")).unwrap();
    fs::remove_dir(root.join("src")).unwrap();
    fs::remove_dir(root).unwrap();
}
