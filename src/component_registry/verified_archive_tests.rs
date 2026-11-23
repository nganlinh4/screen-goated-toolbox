use super::*;
use sha2::{Digest as _, Sha256};
use std::io::Cursor;

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (path, bytes) in files {
        writer
            .start_file(*path, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn run(bytes: Vec<u8>, files: &[FileSpec<'_>], policy: Policy) -> Result<Vec<Vec<u8>>> {
    let mut outputs = vec![Vec::new(); files.len()];
    extract(
        Cursor::new(bytes),
        files,
        policy,
        &AtomicBool::new(false),
        |index, member| member.copy_to(&mut outputs[index], &|_| {}),
    )?;
    Ok(outputs)
}

#[test]
fn maps_members_by_inventory_instead_of_archive_order() {
    let a = digest(b"first");
    let b = digest(b"second");
    let files = [
        FileSpec::mapped(Path::new("src/a"), Path::new("bin/a"), 5, &a),
        FileSpec::same(Path::new("b"), 6, &b),
    ];
    let output = run(
        zip(&[("b", b"second"), ("src/a", b"first")]),
        &files,
        Policy::files(8, 11),
    )
    .unwrap();
    assert_eq!(output, [b"first".to_vec(), b"second".to_vec()]);
}

#[test]
fn checks_the_whole_namespace_before_any_output() {
    let hash = digest(b"x");
    let files = [FileSpec::same(Path::new("a"), 1, &hash)];
    for name in [
        "A",
        "unexpected",
        "../outside",
        "C:/outside",
        "a:b",
        "a.",
        "a ",
        "a\\b",
        "NUL",
        "a/../b",
    ] {
        let mut calls = 0;
        let policy = Policy {
            max_entries: 8,
            size: 1,
            directories: Directories::Empty,
        };
        assert!(
            extract(
                Cursor::new(zip(&[("a", b"x"), (name, b"x")])),
                &files,
                policy,
                &AtomicBool::new(false),
                |_, _| {
                    calls += 1;
                    Ok(())
                }
            )
            .is_err(),
            "{name}"
        );
        assert_eq!(calls, 0, "{name}");
    }
}

#[test]
fn rejects_inventory_aliases_parent_collisions_and_incorrect_limits() {
    let hash = digest(b"x");
    for (a, b, mapped) in [
        ("A", "a", false),
        ("\u{e9}", "\u{c9}", false),
        ("a", "a/b", false),
        ("a", "a/b", true),
        ("A", "a", true),
    ] {
        let files = if mapped {
            [
                FileSpec::mapped(Path::new("one"), Path::new(a), 1, &hash),
                FileSpec::mapped(Path::new("two"), Path::new(b), 1, &hash),
            ]
        } else {
            [
                FileSpec::same(Path::new(a), 1, &hash),
                FileSpec::same(Path::new(b), 1, &hash),
            ]
        };
        assert!(
            run(
                zip(&[("one", b"x"), ("two", b"x")]),
                &files,
                Policy::files(8, 2)
            )
            .is_err()
        );
    }
    let file = [FileSpec::same(Path::new("a"), 1, &hash)];
    assert!(run(zip(&[("a", b"x")]), &file, Policy::files(8, 2)).is_err());
    assert!(run(zip(&[("a", b"x")]), &file, Policy::files(0, 1)).is_err());
    let receipt = [FileSpec::same(Path::new("receipt.json"), 1, &hash)];
    assert!(
        run(
            zip(&[("receipt.json", b"x")]),
            &receipt,
            Policy::files(8, 1)
        )
        .is_err()
    );
    let malformed = [FileSpec::same(Path::new("a"), 1, "not-a-content-pin")];
    assert!(run(zip(&[("a", b"x")]), &malformed, Policy::files(8, 1)).is_err());
    let overflow = [
        FileSpec::same(Path::new("a"), u64::MAX, &hash),
        FileSpec::same(Path::new("b"), 1, &hash),
    ];
    assert!(
        run(
            zip(&[("a", b"x"), ("b", b"x")]),
            &overflow,
            Policy::files(8, u64::MAX)
        )
        .is_err()
    );
}

#[test]
fn empty_directories_are_explicit_and_cannot_alias_files() {
    let hash = digest(b"x");
    let files = [FileSpec::same(Path::new("a/file"), 1, &hash)];
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_directory("a/", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer
        .start_file("a/file", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"x").unwrap();
    let archive = writer.finish().unwrap().into_inner();
    assert!(run(archive.clone(), &files, Policy::files(8, 1)).is_err());
    run(
        archive,
        &files,
        Policy {
            max_entries: 8,
            size: 1,
            directories: Directories::Empty,
        },
    )
    .unwrap();
    let files = [FileSpec::same(Path::new("a"), 1, &hash)];
    for directory in ["a/", "a/child/", "A/child/"] {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("a", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"x").unwrap();
        writer
            .add_directory(directory, zip::write::SimpleFileOptions::default())
            .unwrap();
        assert!(
            run(
                writer.finish().unwrap().into_inner(),
                &files,
                Policy {
                    max_entries: 8,
                    size: 1,
                    directories: Directories::Empty
                }
            )
            .is_err()
        );
    }
}

#[test]
fn rejects_links_special_entries_and_non_utf8_paths() {
    let hash = digest(b"x");
    let files = [FileSpec::same(Path::new("a"), 1, &hash)];
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_symlink("a", "x", zip::write::SimpleFileOptions::default())
        .unwrap();
    assert!(
        run(
            writer.finish().unwrap().into_inner(),
            &files,
            Policy::files(8, 1)
        )
        .is_err()
    );
    for attributes in [(0o010600_u32 << 16), (0o100600_u32 << 16) | 0x400] {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "a",
                zip::write::SimpleFileOptions::default()
                    .system(zip::System::Unix)
                    .external_attributes(attributes),
            )
            .unwrap();
        writer.write_all(b"x").unwrap();
        assert!(
            run(
                writer.finish().unwrap().into_inner(),
                &files,
                Policy::files(8, 1)
            )
            .is_err()
        );
    }
    let mut bytes = zip(&[("a", b"x")]);
    let central = bytes.windows(4).position(|s| s == b"PK\x01\x02").unwrap();
    bytes[30] = 0xff;
    bytes[central + 46] = 0xff;
    assert!(run(bytes, &files, Policy::files(8, 1)).is_err());
    let mut bytes = zip(&[("a", b"x")]);
    let central = bytes.windows(4).position(|s| s == b"PK\x01\x02").unwrap();
    bytes[6] |= 1;
    bytes[central + 8] |= 1;
    assert!(run(bytes, &files, Policy::files(8, 1)).is_err());
}

#[test]
fn streaming_copy_bounds_reads_and_retries_interrupted_input() {
    struct InterruptOnce(Cursor<Vec<u8>>, bool);
    impl Read for InterruptOnce {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            if !self.1 {
                self.1 = true;
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            self.0.read(output)
        }
    }
    let cancel = AtomicBool::new(false);
    let hash = digest(b"x");
    let mut output = Vec::new();
    verified_copy::copy_verified(
        InterruptOnce(Cursor::new(b"x".to_vec()), false),
        &mut output,
        1,
        &hash,
        &cancel,
        &|_| {},
    )
    .unwrap();
    assert_eq!(output, b"x");
    output.clear();
    assert!(
        verified_copy::copy_verified(Cursor::new(b"xx"), &mut output, 1, &hash, &cancel, &|_| {})
            .is_err()
    );
    assert!(output.is_empty());
    assert!(
        verified_copy::copy_verified(Cursor::new(b""), &mut output, 1, &hash, &cancel, &|_| {})
            .is_err()
    );
}

#[test]
fn requires_verified_content_and_rejects_truncation_hash_and_size_mismatch() {
    let hash = digest(b"x");
    let files = [FileSpec::same(Path::new("a"), 1, &hash)];
    assert!(run(zip(&[("a", b"y")]), &files, Policy::files(8, 1)).is_err());
    assert!(run(zip(&[("a", b"xy")]), &files, Policy::files(8, 1)).is_err());
    let mut bytes = zip(&[("a", b"x")]);
    bytes.truncate(bytes.len() - 12);
    assert!(run(bytes, &files, Policy::files(8, 1)).is_err());
    assert!(
        extract(
            Cursor::new(zip(&[("a", b"x")])),
            &files,
            Policy::files(8, 1),
            &AtomicBool::new(false),
            |_, _| Ok(())
        )
        .is_err()
    );
}

#[test]
fn cancellation_stops_before_output_and_between_copy_chunks() {
    let body = vec![7; 600_000];
    let hash = digest(&body);
    let files = [FileSpec::same(Path::new("a"), body.len() as u64, &hash)];
    let cancel = AtomicBool::new(true);
    let mut calls = 0;
    assert!(
        extract(
            Cursor::new(zip(&[("a", &body)])),
            &files,
            Policy::files(8, body.len() as u64),
            &cancel,
            |_, _| {
                calls += 1;
                Ok(())
            }
        )
        .is_err()
    );
    assert_eq!(calls, 0);
    cancel.store(false, Ordering::Release);
    let mut output = Vec::new();
    assert!(
        extract(
            Cursor::new(zip(&[("a", &body)])),
            &files,
            Policy::files(8, body.len() as u64),
            &cancel,
            |_, member| member.copy_to(&mut output, &|_| cancel.store(true, Ordering::Release))
        )
        .is_err()
    );
    assert!(!output.is_empty() && output.len() <= 256 * 1024);
}

#[test]
fn resume_rehashes_the_existing_regular_file_before_skipping_archive_content() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "sgt-archive-resume-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    let target = root.join("a");
    let hash = digest(b"x");
    let files = [FileSpec::same(Path::new("a"), 1, &hash)];
    extract(
        Cursor::new(zip(&[("a", b"x")])),
        &files,
        Policy::files(8, 1),
        &AtomicBool::new(false),
        |_, member| {
            assert!(!member.retain_verified(&target)?);
            member.copy_to(&mut std::io::sink(), &|_| {})
        },
    )
    .unwrap();
    for bytes in [b"y", b"x"] {
        std::fs::write(&target, bytes).unwrap();
        let mut retained = false;
        extract(
            Cursor::new(zip(&[("a", b"x")])),
            &files,
            Policy::files(8, 1),
            &AtomicBool::new(false),
            |_, member| {
                retained = member.retain_verified(&target)?;
                if !retained {
                    member.copy_to(&mut std::io::sink(), &|_| {})?;
                }
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(retained, bytes == b"x");
    }
    std::fs::remove_file(target).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
#[ignore = "requires explicitly provided delivery inventory fixtures"]
fn provided_inventories_match_archive_namespace_policy() {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Inventory {
        installed_size_bytes: u64,
        files: Vec<crate::component_registry::receipt::OwnedComponentFile>,
    }
    let selection = std::env::var_os("SGT_TEST_ARCHIVE_INVENTORIES")
        .expect("explicit inventory fixtures are required");
    let paths = std::env::split_paths(&selection).collect::<Vec<_>>();
    assert!(!paths.is_empty());
    for path in paths {
        let file = super::super::native_file::open_regular(&path).unwrap();
        assert!(file.metadata().unwrap().len() <= 32 * 1024 * 1024);
        let document: Inventory = serde_json::from_reader(file).unwrap();
        let files = document
            .files
            .iter()
            .map(|file| FileSpec::same(&file.path, file.size_bytes, &file.sha256))
            .collect::<Vec<_>>();
        let policy = Policy {
            max_entries: 200_000,
            size: document.installed_size_bytes,
            directories: Directories::Empty,
        };
        validate_inventory(&files, &policy).unwrap();
        eprintln!("validated {} declared payload files", files.len());
    }
}
