use super::*;
use std::sync::Barrier;

#[test]
fn owners_coexist_and_retired_updates_cannot_change_replacements() {
    let registry = Arc::new(Registry::default());
    let first = registry.start("test".into(), false).unwrap();
    assert_eq!(registry.snapshot("test"), Some(-1.0));
    first.report(22.0);
    let stale = first.generation;
    let second = registry.start("test".into(), false).unwrap();
    assert_eq!(registry.snapshot("test"), Some(22.0));
    second.report(75.0);
    drop(first);
    registry.report("test", stale, 99.0);
    assert_eq!(registry.snapshot("test"), Some(75.0));
    drop(second);
    assert_eq!(registry.snapshot("test"), None);
    let replacement = registry.start("test".into(), false).unwrap();
    registry.report("test", stale, 10.0);
    assert_eq!(registry.snapshot("test"), Some(-1.0));
    drop(replacement);
}

#[test]
fn newest_measured_owner_wins_and_remaining_owner_is_restored() {
    let registry = Arc::new(Registry::default());
    let first = registry.start("test".into(), false).unwrap();
    let second = registry.start("test".into(), false).unwrap();
    first.report(20.0);
    second.report(80.0);
    first.report(30.0);
    assert_eq!(registry.snapshot("test"), Some(30.0));
    drop(first);
    assert_eq!(registry.snapshot("test"), Some(80.0));
    second.report(f32::NAN);
    assert_eq!(registry.snapshot("test"), Some(80.0));
    second.report(140.0);
    assert_eq!(registry.snapshot("test"), Some(100.0));
}

#[test]
fn simultaneous_exclusive_starts_have_one_owner() {
    let registry = Arc::new(Registry::default());
    let start = Arc::new(Barrier::new(3));
    let finish = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let (registry, start, finish) = (registry.clone(), start.clone(), finish.clone());
        workers.push(std::thread::spawn(move || {
            start.wait();
            let owner = registry.start("test".into(), true);
            finish.wait();
            owner.is_some()
        }));
    }
    start.wait();
    finish.wait();
    assert_eq!(
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .filter(|won| *won)
            .count(),
        1
    );
}
