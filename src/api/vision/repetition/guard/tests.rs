use super::super::scan_for_onset;
use super::*;

/// A correct reply far longer than the scanned prefix. Every token is a whole
/// word that repeats the previous one's shape, which the guard must keep.
fn long_clean_reply() -> String {
    (0..1_500)
        .map(|index| format!("item{index}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_scan_judges_only_the_bounded_prefix() {
    let reply = long_clean_reply();
    assert!(!scan_for_onset("short reply", 64).saturated);

    let scan = scan_for_onset(&reply, 64);
    assert!(scan.saturated);
    assert!(scan.onset.is_none());

    // Text past the bound cannot change the verdict, so rescanning a longer
    // stream only repeats work.
    let extended = format!("{reply}\nite\nm0 ite\nm1 ite\nm2 ite\nm3");
    assert_eq!(scan_for_onset(&extended, 64).onset, scan.onset);
}

#[test]
fn a_stream_stops_rescanning_once_its_prefix_is_judged() {
    let reply = long_clean_reply();
    let mut guard = RepetitionGuard::default();
    for chunk in reply.split_inclusive(' ') {
        assert!(matches!(guard.observe(chunk), GuardAction::Paint));
    }
    assert!(guard.prefix_judged());
    assert_eq!(guard.finish(reply.clone()), reply);
}
