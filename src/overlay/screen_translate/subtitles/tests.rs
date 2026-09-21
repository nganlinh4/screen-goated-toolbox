use super::*;
use anyhow::Result;
use std::time::{Duration, Instant};

fn session() -> Arc<Session> {
    Arc::new(Session {
        native_window: AtomicIsize::new(0),
        stop: AtomicBool::new(false),
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
    })
}

pub(super) fn frame(lines: &[(&str, u16)]) -> Arc<observation::Frame> {
    use super::super::{contract::DetectedTextRegion, units::Unit};
    let image = Arc::new(image::RgbaImage::new(1000, 200));
    let groups = lines
        .iter()
        .enumerate()
        .map(|(index, (text, left))| {
            let candidate = DetectedTextRegion {
                id: index as u16,
                bounds: [100, *left, 300, *left + 150].into(),
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
    let frame = frame(&[("visible text", 100)]);
    for time in [0, 120] {
        engine::apply(&session, 1, Arc::clone(&frame), time, &frame.image);
    }
    assert!(session.scene.lock().unwrap().tracks[0].queued);
    session.shutdown();
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
    session.view.lock().unwrap().editing = false;
    assert!(!session.stop.load(Ordering::Acquire));
    session.shutdown();
    assert!(session.stop.load(Ordering::Acquire));
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
