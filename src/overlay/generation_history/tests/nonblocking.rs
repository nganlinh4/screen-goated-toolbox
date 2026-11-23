use super::*;
use std::time::{Duration, Instant};

#[test]
fn public_list_returns_snapshot_when_another_thread_owns_delivery() {
    const CHILD: &str = "SGT_TEST_HISTORY_NONBLOCKING_CHILD";
    if std::env::var_os(CHILD).is_some() {
        // Initializing application configuration is outside the timed path and
        // uses the isolated state root supplied by the parent test process.
        drop(crate::APP.lock().unwrap());
        let (ready_send, ready_recv) = std::sync::mpsc::channel();
        let (release_send, release_recv) = std::sync::mpsc::channel();
        let owner = std::thread::spawn(move || {
            let _delivery = crate::overlay::creation_lifecycle::acquire_delivery().unwrap();
            ready_send.send(()).unwrap();
            release_recv.recv_timeout(Duration::from_secs(15)).unwrap();
        });
        ready_recv.recv_timeout(Duration::from_secs(10)).unwrap();
        let started = Instant::now();
        let entries = list("svg").unwrap();
        let elapsed = started.elapsed();
        release_send.send(()).unwrap();
        owner.join().unwrap();
        assert!(entries.is_empty());
        assert!(
            elapsed < Duration::from_secs(2),
            "history list waited {elapsed:?} for delivery"
        );
        return;
    }
    let root = std::env::temp_dir().join(format!(
        "sgt-history-nonblocking-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let temp = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    assert!(root.starts_with(&temp) && root != temp);
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("overlay::generation_history::tests::nonblocking::public_list_returns_snapshot_when_another_thread_owns_delivery")
        .env(CHILD, "1")
        .env(crate::paths::RUNTIME_STATE_ROOT_ENV, &root)
        .output().unwrap();
    assert!(
        output.status.success(),
        "isolated history test failed: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(root).unwrap();
}
