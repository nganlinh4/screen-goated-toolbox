use super::*;
use std::sync::mpsc;
use std::time::Duration;

#[test]
fn nonpersistent_input_disables_all_sink_effects_for_its_session() {
    let owner = SessionRetention::default();
    let _scope = owner.enter();
    assert_eq!(persist(|| "retained"), Some("retained"));
    owner.forbid();
    let mut writes = Vec::new();
    assert!(persist(|| writes.push("private transcript")).is_none());
    assert!(persist(|| writes.push("private screenshot")).is_none());
    assert!(writes.is_empty());
    assert!(!current().allows());
}

#[test]
fn late_worker_keeps_its_original_policy_after_a_fresh_session_starts() {
    let owner = SessionRetention::default();
    let outer = owner.enter();
    let (ready, proceed) = mpsc::channel();
    let work = inherit(move || {
        proceed.recv().unwrap();
        persist(|| "late private result")
    });
    let worker = std::thread::spawn(work);
    owner.forbid();
    drop(outer);
    let _new = start_session();
    assert!(current().allows());
    ready.send(()).unwrap();
    assert!(worker.join().unwrap().is_none());
    assert!(current().allows());
}

#[test]
fn retired_session_cannot_repopulate_its_ephemeral_artifact_store() {
    let session = start_session();
    let owner = current();
    owner.forbid();
    let work = inherit(|| {
        assert!(
            super::super::artifacts::create_text("test", None, None, "late private output")
                .is_err()
        );
        assert!(persist(|| "late trace").is_none());
    });
    drop(session);
    let _fresh = start_session();
    work();
    assert!(current().allows());
}

#[test]
fn disabling_retention_waits_for_an_admitted_write_then_rejects_later_writes() {
    let owner = SessionRetention::default();
    let _scope = owner.enter();
    let (writing, started) = mpsc::channel();
    let (release, finish_write) = mpsc::channel();
    let writer = std::thread::spawn(inherit(move || {
        persist(|| {
            writing.send(()).unwrap();
            finish_write.recv().unwrap();
        })
    }));
    started.recv().unwrap();
    let stopper = owner.clone();
    let (stopped, complete) = mpsc::channel();
    let stopping = std::thread::spawn(move || {
        stopper.forbid();
        stopped.send(()).unwrap();
    });
    assert!(complete.recv_timeout(Duration::from_millis(30)).is_err());
    release.send(()).unwrap();
    assert!(writer.join().unwrap().is_some());
    complete.recv_timeout(Duration::from_secs(1)).unwrap();
    stopping.join().unwrap();
    assert!(persist(|| panic!("write after nonpersistent input")).is_none());
}
