use super::*;
use sha2::{Digest, Sha256};

fn receipt_for(id: &str, bytes: &[u8]) -> ComponentReceipt {
    receipt_with_dependencies(id, bytes, Vec::new())
}

fn receipt_with_dependencies(
    id: &str,
    bytes: &[u8],
    dependencies: Vec<String>,
) -> ComponentReceipt {
    ComponentReceipt {
        schema_version: 1,
        id: id.to_string(),
        version: "1.0.0".to_string(),
        architecture: "x64".to_string(),
        dependencies,
        files: vec![super::super::OwnedComponentFile {
            path: "bin/tool.exe".into(),
            size_bytes: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }],
    }
}

#[test]
fn active_lease_defers_removal() {
    let id = "test-active-lease";
    let lease = super::super::acquire(id).unwrap();
    assert_eq!(request_remove(id).unwrap(), RemovalOutcome::Pending);
    assert!(super::super::lease::pending(id));
    assert!(super::super::acquire(id).is_err());
    drop(lease);
    assert_eq!(
        request_remove_and_wait(id).unwrap(),
        RemovalOutcome::Missing
    );
    assert!(super::super::acquire(id).is_ok());
}

#[test]
fn receipt_validation_rejects_parent_traversal() {
    let mut receipt = receipt_for("test-component", b"tool");
    receipt.files[0].path = "../tool.exe".into();
    assert!(receipt.validate().is_err());
}

#[test]
fn installed_dependent_must_be_removed_before_its_runtime() {
    let runtime_id = "test-runtime-dependency";
    let worker_id = "test-worker-dependent";
    let runtime_bytes = b"runtime";
    let worker_bytes = b"worker";

    let (runtime_root, worker_root) = {
        let _mutation = super::super::acquire_mutation_guard().unwrap();
        let _guard = lock_removal_filesystem();
        let runtime_root = super::super::ensure_version_root(runtime_id, "1.0.0").unwrap();
        let worker_root = super::super::ensure_version_root(worker_id, "1.0.0").unwrap();
        for (root, bytes) in [
            (runtime_root.as_path(), runtime_bytes.as_slice()),
            (worker_root.as_path(), worker_bytes.as_slice()),
        ] {
            std::fs::create_dir(root.join("bin")).unwrap();
            std::fs::write(root.join("bin/tool.exe"), bytes).unwrap();
        }
        super::super::write_receipt(&runtime_root, &receipt_for(runtime_id, runtime_bytes))
            .unwrap();
        super::super::write_receipt(
            &worker_root,
            &receipt_with_dependencies(worker_id, worker_bytes, vec![runtime_id.to_string()]),
        )
        .unwrap();
        (runtime_root, worker_root)
    };

    assert_eq!(
        request_remove(runtime_id).unwrap(),
        RemovalOutcome::RequiredBy(vec![worker_id.to_string()])
    );
    assert!(!super::super::lease::pending(runtime_id));
    assert!(super::super::acquire(runtime_id).is_ok());
    assert!(
        !super::super::pending::list()
            .unwrap()
            .contains(&runtime_id.to_string())
    );
    assert_eq!(request_remove(worker_id).unwrap(), RemovalOutcome::Removed);
    for _ in 0..100 {
        if !runtime_root.exists() {
            break;
        }
        let _ = request_remove(runtime_id);
        std::thread::yield_now();
    }
    assert!(!runtime_root.exists());
    assert_eq!(request_remove(runtime_id).unwrap(), RemovalOutcome::Missing);
    assert!(!worker_root.exists());
}

#[test]
fn empty_runtime_directories_do_not_block_managed_removal() {
    let id = "test-empty-runtime-directories";
    let bytes = b"worker";
    let version_root = {
        let _mutation = super::super::acquire_mutation_guard().unwrap();
        let _guard = lock_removal_filesystem();
        let root = super::super::ensure_version_root(id, "1.0.0").unwrap();
        std::fs::create_dir(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/tool.exe"), bytes).unwrap();
        std::fs::create_dir_all(root.join("bin/runtime/vendor/logs")).unwrap();
        super::super::write_receipt(&root, &receipt_for(id, bytes)).unwrap();
        root
    };

    assert_eq!(request_remove(id).unwrap(), RemovalOutcome::Removed);
    assert!(!version_root.exists());
}

#[test]
fn unknown_runtime_files_are_still_preserved() {
    let id = "test-unknown-runtime-file";
    let bytes = b"worker";
    let unknown = {
        let _mutation = super::super::acquire_mutation_guard().unwrap();
        let _guard = lock_removal_filesystem();
        let root = super::super::ensure_version_root(id, "1.0.0").unwrap();
        std::fs::create_dir(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/tool.exe"), bytes).unwrap();
        let unknown = root.join("bin/runtime/output.log");
        std::fs::create_dir_all(unknown.parent().unwrap()).unwrap();
        std::fs::write(&unknown, b"runtime output").unwrap();
        super::super::write_receipt(&root, &receipt_for(id, bytes)).unwrap();
        unknown
    };

    let RemovalOutcome::PreservedModified(paths) = request_remove(id).unwrap() else {
        panic!("unknown runtime file should block managed removal");
    };
    assert!(unknown.exists());
    assert!(paths.iter().any(|path| unknown.starts_with(path)));
    std::fs::remove_file(&unknown).unwrap();
    assert_eq!(request_remove(id).unwrap(), RemovalOutcome::Removed);
}

#[test]
#[ignore = "mutates the isolated process component test root"]
fn isolated_pending_replay_and_clean_all_delete_recorded_bytes_and_preserve_unknowns() {
    let pending_id = "test-pending-replay";
    let owned_id = "test-clean-all-owned";
    let preserved_id = "test-clean-all-preserved";
    let original = b"managed";

    let (pending_root, owned_root, preserved_root, modified, unknown) = {
        let _mutation = super::super::acquire_mutation_guard().unwrap();
        let _guard = lock_removal_filesystem();
        let mut roots = Vec::new();
        for id in [pending_id, owned_id, preserved_id] {
            let root = super::super::ensure_version_root(id, "1.0.0").unwrap();
            std::fs::create_dir(root.join("bin")).unwrap();
            std::fs::write(root.join("bin/tool.exe"), original).unwrap();
            super::super::write_receipt(&root, &receipt_for(id, original)).unwrap();
            roots.push(root);
        }
        let modified = roots[2].join("bin/tool.exe");
        std::fs::write(&modified, b"user-modified").unwrap();
        let unknown = roots[2].join("notes.txt");
        std::fs::write(&unknown, b"user-owned").unwrap();
        (
            roots.remove(0),
            roots.remove(0),
            roots.remove(0),
            modified,
            unknown,
        )
    };

    super::super::pending::record(pending_id).unwrap();
    let replayed = resume_pending().unwrap();
    assert_eq!(
        replayed,
        vec![(pending_id.to_string(), RemovalOutcome::Removed)]
    );
    assert!(!pending_root.exists());
    assert!(
        !super::super::pending::list()
            .unwrap()
            .contains(&pending_id.to_string())
    );

    let excluded = clean_all(&[owned_id]).unwrap();
    assert!(excluded.iter().all(|(id, _)| id != owned_id));
    assert!(owned_root.join("bin/tool.exe").exists());
    let outcomes = clean_all(&[]).unwrap();
    assert!(outcomes.contains(&(owned_id.to_string(), RemovalOutcome::Removed)));
    assert!(outcomes.iter().any(|(id, outcome)| {
        id == preserved_id && matches!(outcome, RemovalOutcome::PreservedModified(_))
    }));
    assert!(!owned_root.exists());
    assert!(preserved_root.exists());
    assert_eq!(std::fs::read(&modified).unwrap(), b"user-modified");
    assert_eq!(std::fs::read(&unknown).unwrap(), b"user-owned");

    std::fs::remove_file(unknown).unwrap();
    std::fs::write(&modified, original).unwrap();
    assert_eq!(
        request_remove(preserved_id).unwrap(),
        RemovalOutcome::Removed
    );
    assert!(!preserved_root.exists());
}

#[test]
fn unrelated_registry_state_does_not_block_dependency_checks() {
    let root = std::env::temp_dir().join(format!(
        "sgt-dependents-under-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("desktop.ini"), b"[.ShellClassInfo]").unwrap();
    std::fs::create_dir_all(root.join("Not A Component").join("1.0.0")).unwrap();
    for unreadable in ["test-newer-schema", "screen-text-detector"] {
        let version = root.join(unreadable).join("1.0.0");
        std::fs::create_dir_all(&version).unwrap();
        std::fs::write(version.join(RECEIPT_NAME), br#"{"schemaVersion":2}"#).unwrap();
    }
    let dependent = root.join("test-declared-dependent").join("1.0.0");
    super::super::write_receipt(
        &dependent,
        &receipt_with_dependencies(
            "test-declared-dependent",
            b"worker",
            vec!["test-runtime".to_string()],
        ),
    )
    .unwrap();

    let broken_sibling = root.join("test-declared-dependent").join("0.9.0");
    std::fs::create_dir_all(&broken_sibling).unwrap();
    std::fs::write(broken_sibling.join(RECEIPT_NAME), b"not json").unwrap();
    assert_eq!(
        dependents_under(&root, "test-runtime").unwrap(),
        vec!["test-declared-dependent".to_string()],
        "a readable version's dependency survives an unreadable sibling version"
    );
    assert_eq!(
        dependents_under(&root, "test-unrelated").unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(
        dependents_under(&root, "onnx-directml-runtime").unwrap(),
        vec!["screen-text-detector".to_string()],
        "an unreadable component still blocks what the catalog says it needs"
    );
    std::fs::remove_dir_all(&root).unwrap();
}
