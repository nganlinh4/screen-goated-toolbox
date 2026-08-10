use super::*;
const TEST_FILES: &[WebAssetFile] = &[WebAssetFile {
    path: "index.html",
    size_bytes: 4,
    sha256: "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08",
}];

fn test_delivery() -> WebAssetDelivery {
    WebAssetDelivery {
        component: WebAssetComponent::PromptDj,
        version: "test",
        asset: "test.zip",
        download_url: "https://example.invalid/test.zip",
        size_bytes: 1,
        sha256: "00",
        unpacked_size_bytes: 4,
        files: TEST_FILES,
    }
}

fn temp_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "sgt-web-assets-{label}-{}-{}",
        std::process::id(),
        INSTALL_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ))
}

fn write_zip(path: &Path, entry: &str) {
    let file = std::fs::File::create(path).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file(entry, zip::write::SimpleFileOptions::default())
        .unwrap();
    archive.write_all(b"test").unwrap();
    archive.finish().unwrap();
}

#[test]
fn supported_component_ids_are_present_in_the_signed_catalog() {
    for component in [
        WebAssetComponent::Creation3d,
        WebAssetComponent::PromptDj,
        WebAssetComponent::TtsPlayground,
    ] {
        assert!(
            super::super::embedded_catalog()
                .components
                .iter()
                .any(|entry| entry.id == component.id())
        );
    }
}

#[test]
fn release_build_without_delivery_data_fails_closed() {
    if WEB_ASSET_DELIVERIES.is_empty() {
        assert!(!is_installed(WebAssetComponent::PromptDj));
    }
}

#[test]
fn creation_web_receipt_does_not_block_runtime_removal() {
    let delivery = WebAssetDelivery {
        component: WebAssetComponent::Creation3d,
        ..test_delivery()
    };
    let web_receipt = receipt(&delivery);
    assert!(web_receipt.dependencies.is_empty());
    let catalog_entry = super::super::embedded_catalog()
        .components
        .iter()
        .find(|entry| entry.id == WebAssetComponent::Creation3d.id())
        .unwrap();
    assert!(
        !catalog_entry
            .dependencies
            .iter()
            .any(|dependency| dependency == "creation-3d-runtime")
    );
}

#[test]
fn extraction_accepts_only_manifest_owned_paths() {
    let root = temp_root("owned");
    let staging = root.join("staging");
    std::fs::create_dir_all(&staging).unwrap();
    let archive = root.join("pack.zip");
    write_zip(&archive, "index.html");
    extract_archive(&archive, &staging, &test_delivery()).unwrap();
    assert_eq!(std::fs::read(staging.join("index.html")).unwrap(), b"test");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn extraction_rejects_parent_traversal() {
    let root = temp_root("traversal");
    let staging = root.join("staging");
    std::fs::create_dir_all(&staging).unwrap();
    let archive = root.join("pack.zip");
    write_zip(&archive, "../index.html");
    assert!(extract_archive(&archive, &staging, &test_delivery()).is_err());
    assert!(!root.join("index.html").exists());
    std::fs::remove_dir_all(root).unwrap();
}
