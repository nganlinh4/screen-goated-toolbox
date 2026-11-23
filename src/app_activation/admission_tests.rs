//! Owner-side admission, the in-memory pending list and the retired TEMP queue.

use super::admission::Admission;
use super::{PENDING_FILES, PendingFiles};
use std::path::PathBuf;
use std::sync::{Mutex, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Tests that use the process-wide admission lock and pending list run alone.
static GLOBAL_STATE: Mutex<()> = Mutex::new(());

fn unique(label: &str) -> String {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("sgt-{label}-{}-{nonce}", std::process::id())
}

#[test]
fn admission_serializes_holders_times_out_and_survives_a_panicking_holder() {
    static LOCAL: Admission = Admission::new();
    let holder = LOCAL.acquire_with_timeout(Duration::from_secs(1)).unwrap();
    let timed_out = std::thread::spawn(|| {
        LOCAL
            .acquire_with_timeout(Duration::from_millis(100))
            .err()
            .map(|error| error.kind())
    });
    assert_eq!(
        timed_out.join().unwrap(),
        Some(std::io::ErrorKind::TimedOut)
    );

    let (sender, receiver) = mpsc::channel();
    let waiter = std::thread::spawn(move || {
        let guard = LOCAL.acquire_with_timeout(Duration::from_secs(10)).unwrap();
        sender.send(()).unwrap();
        drop(guard);
    });
    assert!(
        receiver.recv_timeout(Duration::from_millis(250)).is_err(),
        "admission was granted to two holders"
    );
    drop(holder);
    receiver.recv_timeout(Duration::from_secs(10)).unwrap();
    waiter.join().unwrap();

    let panicked = std::thread::spawn(|| {
        let _guard = LOCAL.acquire_with_timeout(Duration::from_secs(1)).unwrap();
        panic!("holder failed");
    });
    assert!(panicked.join().is_err());
    drop(
        LOCAL
            .acquire_with_timeout(Duration::from_millis(100))
            .unwrap(),
    );
}

#[test]
fn pending_files_keep_order_and_exact_paths_and_drain_once() {
    use std::os::windows::ffi::OsStringExt;
    let pending = PendingFiles::new();
    let first = PathBuf::from(r"C:\  입력-🎵 file.txt ");
    let second = PathBuf::from(std::ffi::OsString::from_wide(&[
        b'C' as u16,
        b':' as u16,
        b'\\' as u16,
        0xd800,
    ]));
    pending.push(first.clone());
    pending.push(second.clone());
    assert!(!pending.is_empty());

    let mut seen = Vec::new();
    assert!(pending.drain(|path| {
        let dispatched = path == first;
        seen.push(path);
        dispatched
    }));
    assert_eq!(seen, vec![first, second]);
    assert!(pending.is_empty());
    assert!(!pending.drain(|_| true));
}

#[test]
fn a_reserved_restart_holds_admission_and_a_queued_file_refuses_restart() {
    let _serial = GLOBAL_STATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let file = std::env::temp_dir().join(unique("activation-file"));
    std::fs::write(&file, b"user file").unwrap();

    let restart = super::reserve_restart_admission().unwrap();
    let (sender, receiver) = mpsc::channel();
    let admitted = file.clone();
    let producer = std::thread::spawn(move || {
        sender
            .send(super::admit_activation(Some(&admitted)))
            .unwrap();
    });
    assert!(
        receiver.recv_timeout(Duration::from_millis(250)).is_err(),
        "a file was admitted while a restart was reserved"
    );
    drop(restart);
    receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    producer.join().unwrap();

    let refused = super::reserve_restart_admission()
        .err()
        .map(|error| error.kind());
    assert_eq!(refused, Some(std::io::ErrorKind::WouldBlock));
    let mut queued = Vec::new();
    {
        let _admission = super::admission::acquire().unwrap();
        PENDING_FILES.drain(|path| {
            queued.push(path);
            true
        });
    }
    assert_eq!(queued, vec![file.clone()]);
    drop(super::reserve_restart_admission().unwrap());
    assert_eq!(std::fs::read(&file).unwrap(), b"user file");
    std::fs::remove_file(file).unwrap();
}

/// A same-user process at lower integrity could write the old TEMP queue. The
/// owner, at any integrity level, must neither dispatch, delete nor be
/// blocked by such an entry.
#[test]
fn the_owner_ignores_requests_planted_in_the_retired_temp_queue() {
    let _serial = GLOBAL_STATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let target = std::env::temp_dir().join(unique("planted-target"));
    std::fs::write(&target, b"target").unwrap();
    let queue_name = format!("sgt-activation-{}", super::current_exe_namespace_suffix());
    let mut planted = Vec::new();
    for root in [std::env::temp_dir(), crate::paths::app_temp_dir()] {
        let queue = root.join(&queue_name);
        let created = !queue.exists();
        std::fs::create_dir_all(&queue).unwrap();
        let entry = queue.join(format!("{}.pending", unique("legacy")));
        std::fs::write(&entry, target.to_str().unwrap().as_bytes()).unwrap();
        planted.push((queue, entry, created));
    }

    let mut dispatched = Vec::new();
    let processed = super::process_pending_files_with(|path| dispatched.push(path));
    assert!(!processed);
    assert!(dispatched.is_empty(), "{dispatched:?}");
    drop(super::reserve_restart_admission().expect("a planted entry must not block restart"));

    for (queue, entry, created) in planted {
        assert_eq!(
            std::fs::read(&entry).unwrap(),
            target.to_str().unwrap().as_bytes()
        );
        std::fs::remove_file(&entry).unwrap();
        if created {
            std::fs::remove_dir(&queue).unwrap();
        }
    }
    std::fs::remove_file(target).unwrap();
}
