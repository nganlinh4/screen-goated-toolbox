use super::*;
use std::time::Duration;

/// A disposable HKCU subtree standing in for the user's registry root.
struct TestRoot {
    path: String,
    key: RegKey,
}

impl TestRoot {
    fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = format!(
            "Software\\SGT-registry-integration-test-{label}-{}-{nanos}",
            std::process::id()
        );
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(&path)
            .unwrap();
        Self { path, key }
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.path);
    }
}

fn verb_path(extension: &str) -> String {
    build_shell_verb_path(
        SYSTEM_FILE_ASSOCIATIONS_ROOT,
        extension,
        PROCESS_WITH_SGT_VERB,
    )
}

fn last_write(root: &RegKey, path: &str) -> (u32, u32) {
    let info = root.open_subkey(path).unwrap().query_info().unwrap();
    (
        info.last_write_time.dwLowDateTime,
        info.last_write_time.dwHighDateTime,
    )
}

#[test]
fn legacy_locations_are_removed_and_current_verbs_registered() {
    let root = TestRoot::new("legacy");
    let legacy = [
        build_shell_verb_path(CLASSES_ROOT, "*", PROCESS_WITH_SGT_VERB),
        build_shell_verb_path(
            SYSTEM_FILE_ASSOCIATIONS_ROOT,
            "image",
            LEGACY_PROCESS_WITH_SGT_VERB,
        ),
        build_shell_verb_path(
            SYSTEM_FILE_ASSOCIATIONS_ROOT,
            ".png",
            LEGACY_PROCESS_WITH_SGT_VERB,
        ),
        build_shell_verb_path(CLASSES_ROOT, ".txt", PROCESS_WITH_SGT_VERB),
    ];
    for path in &legacy {
        root.key.create_subkey(path).unwrap();
    }

    ensure_context_menu_entries(&root.key, r"C:\Apps\SGT.exe");

    for path in &legacy {
        assert!(root.key.open_subkey(path).is_err(), "{path} survived");
    }
    for extension in supported_extensions() {
        let key = root.key.open_subkey(verb_path(extension)).unwrap();
        assert_eq!(
            key.get_value::<String, _>("").unwrap(),
            PROCESS_WITH_SGT_LABEL
        );
        assert_eq!(
            key.open_subkey("command")
                .unwrap()
                .get_value::<String, _>("")
                .unwrap(),
            r#""C:\Apps\SGT.exe" --process-with-sgt "%1""#
        );
    }
}

#[test]
fn an_up_to_date_registration_is_left_untouched() {
    let root = TestRoot::new("steady");
    ensure_context_menu_entries(&root.key, r"C:\Apps\SGT.exe");
    let shell = build_shell_verb_path(SYSTEM_FILE_ASSOCIATIONS_ROOT, ".png", "")
        .trim_end_matches('\\')
        .to_string();
    let verb = verb_path(".png");
    let command = format!("{verb}\\command");
    let before = [&shell, &verb, &command].map(|path| last_write(&root.key, path));

    // Registry timestamps tick coarsely; leave room for any rewrite to show.
    std::thread::sleep(Duration::from_millis(50));
    ensure_context_menu_entries(&root.key, r"C:\Apps\SGT.exe");

    let after = [&shell, &verb, &command].map(|path| last_write(&root.key, path));
    assert_eq!(before, after, "a steady startup rewrote the context menu");
}

#[test]
fn a_moved_executable_updates_only_the_changed_values() {
    let root = TestRoot::new("moved");
    ensure_context_menu_entries(&root.key, r"C:\Old\SGT.exe");
    ensure_context_menu_entries(&root.key, r"D:\New\SGT.exe");
    let key = root.key.open_subkey(verb_path(".md")).unwrap();
    assert_eq!(
        key.get_value::<String, _>("Icon").unwrap(),
        r"D:\New\SGT.exe"
    );
    assert_eq!(
        key.open_subkey("command")
            .unwrap()
            .get_value::<String, _>("")
            .unwrap(),
        r#""D:\New\SGT.exe" --process-with-sgt "%1""#
    );
}
