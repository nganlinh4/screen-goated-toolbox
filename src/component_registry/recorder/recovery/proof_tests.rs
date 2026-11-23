use super::*;

static FILES: [super::super::RecorderFile; 1] = [super::super::RecorderFile {
    path: "assets/app.js",
    size_bytes: 7,
    sha256: "7fdfda5f50a433ae127a784fc143105fb6d93fedec7601ddeb3d1d584f83de05",
}];
static DELIVERY: RecorderDelivery = RecorderDelivery {
    id: super::super::WEB_ID,
    version: "5.4.3",
    asset: "unused.zip",
    download_url: "https://example.invalid/unused.zip",
    size_bytes: 1,
    sha256: "00",
    unpacked_size_bytes: 7,
    files: &FILES,
};

#[test]
fn missing_invalid_or_mismatched_receipt_preserves_recorded_paths() {
    let root = std::env::temp_dir().join(format!(
        "sgt-recorder-proof-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(root.join("assets/app.js"), b"managed").unwrap();
    let mut mismatched = super::super::receipt(&DELIVERY);
    mismatched.version = "5.4.4".into();
    for body in [
        None,
        Some(b"invalid".to_vec()),
        Some(serde_json::to_vec(&mismatched).unwrap()),
    ] {
        if let Some(body) = body {
            std::fs::write(root.join(RECEIPT_NAME), body).unwrap();
        }
        let record = snapshot(&DELIVERY, &root, "unused", "invalid receipt").unwrap();
        assert!(
            record
                .files
                .iter()
                .all(|file| !file.cleanable && !file.declared)
        );
        assert_eq!(
            std::fs::read(root.join("assets/app.js")).unwrap(),
            b"managed"
        );
    }
    std::fs::remove_file(root.join(RECEIPT_NAME)).unwrap();
    std::fs::remove_file(root.join("assets/app.js")).unwrap();
    std::fs::remove_dir(root.join("assets")).unwrap();
    std::fs::remove_dir(root).unwrap();
}
