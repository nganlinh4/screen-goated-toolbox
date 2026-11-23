//! Fast in-place translation mini app built on the shared screen-selection flow.

mod appearance;
mod backdrop;
mod capture;
pub(crate) mod contract;
mod detector;
mod diagnostics;
mod diagnostics_model_attempts;
#[cfg(debug_assertions)]
mod evidence_capture;
pub(crate) mod fixed_region;
pub(crate) mod geometry;
mod image_payload;
mod incremental;
mod inference;
mod inference_telemetry;
#[cfg(debug_assertions)]
mod lab_output;
#[cfg(debug_assertions)]
mod lab_queue;
mod processing;
mod render;
mod render_mailbox;
mod render_scene;
#[cfg(debug_assertions)]
mod replay;
pub(crate) mod request;
mod runtime;
mod schema;
pub(crate) mod source_bounds;
pub(crate) mod stream_parser;
pub(crate) mod subtitles;
mod text_metrics;
mod translation_validation;
mod units;

static CAPTURE_HANDLER: crate::overlay::image_capture_target::ImageCaptureHandler =
    crate::overlay::image_capture_target::ImageCaptureHandler {
        prepare: prepare_detector,
        process: process_captured_region,
        localized_name: capture_label,
    };

pub(crate) fn capture_target() -> crate::overlay::image_capture_target::ImageCaptureTarget {
    crate::overlay::image_capture_target::ImageCaptureTarget::Handler(&CAPTURE_HANDLER)
}

pub(crate) fn prepare_detector() {
    detector::prepare(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
        false,
    )));
}

pub(crate) fn process_captured_region(
    image: image::RgbaImage,
    rect: windows::Win32::Foundation::RECT,
) {
    capture::process_captured_region(image, rect);
}

fn capture_label(ui_language: &str) -> String {
    crate::gui::locale::LocaleText::get(ui_language)
        .screen_translate
        .screen_translate_title
        .to_string()
}

pub(crate) fn run_ui_test(image_path: Option<std::path::PathBuf>) {
    if let Some(image_path) = image_path {
        capture::start_image(image_path);
        return;
    }
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(1_500));
        capture::start_foreground();
    });
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy)]
struct LabSnapshot {
    subtitle: u64,
    capture: u64,
    jobs: u64,
}

#[cfg(debug_assertions)]
fn lab_snapshot() -> Option<LabSnapshot> {
    let subtitle = subtitles::diagnostic_idle_snapshot()?;
    let capture = crate::overlay::capture_activity();
    let jobs = runtime::lab_activity_snapshot()?;
    if capture == u64::MAX
        || crate::overlay::is_busy()
        || capture != crate::overlay::capture_activity()
    {
        return None;
    }
    Some(LabSnapshot {
        subtitle,
        capture,
        jobs,
    })
}

#[cfg(debug_assertions)]
fn admit_lab_job(
    snapshot: LabSnapshot,
) -> Option<(
    u64,
    std::sync::Arc<std::sync::atomic::AtomicBool>,
    Option<u64>,
)> {
    let ((job, cancel), generation) = subtitles::admit_lab_effect(snapshot.subtitle, || {
        let claim = claim_lab_capture(snapshot)?;
        let job = runtime::begin_lab_job(snapshot.jobs)?;
        claim.transfer();
        Some(job)
    })?;
    Some((job, cancel, generation))
}

#[cfg(debug_assertions)]
fn reserve_lab_request(snapshot: LabSnapshot) -> Option<LabSnapshot> {
    let subtitle = subtitles::reserve_lab_request(snapshot.subtitle, || {
        let _claim = claim_lab_capture(snapshot)?;
        runtime::with_lab_start_guard(snapshot.jobs, || ())
    })?;
    Some(LabSnapshot {
        subtitle,
        ..snapshot
    })
}

#[cfg(debug_assertions)]
struct LabCaptureClaim {
    release: bool,
    _internal: std::sync::MutexGuard<'static, ()>,
}

#[cfg(debug_assertions)]
static LAB_CAPTURE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(debug_assertions)]
impl LabCaptureClaim {
    fn transfer(mut self) {
        self.release = false;
    }
}

#[cfg(debug_assertions)]
impl Drop for LabCaptureClaim {
    fn drop(&mut self) {
        if self.release {
            crate::overlay::set_is_busy(false);
        }
    }
}

#[cfg(debug_assertions)]
fn claim_lab_capture(snapshot: LabSnapshot) -> Option<LabCaptureClaim> {
    let claim = claim_current_capture(snapshot.capture)?;
    if runtime::lab_activity_snapshot() != Some(snapshot.jobs) {
        return None;
    }
    Some(claim)
}

#[cfg(debug_assertions)]
fn claim_current_capture(observed: u64) -> Option<LabCaptureClaim> {
    let internal = LAB_CAPTURE_LOCK.lock().ok()?;
    if !crate::overlay::try_claim_internal_capture() {
        return None;
    }
    let claim = LabCaptureClaim {
        release: true,
        _internal: internal,
    };
    if !claim_matches(observed, crate::overlay::capture_activity()) {
        return None;
    }
    Some(claim)
}

#[cfg(debug_assertions)]
fn claim_matches(observed: u64, claimed: u64) -> bool {
    observed != u64::MAX && observed == claimed
}

#[cfg(debug_assertions)]
fn valid_lab_request(value: &serde_json::Value) -> bool {
    let absolute = |key| {
        value
            .get(key)
            .and_then(|item| item.as_str())
            .is_some_and(|path| std::path::Path::new(path).is_absolute())
    };
    match value.get("action").and_then(|item| item.as_str()) {
        Some("subtitle-toggle" | "subtitle-stop") => true,
        Some("subtitle-region") => subtitles::acceptance::region(value).is_some(),
        Some("subtitle-image") => absolute("image"),
        Some("replay") => absolute("runDirectory") && absolute("output"),
        None => value
            .get("image")
            .and_then(|item| item.as_str())
            .is_some_and(|path| {
                let path = std::path::Path::new(path);
                path.is_absolute() && path.is_file()
            }),
        Some(_) => false,
    }
}

#[cfg(debug_assertions)]
fn parse_lab_request(bytes: &[u8]) -> Option<serde_json::Value> {
    serde_json::from_slice(bytes).ok().filter(valid_lab_request)
}

#[cfg(all(test, debug_assertions))]
fn lab_revisions_match(snapshot: LabSnapshot, claimed: u64, jobs: Option<u64>) -> bool {
    claim_matches(snapshot.capture, claimed) && jobs == Some(snapshot.jobs)
}

#[cfg(all(test, debug_assertions))]
mod lab_admission_tests {
    #[test]
    fn deferred_decode_rejects_a_completed_newer_selection() {
        use std::sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
            mpsc,
        };
        let activity = Arc::new(AtomicU64::new(4));
        let observed = activity.load(Ordering::SeqCst);
        let (resume, wait) = mpsc::channel();
        let later_activity = Arc::clone(&activity);
        let deferred = std::thread::spawn(move || {
            wait.recv().unwrap();
            let claimed = later_activity.load(Ordering::SeqCst);
            super::claim_matches(observed, claimed)
        });
        // A newer selector claims and finishes while the image is decoding.
        activity.fetch_add(1, Ordering::SeqCst);
        resume.send(()).unwrap();
        assert!(!deferred.join().unwrap());
        assert!(super::claim_matches(4, 4));
        assert!(!super::claim_matches(u64::MAX, 0));
    }

    #[test]
    fn diagnostic_ocr_rejects_newer_normal_selection_or_job() {
        let observed = super::LabSnapshot {
            subtitle: 0,
            capture: 4,
            jobs: 7,
        };
        assert!(super::lab_revisions_match(observed, 4, Some(7)));
        assert!(!super::lab_revisions_match(observed, 5, Some(7)));
        assert!(!super::lab_revisions_match(observed, 4, Some(8)));
    }

    #[test]
    fn older_internal_publication_does_not_invalidate_newer_snapshot() {
        let snapshot = crate::overlay::capture_activity();
        let older = super::claim_current_capture(snapshot).unwrap();
        let (started, entered) = std::sync::mpsc::channel();
        let (sent, received) = std::sync::mpsc::channel();
        let newer = std::thread::spawn(move || {
            started.send(()).unwrap();
            let admitted = super::claim_current_capture(snapshot).is_some();
            sent.send(admitted).unwrap();
        });
        entered.recv().unwrap();
        assert!(
            received
                .recv_timeout(std::time::Duration::from_millis(30))
                .is_err()
        );
        drop(older);
        assert!(received.recv().unwrap());
        newer.join().unwrap();
        assert_eq!(crate::overlay::capture_activity(), snapshot);

        assert!(crate::overlay::try_claim_capture());
        crate::overlay::set_is_busy(false);
        let after_selection = crate::overlay::capture_activity();
        assert!(super::claim_current_capture(snapshot).is_none());
        assert_eq!(crate::overlay::capture_activity(), after_selection);
    }

    #[test]
    fn malformed_requests_cannot_replace_valid_submission() {
        let first = super::parse_lab_request(br#"{"action":"subtitle-stop"}"#).unwrap();
        let mut pending = first.clone();
        for malformed in [b"{".as_slice(), br#"{"action":"unknown"}"#] {
            if let Some(value) = super::parse_lab_request(malformed) {
                pending = value;
            }
            assert_eq!(pending, first);
        }
        let newer = super::parse_lab_request(br#"{"action":"subtitle-toggle"}"#).unwrap();
        pending = newer.clone();
        assert_eq!(pending, newer);
        assert_ne!(pending, first);
    }
}

#[cfg(debug_assertions)]
pub(crate) fn run_lab_queue(queue: std::path::PathBuf) {
    std::thread::Builder::new()
        .name("sgt-screen-translate-lab-queue".to_string())
        .spawn(move || {
            let mut pending = lab_queue::PendingRequests::default();
            loop {
                pending.poll(&queue, parse_lab_request);
                if pending.is_ready() {
                    let validation = lab_queue::begin_validation();
                    let admission = lab_snapshot().and_then(reserve_lab_request);
                    drop(validation);
                    if let Some(admission) = admission
                        && let Some(value) = pending.take_ready()
                    {
                        if value.get("action").and_then(|item| item.as_str())
                            == Some("subtitle-toggle")
                        {
                            subtitles::toggle_from_lab(0, admission);
                        } else if value.get("action").and_then(|item| item.as_str())
                            == Some("subtitle-region")
                        {
                            subtitles::acceptance::start(&value, admission);
                        } else if value.get("action").and_then(|item| item.as_str())
                            == Some("subtitle-stop")
                        {
                            subtitles::stop_from_lab(admission);
                        } else if value.get("action").and_then(|item| item.as_str())
                            == Some("replay")
                        {
                            replay::start(value, admission);
                        } else if value.get("action").and_then(|item| item.as_str())
                            == Some("subtitle-image")
                        {
                            if let Some(path) = value.get("image").and_then(|item| item.as_str()) {
                                subtitles::test_image(std::path::PathBuf::from(path), admission);
                            }
                        } else if let Some(image) = value
                            .get("image")
                            .and_then(|item| item.as_str())
                            .map(std::path::PathBuf::from)
                            .filter(|path| path.is_absolute() && path.is_file())
                        {
                            capture::start_lab_image(image, admission);
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        })
        .ok();
}

#[cfg(not(debug_assertions))]
pub(crate) fn run_lab_queue(_queue: std::path::PathBuf) {}

pub(crate) fn stop_detector() {
    subtitles::stop();
    runtime::cancel_active();
    detector::stop();
}
