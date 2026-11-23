use super::*;

#[test]
fn private_artifacts_remain_readable_without_automatic_disk_storage() {
    let session = super::super::session_retention::start_session();
    super::super::session_retention::current().forbid();
    let artifact = create_text("test", Some("private title"), None, "private tool result").unwrap();
    let path = artifact.path.clone();
    assert!(!path.exists());
    assert!(artifact.response("private tool result")["path"].is_null());
    assert_eq!(load_text(artifact.id()).unwrap().1, "private tool result");
    let id = artifact.id().to_string();
    {
        let _other = super::super::session_retention::start_session();
        assert!(load_text(&id).is_err());
    }
    assert_eq!(load_text(&id).unwrap().1, "private tool result");
    drop(session);
    assert!(!store().lock().contains_key(&id));
    assert!(!path.exists());
}

#[test]
fn private_artifact_can_still_be_explicitly_saved_and_verified() {
    let _session = super::super::session_retention::start_session();
    super::super::session_retention::current().forbid();
    let artifact = create_text("test", None, None, "explicit export").unwrap();
    let target = std::env::temp_dir().join(format!(
        "sgt-explicit-artifact-{}-{}.txt",
        std::process::id(),
        counter()
    ));
    assert!(!target.exists());
    let result = save_tool(artifact.id(), target.to_str(), false);
    assert_eq!(result["saved"], true, "{result}");
    assert_eq!(result["effect_verified"], true);
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "explicit export");
    std::fs::remove_file(target).unwrap();
}

#[test]
fn volatile_artifacts_have_a_session_quota_without_discarding_existing_content() {
    let _session = super::super::session_retention::start_session();
    super::super::session_retention::current().forbid();
    let mut first = None;
    for index in 0..128 {
        let artifact = create_text("test", None, None, "retained exact text").unwrap();
        if index == 0 {
            first = Some(artifact);
        }
    }
    assert!(create_text("test", None, None, "excess").is_err());
    assert_eq!(
        load_text(first.unwrap().id()).unwrap().1,
        "retained exact text"
    );
}

#[test]
fn volatile_artifact_byte_quota_includes_all_retained_content() {
    let _session = super::super::session_retention::start_session();
    super::super::session_retention::current().forbid();
    let text = "x".repeat(16 * 1024 * 1024);
    let artifact = create_text("test", None, None, &text).unwrap();
    assert!(create_text("test", None, None, &text).is_err());
    assert_eq!(info_tool(artifact.id())["byte_count"], text.len());
    assert!(!artifact.path.exists());
}
