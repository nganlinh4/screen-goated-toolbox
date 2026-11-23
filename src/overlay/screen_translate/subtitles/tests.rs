use super::*;
use anyhow::Result;
use std::time::{Duration, Instant};

pub(super) fn test_session() -> Arc<Session> {
    Arc::new(Session {
        kind: SessionKind::Interactive,
        native_window: AtomicIsize::new(0),
        stop: Arc::new(AtomicBool::new(false)),
        started: AtomicBool::new(true),
        view: Mutex::new(View {
            rect: RECT {
                left: 0,
                top: 0,
                right: 1280,
                bottom: 180,
            },
            bounds: RECT {
                left: 0,
                top: 0,
                right: 1280,
                bottom: 720,
            },
            editing: false,
            error: String::new(),
            epoch: 1,
            language: String::new(),
            hotkey: "F10".into(),
        }),
        scene: Mutex::new(tracks::Scene::default()),
        wake: Condvar::new(),
        renderer_failure_reported: Arc::new(AtomicBool::new(false)),
        capture_lifetime: Default::default(),
    })
}

fn session() -> Arc<Session> {
    test_session()
}

#[test]
fn subtitle_shutdown_waits_for_capture_without_holding_the_view_lock() {
    let session = session();
    let completion = session.capture_lifetime.begin();
    let capture = Arc::clone(&session);
    let (closed, received) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        while !capture.stop.load(Ordering::Acquire) {
            std::thread::yield_now();
        }
        let _view = capture.view.lock().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        closed.send(()).unwrap();
        drop(completion);
    });
    session.shutdown();
    assert!(
        received.try_recv().is_ok(),
        "shutdown returned before native capture closed"
    );
    worker.join().unwrap();
}

pub(super) fn frame(lines: &[(&str, u16)]) -> Arc<observation::Frame> {
    use super::super::{contract::DetectedTextRegion, units::Unit};
    static NEXT_CAPTURE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let image = Arc::new(image::RgbaImage::new(1000, 200));
    let groups = lines
        .iter()
        .enumerate()
        .map(|(index, (text, left))| {
            let candidate = DetectedTextRegion {
                id: index as u16,
                bounds:
                    crate::overlay::screen_translate::source_bounds::SourceBounds::from_normalized(
                        [100, *left, 300, *left + 150].into(),
                        1000,
                        200,
                    )
                    .unwrap(),
                source_text: (*text).into(),
                source_alternatives: vec![(*text).into()],
                recognition: Default::default(),
                appearance: None,
            };
            observation::Group {
                candidate: candidate.clone(),
                sources: Arc::from(vec![candidate]),
                unit: Unit {
                    id: index as u16,
                    members: vec![index as u16],
                },
            }
        })
        .collect();
    Arc::new(observation::Frame {
        capture_id: NEXT_CAPTURE.fetch_add(1, Ordering::Relaxed),
        origin: (0, 0),
        image,
        groups,
        uncertain: vec![],
        observed_at: Instant::now(),
    })
}

#[test]
fn quit_discards_pending_groups_and_rejects_late_ocr() {
    let session = session();
    for time in [0, 120] {
        let frame = frame(&[("visible text", 100)]);
        engine::apply(&session, 1, Arc::clone(&frame), time, &frame.image);
    }
    assert!(session.scene.lock().unwrap().tracks[0].queued);
    session.shutdown();
    let frame = frame(&[("visible text", 100)]);
    for time in [200, 400] {
        engine::apply(&session, 1, Arc::clone(&frame), time, &frame.image);
    }
    assert!(session.scene.lock().unwrap().tracks.is_empty());
}

#[test]
fn moving_region_rejects_old_pixels() {
    let session = session();
    session.move_region(RECT {
        left: 20,
        top: 0,
        right: 1300,
        bottom: 180,
    });
    let frame = frame(&[("old text", 100)]);
    for time in [0, 120] {
        engine::apply(&session, 1, Arc::clone(&frame), time, &frame.image);
    }
    assert!(session.scene.lock().unwrap().tracks.is_empty());
}

#[test]
fn hiding_editor_does_not_stop_session() {
    let session = session();
    let slot = Some(Arc::clone(&session));
    assert!(toggle_existing(&slot, "F11"));
    assert!(session.view.lock().unwrap().editing);
    assert!(toggle_existing(&slot, "F12"));
    assert!(!session.view.lock().unwrap().editing);
    assert_eq!(session.view.lock().unwrap().hotkey, "F12");
    for time in [0, 120] {
        let frame = frame(&[("hidden editor dialogue", 100)]);
        engine::apply(&session, 1, Arc::clone(&frame), time, &frame.image);
    }
    let scene = session.scene.lock().unwrap();
    assert_eq!(scene.tracks.len(), 1);
    assert!(scene.tracks[0].queued);
    assert_eq!(scene.tracks[0].source, "hidden editor dialogue");
    drop(scene);
    assert!(!session.stop.load(Ordering::Acquire));
    session.shutdown();
    assert!(session.stop.load(Ordering::Acquire));
}

#[cfg(debug_assertions)]
#[test]
fn diagnostic_replay_uses_fresh_ordered_observations_across_invocations() {
    let mut session = session();
    Arc::get_mut(&mut session).unwrap().kind = SessionKind::Diagnostic;
    let first = frame(&[("a", 100)]);
    engine::replay_test_frame(&session, 1, &first, &first.image).unwrap();
    assert_eq!(session.scene.lock().unwrap().tracks[0].source, "a");

    let second = frame(&[("b", 100)]);
    engine::replay_test_frame(&session, 1, &second, &second.image).unwrap();
    let scene = session.scene.lock().unwrap();
    assert_eq!(scene.tracks.len(), 1);
    assert_eq!(scene.tracks[0].source, "b");
    assert!(scene.tracks[0].dispatchable());
}

#[cfg(debug_assertions)]
#[test]
fn diagnostic_session_does_not_block_interactive_selection_and_is_retired_on_install() {
    let mut diagnostic = session();
    Arc::get_mut(&mut diagnostic).unwrap().kind = SessionKind::Diagnostic;
    let input = frame(&[("a", 100)]);
    engine::replay_test_frame(&diagnostic, 1, &input, &input.image).unwrap();
    let mut slot = Some(Arc::clone(&diagnostic));
    assert!(!toggle_existing(&slot, "F11"));
    assert!(!diagnostic.stop.load(Ordering::Acquire));

    let interactive = session();
    assert!(install_interactive(&mut slot, Arc::clone(&interactive)));
    assert!(diagnostic.stop.load(Ordering::Acquire));
    assert!(diagnostic.scene.lock().unwrap().tracks.is_empty());
    assert!(Arc::ptr_eq(slot.as_ref().unwrap(), &interactive));
    assert!(toggle_existing(&slot, "F11"));
    assert_eq!(interactive.view.lock().unwrap().hotkey, "F11");
    assert!(!install_interactive(&mut slot, session()));
    assert!(Arc::ptr_eq(slot.as_ref().unwrap(), &interactive));
}
#[test]
#[ignore = "requires an unlocked Windows desktop and hardware capture"]
fn native_capture_returns_region_pixels() -> Result<()> {
    let (rect, _) = monitor_region(RECT {
        left: 100,
        top: 100,
        right: 500,
        bottom: 250,
    })?;
    let mut capture = capture::Capture::new(rect)?;
    let began = Instant::now();
    while began.elapsed() < Duration::from_secs(3) {
        if let Some(frame) = capture.frame(rect)? {
            assert_eq!(
                frame.dimensions(),
                (
                    (rect.right - rect.left) as u32,
                    (rect.bottom - rect.top) as u32
                )
            );
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    anyhow::bail!("no native frame within the capture deadline")
}
