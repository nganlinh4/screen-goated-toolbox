use super::{PreparedAutoCopy, auto_paste_options, with_auto_copy_lock};
use crate::overlay::result::ChainCancelToken;
use std::sync::{Arc, Mutex, mpsc};

#[test]
fn copy_only_newline_is_prepared_without_authorizing_paste() {
    let prepared = PreparedAutoCopy::new("content".into(), Some((true, false)));
    assert_eq!(prepared.text, "content\n");
    assert!(!prepared.allows_paste(Some((true, false))));
    assert!(!prepared.allows_paste(Some((true, true))));
}

#[test]
fn prepared_text_preserves_bytes_and_adds_one_requested_newline() {
    let source = "Unicode \u{1f9ea}\r\n";
    let unchanged = PreparedAutoCopy::new(source.into(), Some((false, true)));
    assert_eq!(unchanged.text.as_bytes(), source.as_bytes());
    let appended = PreparedAutoCopy::new(source.into(), Some((true, true)));
    assert_eq!(appended.text.as_bytes(), format!("{source}\n").as_bytes());
    assert!(appended.allows_paste(Some((true, true))));
}

#[test]
fn prepared_copy_cannot_admit_paste_after_policy_or_identity_changes() {
    let prepared = PreparedAutoCopy::new("content".into(), Some((true, true)));
    assert!(!prepared.allows_paste(None));
    assert!(!prepared.allows_paste(Some((false, true))));
    assert!(!prepared.allows_paste(Some((true, false))));
    let missing = PreparedAutoCopy::new("content".into(), None);
    assert_eq!(missing.text, "content");
    assert!(!missing.allows_paste(Some((false, true))));
}

#[test]
fn paste_policy_uses_only_the_originating_preset_and_current_opt_in() {
    let original = crate::config::Preset {
        id: "original".into(),
        auto_paste: false,
        ..Default::default()
    };
    let replacement = crate::config::Preset {
        id: "replacement".into(),
        auto_paste: true,
        auto_paste_newline: true,
        ..Default::default()
    };
    let mut config = crate::config::Config {
        presets: vec![original, replacement],
        active_preset_idx: 1,
        ..Default::default()
    };
    assert_eq!(
        auto_paste_options(&config, "original"),
        Some((false, false))
    );
    config.presets[0].auto_paste = true;
    assert_eq!(auto_paste_options(&config, "original"), Some((false, true)));
    config.presets.remove(0);
    config.active_preset_idx = 0;
    assert_eq!(auto_paste_options(&config, "original"), None);
    assert_eq!(
        auto_paste_options(&config, "replacement"),
        Some((true, true))
    );
    config.presets.push(config.presets[0].clone());
    assert_eq!(auto_paste_options(&config, "replacement"), None);
}

#[test]
fn cancelled_queued_copy_does_not_reach_clipboard_admission() {
    let token = ChainCancelToken::new();
    let worker_token = token.clone();
    let effects = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let worker_effects = effects.clone();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let lock_holder = std::thread::spawn(move || {
        with_auto_copy_lock(|| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        })
    });
    started_rx.recv().unwrap();
    let worker = std::thread::spawn(move || {
        with_auto_copy_lock(|| {
            worker_token
                .run_if_active(|| worker_effects.fetch_add(1, std::sync::atomic::Ordering::SeqCst))
        })
    });
    token.cancel();
    release_tx.send(()).unwrap();
    lock_holder.join().unwrap();
    assert_eq!(worker.join().unwrap(), None);
    assert_eq!(effects.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn concurrent_copy_cannot_replace_content_before_first_paste() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let (first_started_tx, first_started_rx) = mpsc::channel();
    let (release_first_tx, release_first_rx) = mpsc::channel();
    let (second_attempted_tx, second_attempted_rx) = mpsc::channel();
    let first_events = events.clone();
    let first = std::thread::spawn(move || {
        with_auto_copy_lock(|| {
            first_events.lock().unwrap().push("first copy");
            first_started_tx.send(()).unwrap();
            release_first_rx.recv().unwrap();
            first_events.lock().unwrap().push("first paste");
        });
    });
    first_started_rx.recv().unwrap();
    let second_events = events.clone();
    let second = std::thread::spawn(move || {
        second_attempted_tx.send(()).unwrap();
        with_auto_copy_lock(|| second_events.lock().unwrap().push("second copy"));
    });
    second_attempted_rx.recv().unwrap();
    release_first_tx.send(()).unwrap();
    first.join().unwrap();
    second.join().unwrap();
    assert_eq!(
        *events.lock().unwrap(),
        ["first copy", "first paste", "second copy"]
    );
}
