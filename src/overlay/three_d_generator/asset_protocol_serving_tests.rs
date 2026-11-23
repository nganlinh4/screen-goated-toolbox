use super::*;

#[test]
fn issued_preview_validates_current_bytes_once_and_revokes_stale_grants() {
    let name = format!(
        "{}::issued_preview_validates_current_bytes_once_and_revokes_stale_grants",
        module_path!().split_once("::").unwrap().1
    );
    crate::overlay::creation_submission_test::run_isolated(&name, |root| {
        let path = root.join("result.glb");
        let geometry = br#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}]}"#;
        let valid = super::tests::glb_bytes(geometry, &[0; 36]);
        std::fs::write(&path, &valid).unwrap();
        let active = AtomicBool::new(true);
        let issue = || issue_with(path.to_str().unwrap(), &active, |_| true);
        let fetch = |grant: &Value| {
            handle(
                Request::builder()
                    .uri(grant["url"].as_str().unwrap())
                    .body(vec![])
                    .unwrap(),
            )
        };
        assert!(issue_with(path.to_str().unwrap(), &active, |_| false).is_err());

        let grant = issue().unwrap();
        let served = fetch(&grant);
        assert_eq!(served.status(), 200);
        assert_eq!(served.body().as_ref(), valid);
        assert_eq!(fetch(&grant).status(), 404, "preview grants are single use");

        let changed = issue().unwrap();
        let mut invalid = valid.clone();
        invalid[0] = b'x';
        std::fs::write(&path, &invalid).unwrap();
        assert_eq!(
            fetch(&changed).status(),
            404,
            "same-size substitution must be validated"
        );
        let malformed = issue().unwrap();
        assert_eq!(
            fetch(&malformed).status(),
            404,
            "authorization must never bypass GLB validation"
        );

        std::fs::write(&path, &valid).unwrap();
        let cleared = issue().unwrap();
        clear();
        assert_eq!(fetch(&cleared).status(), 404);
        active.store(false, Ordering::Release);
        assert!(
            issue().is_err(),
            "closed targets cannot install a late grant"
        );
        assert!(TOKENS.lock().unwrap().is_empty());
    });
}
