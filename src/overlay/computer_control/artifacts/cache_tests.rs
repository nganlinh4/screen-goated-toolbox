use super::*;

fn lifecycle_fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/parity-fixtures/phone-control/artifact-lifecycle.json"
    )))
    .unwrap()
}

#[test]
fn shared_artifact_contract_keeps_limits_and_session_ids_aligned() {
    let fixture = lifecycle_fixture();
    assert_eq!(
        fixture["limits"]["maxRetainedBytes"],
        session_storage::MAX_BYTES
    );
    assert_eq!(fixture["limits"]["maxEntries"], session_storage::MAX_ITEMS);
    let case = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["operation"] == "replace_session")
        .unwrap();
    let text = case["value"].as_str().unwrap();
    let old_id = {
        let _session = super::super::session_retention::start_session();
        create_text("test", None, None, text).unwrap().id
    };
    let _next = super::super::session_retention::start_session();
    let fresh = create_text("test", None, None, text).unwrap();
    assert_eq!(
        load_text(&old_id).is_ok(),
        case["oldIdResolves"].as_bool().unwrap()
    );
    assert_eq!(
        old_id == fresh.id,
        case["newIdEqualsOld"].as_bool().unwrap()
    );
}

fn fixture(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = crate::paths::app_temp_dir();
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join(format!(
        "sgt-artifact-{name}-{}-{nonce}",
        std::process::id()
    ));
    assert!(!path.exists());
    path
}

#[test]
fn ordinary_artifacts_retire_with_their_session_and_cannot_be_imported_by_another() {
    let (id, path) = {
        let _session = super::super::session_retention::start_session();
        let artifact = create_text("test", None, None, "owned exact content").unwrap();
        let id = artifact.id.clone();
        let path = artifact.path.clone();
        assert!(path.exists());
        assert_eq!(
            load_text(path.to_str().unwrap()).unwrap().1,
            "owned exact content"
        );
        {
            let _other = super::super::session_retention::start_session();
            assert!(load_text(&id).is_err());
            assert!(load_text(path.to_str().unwrap()).is_err());
        }
        (id, path)
    };
    assert!(!store().lock().contains_key(&id));
    assert!(!path.exists());
}

#[test]
fn explicit_export_survives_cache_retirement() {
    let target = fixture("export.txt");
    let cached = {
        let _session = super::super::session_retention::start_session();
        let artifact = create_text("test", None, None, "exported exact bytes").unwrap();
        assert_eq!(
            save_tool(artifact.id(), target.to_str(), false)["effect_verified"],
            true
        );
        artifact.path.clone()
    };
    assert!(!cached.exists());
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        "exported exact bytes"
    );
    std::fs::remove_file(target).unwrap();
}

#[test]
fn cache_creation_never_replaces_an_existing_file() {
    let path = fixture("collision.txt");
    std::fs::write(&path, "preserve existing").unwrap();
    assert!(cache_file::CacheFile::create(&path, "new bytes").is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "preserve existing");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn cache_cleanup_follows_its_handle_after_a_path_is_replaced() {
    let path = fixture("original.txt");
    let renamed = path.with_extension("renamed.txt");
    let cache = cache_file::CacheFile::create(&path, "owned bytes").unwrap();
    assert!(std::fs::write(&path, "overwrite").is_err());
    std::fs::rename(&path, &renamed).unwrap();
    std::fs::write(&path, "preserve replacement").unwrap();
    assert_eq!(cache.read(64).unwrap(), "owned bytes");
    drop(cache);
    assert!(!renamed.exists());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "preserve replacement"
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn bounded_import_rejects_oversized_nontext_and_busy_sources() {
    let path = fixture("bounded.txt");
    std::fs::write(&path, vec![b'x'; 65]).unwrap();
    assert!(cache_file::read_path(&path, 64).is_err());
    std::fs::write(&path, b"\xff").unwrap();
    assert!(cache_file::read_path(&path, 64).is_err());
    std::fs::write(&path, b"exact text").unwrap();
    let writer = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    assert!(cache_file::read_path(&path, 64).is_err());
    drop(writer);
    assert_eq!(cache_file::read_path(&path, 64).unwrap(), "exact text");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn cached_source_metadata_removes_credentials_queries_fragments_and_opaque_content() {
    assert_eq!(
        source_label(Some(
            "https://user:secret@example.test/path?token=private#private"
        )),
        "https://example.test/path"
    );
    assert_eq!(source_label(Some("data:text/plain,private")), "data:");
    assert_eq!(source_label(Some("not a URL")), "");
}

#[test]
fn imported_user_file_survives_retirement_of_its_cached_copy() {
    let source = fixture("source.txt");
    std::fs::write(&source, "user-owned source").unwrap();
    let cached = {
        let _session = super::super::session_retention::start_session();
        let (artifact, text) = load_text(source.to_str().unwrap()).unwrap();
        assert_eq!(text, "user-owned source");
        assert_ne!(artifact.path, source);
        artifact.path.clone()
    };
    assert!(!cached.exists());
    assert_eq!(
        std::fs::read_to_string(&source).unwrap(),
        "user-owned source"
    );
    std::fs::remove_file(source).unwrap();
}

#[test]
fn ordinary_sessions_have_the_same_artifact_count_bound_without_eviction() {
    let _session = super::super::session_retention::start_session();
    let mut first = None;
    for index in 0..lifecycle_fixture()["limits"]["maxEntries"]
        .as_u64()
        .unwrap()
    {
        let artifact = create_text("test", None, None, "bounded content").unwrap();
        if index == 0 {
            first = Some(artifact);
        }
    }
    assert!(create_text("test", None, None, "excess").is_err());
    assert_eq!(load_text(first.unwrap().id()).unwrap().1, "bounded content");
}
