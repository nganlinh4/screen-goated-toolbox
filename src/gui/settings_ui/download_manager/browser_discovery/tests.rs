use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn no_scan() -> Vec<CookieBrowser> {
    panic!("discovery unexpectedly restarted")
}

fn no_wake() {
    panic!("discovery unexpectedly restarted")
}

#[test]
fn discovery_is_deferred_nonblocking_and_cached_after_completion() -> Result<()> {
    let mut discovery = BrowserDiscovery::new();
    assert!(matches!(discovery.state, State::NotStarted));
    assert_eq!(discovery.available(), &[CookieBrowser::None]);

    let calls = Arc::new(AtomicUsize::new(0));
    let worker_calls = calls.clone();
    let (entered, entry) = mpsc::sync_channel(1);
    let (release, blocked) = mpsc::sync_channel(1);
    let (woke, wake) = mpsc::sync_channel(1);
    discovery.update_with(
        move || {
            worker_calls.fetch_add(1, Ordering::SeqCst);
            entered.send(()).unwrap();
            blocked.recv_timeout(Duration::from_secs(5)).unwrap();
            vec![CookieBrowser::None, CookieBrowser::Firefox]
        },
        move || woke.send(()).unwrap(),
    )?;
    entry.recv_timeout(Duration::from_secs(5))?;
    for _ in 0..100 {
        discovery.update_with(no_scan, no_wake)?;
        assert_eq!(discovery.available(), &[CookieBrowser::None]);
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    release.send(())?;
    wake.recv_timeout(Duration::from_secs(5))?;
    discovery.update_with(no_scan, no_wake)?;
    assert_eq!(
        discovery.available(),
        &[CookieBrowser::None, CookieBrowser::Firefox]
    );
    assert!(matches!(discovery.state, State::Complete));
    for _ in 0..100 {
        discovery.update_with(no_scan, no_wake)?;
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

#[test]
fn hiding_and_reopening_observes_the_same_completed_scan() -> Result<()> {
    let mut discovery = BrowserDiscovery::new();
    let (woke, wake) = mpsc::sync_channel(1);
    discovery.update_with(
        || vec![CookieBrowser::None, CookieBrowser::Chrome],
        move || woke.send(()).unwrap(),
    )?;
    // No polling while the feature is hidden. Completion is retained for reopening.
    wake.recv_timeout(Duration::from_secs(5))?;
    discovery.update_with(no_scan, no_wake)?;
    assert_eq!(
        discovery.available(),
        &[CookieBrowser::None, CookieBrowser::Chrome]
    );
    discovery.update_with(no_scan, no_wake)?;
    Ok(())
}

#[test]
fn worker_panic_wakes_once_and_does_not_cause_a_retry_loop() -> Result<()> {
    let mut discovery = BrowserDiscovery::new();
    let (woke, wake) = mpsc::sync_channel(1);
    discovery.update_with(
        || panic!("detector fixture failure"),
        move || woke.send(()).unwrap(),
    )?;
    wake.recv_timeout(Duration::from_secs(5))?;
    assert!(discovery.update_with(no_scan, no_wake).is_err());
    assert!(matches!(discovery.state, State::Complete));
    assert_eq!(discovery.available(), &[CookieBrowser::None]);
    discovery.update_with(no_scan, no_wake)?;
    Ok(())
}

#[test]
fn dropping_the_owner_neither_blocks_nor_retargets_a_late_completion() -> Result<()> {
    let mut discovery = BrowserDiscovery::new();
    let (entered, entry) = mpsc::sync_channel(1);
    let (release, blocked) = mpsc::sync_channel(1);
    let (woke, wake) = mpsc::sync_channel(1);
    discovery.update_with(
        move || {
            entered.send(()).unwrap();
            blocked.recv_timeout(Duration::from_secs(5)).unwrap();
            vec![CookieBrowser::None, CookieBrowser::Edge]
        },
        move || woke.send(()).unwrap(),
    )?;
    entry.recv_timeout(Duration::from_secs(5))?;
    drop(discovery);
    let replacement = BrowserDiscovery::new();
    release.send(())?;
    wake.recv_timeout(Duration::from_secs(5))?;
    assert!(matches!(replacement.state, State::NotStarted));
    assert_eq!(replacement.available(), &[CookieBrowser::None]);
    Ok(())
}
