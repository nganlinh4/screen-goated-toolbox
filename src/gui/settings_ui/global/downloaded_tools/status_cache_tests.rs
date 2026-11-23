use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

#[test]
fn blocked_probe_does_not_block_caller_or_spawn_replacement() {
    let cache = StatusCache::new();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (wake_tx, wake_rx) = mpsc::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let first_calls = calls.clone();
    assert!(
        cache
            .get(
                Duration::from_secs(10),
                move || {
                    first_calls.fetch_add(1, Ordering::SeqCst);
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    1
                },
                move || {
                    let _ = wake_tx.send(());
                }
            )
            .is_none()
    );
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    cache.invalidate();
    let next_calls = calls.clone();
    assert!(
        cache
            .get(
                Duration::from_secs(10),
                move || {
                    next_calls.fetch_add(1, Ordering::SeqCst);
                    2
                },
                || {}
            )
            .is_none()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    release_tx.send(()).unwrap();
    wake_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    let (tx, rx) = mpsc::channel();
    assert!(
        cache
            .get(
                Duration::from_secs(10),
                || 2,
                move || {
                    let _ = tx.send(());
                }
            )
            .is_none()
    );
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(cache.get(Duration::from_secs(10), || 3, || {}), Some(2));
}

#[test]
fn panicking_probe_releases_reservation_without_fabricating_status() {
    let cache = StatusCache::<u32>::new();
    let (tx, rx) = mpsc::channel();
    assert!(
        cache
            .get(
                Duration::from_secs(10),
                || panic!("injected failure"),
                move || {
                    let _ = tx.send(());
                }
            )
            .is_none()
    );
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    {
        let mut state = cache.state.lock().unwrap();
        assert!(!state.running);
        assert!(state.value.is_none());
        state.expires = Instant::now();
    }
    let (tx, rx) = mpsc::channel();
    assert!(
        cache
            .get(
                Duration::from_secs(10),
                || 7,
                move || {
                    let _ = tx.send(());
                }
            )
            .is_none()
    );
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(cache.get(Duration::from_secs(10), || 8, || {}), Some(7));
}

#[test]
fn invalidation_discards_a_completed_snapshot() {
    let cache = StatusCache::new();
    let (tx, rx) = mpsc::channel();
    cache.get(
        Duration::from_secs(10),
        || 9,
        move || {
            let _ = tx.send(());
        },
    );
    rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert_eq!(cache.get(Duration::from_secs(10), || 0, || {}), Some(9));
    cache.invalidate();
    assert!(cache.get(Duration::from_secs(10), || 10, || {}).is_none());
}

#[test]
fn wake_panic_during_probe_unwind_is_contained() {
    const CHILD_ENV: &str = "SGT_STATUS_CACHE_UNWIND_CHILD";
    if std::env::var_os(CHILD_ENV).is_some() {
        let cache = StatusCache::<u8>::new();
        cache.state.lock().unwrap().running = true;
        let woke = Arc::new(AtomicUsize::new(0));
        let wake_count = woke.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = ProbeGuard {
                cache: cache.clone(),
                generation: 0,
                wake: Box::new(move || {
                    wake_count.fetch_add(1, Ordering::SeqCst);
                    panic!("injected wake failure");
                }),
            };
            panic!("injected probe failure");
        }));
        assert!(result.is_err());
        assert_eq!(woke.load(Ordering::SeqCst), 1);
        assert!(!cache.state.lock().unwrap().running);
        return;
    }

    let module = module_path!().split_once("::").unwrap().1;
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg(format!(
            "{module}::wake_panic_during_probe_unwind_is_contained"
        ))
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "isolated panic probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}
