use super::*;

const MAX_PINNED_SOURCE_BYTES: u64 = 32 * 1024 * 1024;

fn assert_pin(id: &str, pin: PinnedBytes) {
    assert!(
        pin.size > 0 && pin.size <= MAX_PINNED_SOURCE_BYTES,
        "{id}: pinned size {} is outside the download bound",
        pin.size
    );
    assert!(
        pin.sha256.len() == 64
            && pin
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{id}: SHA-256 must be 64 lowercase hex digits"
    );
}

fn assert_immutable_https(id: &str, url: &str) {
    assert!(url.starts_with("https://"), "{id}: {url} is not HTTPS");
    assert!(!url.contains("?ref="), "{id}: {url} follows a branch");
}

#[test]
fn every_cursor_source_is_pinned_by_size_and_sha256() {
    for spec in collections() {
        match spec.source {
            CollectionSource::Github(source) => {
                assert_immutable_https(spec.id, source.raw_root);
                let commit = source.raw_root.rsplit('/').next().unwrap_or_default();
                assert!(
                    commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
                    "{}: GitHub source must name a full commit, not a branch",
                    spec.id
                );
                for (index, (dir, _, _)) in source.files.iter().enumerate() {
                    assert!(*dir < source.dirs.len(), "{}: bad dir index", spec.id);
                    assert_pin(spec.id, source.file_pin(index));
                    let url =
                        source.file_url(index, source_name_for_file(*spec, REQUIRED_FILES[index]));
                    assert!(!url.contains(' '), "{}: {url} is not URL-encoded", spec.id);
                }
            }
            CollectionSource::ZipArchive { url, pin, .. }
            | CollectionSource::RarArchive { url, pin, .. } => {
                assert_immutable_https(spec.id, url);
                assert_pin(spec.id, pin);
            }
        }
    }
}

#[test]
fn collections_sharing_an_archive_share_one_pin() {
    let mut pins = std::collections::HashMap::new();
    for spec in collections() {
        if let CollectionSource::ZipArchive { url, pin, .. }
        | CollectionSource::RarArchive { url, pin, .. } = spec.source
        {
            assert_eq!(*pins.entry(url).or_insert(pin), pin, "{url} has two pins");
        }
    }
}

#[test]
fn github_file_urls_encode_source_names_under_the_pinned_commit() {
    let spec = collections()
        .iter()
        .find(|spec| spec.id == "antiden-sierra-shadow")
        .expect("catalog keeps the shadowed Sierra pack");
    let CollectionSource::Github(source) = spec.source else {
        panic!("Sierra pack is a GitHub source");
    };
    let index = REQUIRED_FILES
        .iter()
        .position(|name| *name == "vert.cur")
        .unwrap();
    let url = source.file_url(index, source_name_for_file(*spec, "vert.cur"));
    assert!(url.starts_with(source.raw_root));
    assert!(url.ends_with("/1.%20Normal/Vertical%20Resize.cur"), "{url}");
}

#[test]
fn verified_store_never_reuses_the_unpinned_legacy_folder() {
    let root = Path::new("cache");
    for spec in collections() {
        assert_ne!(spec.local_dir(root), spec.legacy_dir(root));
        assert!(
            spec.local_dir(root)
                .starts_with(root.join(VERIFIED_PACKS_DIR))
        );
    }
}
