use std::time::{Duration, Instant};

use sha2::{Digest as _, Sha256};

use super::{RemovalOutcome, resume_pending};
use crate::component_registry::{self, ComponentReceipt, OwnedComponentFile};

fn install_fixture(id: &str, dependencies: &[&str]) -> std::path::PathBuf {
    let root = component_registry::ensure_version_root(id, "1.0.0").unwrap();
    std::fs::write(root.join("component.bin"), b"owned").unwrap();
    component_registry::write_receipt(
        &root,
        &ComponentReceipt {
            schema_version: 1,
            id: id.to_string(),
            version: "1.0.0".to_string(),
            architecture: "x64".to_string(),
            dependencies: dependencies.iter().map(|id| (*id).to_string()).collect(),
            files: vec![OwnedComponentFile {
                path: "component.bin".into(),
                size_bytes: 5,
                sha256: format!("{:x}", Sha256::digest(b"owned")),
            }],
        },
    )
    .unwrap();
    root
}

#[test]
fn pending_lease_does_not_block_unrelated_startup_removal() {
    let _mutation = component_registry::acquire_mutation_guard().unwrap();
    let active = "test-pending-startup-active";
    let other = "test-pending-startup-unrelated";
    let lease = component_registry::acquire(active).unwrap();
    component_registry::pending::record(active).unwrap();
    component_registry::pending::record(other).unwrap();

    let start = Instant::now();
    let outcomes = resume_pending().unwrap();
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(
        outcomes
            .iter()
            .any(|(id, outcome)| id == active && *outcome == RemovalOutcome::Pending)
    );
    assert!(
        outcomes
            .iter()
            .any(|(id, outcome)| id == other && *outcome == RemovalOutcome::Missing)
    );
    assert!(
        component_registry::pending::list()
            .unwrap()
            .contains(&active.to_string())
    );
    assert!(
        !component_registry::pending::list()
            .unwrap()
            .contains(&other.to_string())
    );

    drop(lease);
    assert_eq!(
        super::request_remove_and_wait(active).unwrap(),
        RemovalOutcome::Missing
    );
}

#[test]
fn pending_dependency_is_retained_until_its_user_is_removed() {
    let mutation = component_registry::acquire_mutation_guard().unwrap();
    let runtime = "test-pending-startup-a-runtime";
    let worker = "test-pending-startup-b-worker";
    let runtime_root = install_fixture(runtime, &[]);
    let worker_root = install_fixture(worker, &[runtime]);
    let lease = component_registry::acquire(worker).unwrap();
    component_registry::pending::record(runtime).unwrap();
    component_registry::pending::record(worker).unwrap();

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;

        let path = std::env::temp_dir().join(format!(
            "screen-goated-toolbox-component-removals-{}.json",
            std::process::id()
        ));
        let pending_guard = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .open(path)
            .unwrap();
        assert!(matches!(
            super::retry_pending_removal(runtime).unwrap(),
            Some(RemovalOutcome::RequiredBy(_))
        ));
        drop(pending_guard);
    }

    let outcomes = resume_pending().unwrap();
    assert!(
        outcomes
            .iter()
            .any(|(id, outcome)| id == runtime && matches!(outcome, RemovalOutcome::RequiredBy(_)))
    );
    assert!(
        outcomes
            .iter()
            .any(|(id, outcome)| id == worker && *outcome == RemovalOutcome::Pending)
    );
    let pending = component_registry::pending::list().unwrap();
    assert!(pending.contains(&runtime.to_string()));
    assert!(pending.contains(&worker.to_string()));
    drop(mutation);
    drop(lease);

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let pending = component_registry::pending::list().unwrap();
        if !runtime_root.exists()
            && !worker_root.exists()
            && !pending.contains(&runtime.to_string())
            && !pending.contains(&worker.to_string())
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "pending dependencies were not reconciled"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "holds an intentionally invalid lock in the isolated process registry"]
fn failed_pending_removal_does_not_stop_later_entries() {
    let mutation = component_registry::acquire_mutation_guard().unwrap();
    let invalid = "test-pending-startup-a-invalid";
    let other = "test-pending-startup-z-valid";
    let initializer = component_registry::acquire("test-pending-startup-initializer").unwrap();
    let lock_root =
        std::env::temp_dir().join(format!("sgt-component-leases-test-{}", std::process::id()));
    let unsafe_lock = lock_root.join(format!("{invalid}.lock"));
    std::fs::create_dir(&unsafe_lock).unwrap();
    component_registry::pending::record(invalid).unwrap();
    component_registry::pending::record(other).unwrap();

    assert!(resume_pending().is_err());
    let pending = component_registry::pending::list().unwrap();
    assert!(pending.contains(&invalid.to_string()));
    assert!(!pending.contains(&other.to_string()));
    for _ in 0..10 {
        component_registry::lease::start_pending_watcher(invalid);
    }
    assert!(component_registry::lease::watcher_active(invalid));
    drop(mutation);
    std::thread::sleep(Duration::from_millis(700));
    assert!(
        component_registry::pending::list()
            .unwrap()
            .contains(&invalid.to_string())
    );
    assert!(component_registry::lease::watcher_active(invalid));
    std::fs::remove_dir(unsafe_lock).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while component_registry::pending::list()
        .unwrap()
        .contains(&invalid.to_string())
        || component_registry::lease::watcher_active(invalid)
    {
        assert!(
            Instant::now() < deadline,
            "transient failure was not retried"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(component_registry::acquire(invalid).is_ok());
    drop(initializer);
}

#[test]
#[ignore = "holds an intentionally invalid lock in the isolated process registry"]
fn foreground_failure_retries_without_an_explicit_second_request() {
    let mutation = component_registry::acquire_mutation_guard().unwrap();
    let id = "test-pending-foreground-transient";
    let initializer = component_registry::acquire("test-pending-foreground-init").unwrap();
    let lock_root =
        std::env::temp_dir().join(format!("sgt-component-leases-test-{}", std::process::id()));
    let obstruction = lock_root.join(format!("{id}.lock"));
    std::fs::create_dir(&obstruction).unwrap();
    assert!(super::request_remove(id).is_err());
    assert!(
        component_registry::pending::list()
            .unwrap()
            .contains(&id.to_string())
    );
    drop(mutation);
    std::thread::sleep(Duration::from_millis(700));
    assert!(component_registry::lease::watcher_active(id));
    std::fs::remove_dir(obstruction).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while component_registry::pending::list()
        .unwrap()
        .contains(&id.to_string())
        || component_registry::lease::watcher_active(id)
    {
        assert!(
            Instant::now() < deadline,
            "foreground removal was not retried"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(component_registry::acquire(id).is_ok());
    drop(initializer);
}

#[test]
#[ignore = "temporarily holds a file in the isolated process registry"]
fn locked_owned_file_retains_pending_until_it_can_be_removed() {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let mutation = component_registry::acquire_mutation_guard().unwrap();
    let id = "test-pending-locked-owned-file";
    let root = install_fixture(id, &[]);
    let obstruction = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .open(root.join("component.bin"))
        .unwrap();
    assert!(super::request_remove(id).is_err());
    assert!(
        component_registry::pending::list()
            .unwrap()
            .contains(&id.to_string())
    );
    drop(mutation);
    std::thread::sleep(Duration::from_millis(700));
    assert!(
        component_registry::pending::list()
            .unwrap()
            .contains(&id.to_string())
    );
    assert!(root.join("component.bin").exists());
    drop(obstruction);
    let deadline = Instant::now() + Duration::from_secs(10);
    while component_registry::pending::list()
        .unwrap()
        .contains(&id.to_string())
        || component_registry::lease::watcher_active(id)
    {
        assert!(
            Instant::now() < deadline,
            "locked file removal was not retried"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!root.exists());
}

#[test]
#[ignore = "temporarily locks the isolated process pending-removal store"]
fn inaccessible_pending_store_is_retried_after_it_becomes_available() {
    use std::os::windows::fs::OpenOptionsExt as _;

    let mutation = component_registry::acquire_mutation_guard().unwrap();
    let id = "test-pending-store-transient";
    component_registry::pending::record(id).unwrap();
    let path = std::env::temp_dir().join(format!(
        "screen-goated-toolbox-component-removals-{}.json",
        std::process::id()
    ));
    let obstruction = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .unwrap();
    assert!(super::request_remove(id).is_err());
    assert!(component_registry::lease::watcher_active(id));
    assert!(resume_pending().is_err());
    drop(mutation);
    std::thread::sleep(Duration::from_millis(700));
    assert!(component_registry::lease::watcher_active(""));
    drop(obstruction);
    let deadline = Instant::now() + Duration::from_secs(10);
    while component_registry::pending::list()
        .unwrap()
        .contains(&id.to_string())
        || component_registry::lease::watcher_active(id)
        || component_registry::lease::watcher_active("")
    {
        assert!(Instant::now() < deadline, "pending store was not retried");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(component_registry::acquire(id).is_ok());
}
