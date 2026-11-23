//! Capture and visibility never wait for OCR or translation network work.
use super::{Session, capture::Capture, change_detection::TextWatch, observation};
use anyhow::Result;
use std::{
    sync::{Arc, Condvar, Mutex, atomic::Ordering, mpsc},
    time::{Duration, Instant},
};

struct Task {
    epoch: u64,
    capture_id: u64,
    image: Arc<image::RgbaImage>,
    at: Instant,
    origin: (i32, i32),
}
#[derive(Clone)]
struct CapturedFrame {
    id: u64,
    image: Arc<image::RgbaImage>,
}
#[derive(Default)]
struct EmptyConfirmation {
    image: Option<Arc<image::RgbaImage>>,
    capture_id: Option<u64>,
    observations: u8,
    retry_requested: bool,
}

impl EmptyConfirmation {
    fn observe(&mut self, frame: &observation::Frame) {
        if !frame.groups.is_empty() {
            *self = Self::default();
            return;
        }
        if self.capture_id.is_some_and(|old| frame.capture_id <= old) {
            return;
        }
        if self
            .image
            .as_ref()
            .is_some_and(|old| old.as_ref() == frame.image.as_ref())
        {
            self.observations = self.observations.saturating_add(1);
        } else {
            self.observations = 1;
        }
        self.image = Some(Arc::clone(&frame.image));
        self.capture_id = Some(frame.capture_id);
        self.retry_requested = false;
    }

    fn pending(&self) -> bool {
        self.image.is_some() && self.observations < 2 && !self.retry_requested
    }
}
fn is_current_request(requested: Option<&CapturedFrame>, failed_capture_id: u64) -> bool {
    requested.is_some_and(|current| current.id == failed_capture_id)
}
type Queue = Arc<(Mutex<Option<Task>>, Condvar)>;

#[cfg(debug_assertions)]
#[derive(Default)]
pub(super) struct DiagnosticAdmission {
    generation: u64,
    exhausted: bool,
    selector: Option<u64>,
}

#[cfg(debug_assertions)]
impl DiagnosticAdmission {
    fn advance(&mut self) -> Result<u64> {
        if self.exhausted {
            anyhow::bail!("diagnostic request identity exhausted");
        }
        let Some(next) = self.generation.checked_add(1) else {
            self.exhausted = true;
            anyhow::bail!("diagnostic request identity exhausted");
        };
        self.generation = next;
        Ok(self.generation)
    }

    pub(super) fn invalidate(&mut self) {
        let _ = self.advance();
    }

    fn reserve_replay(&mut self) -> Result<u64> {
        anyhow::ensure!(self.selector.is_none(), "subtitle selection is active");
        self.advance()
    }

    pub(super) fn begin_selector(&mut self) -> Result<u64> {
        anyhow::ensure!(
            self.selector.is_none(),
            "subtitle selection is already active"
        );
        let ticket = self.advance()?;
        self.selector = Some(ticket);
        Ok(ticket)
    }

    fn finish_selector(&mut self, ticket: u64) -> bool {
        if self.selector != Some(ticket) {
            return false;
        }
        self.selector = None;
        !self.exhausted && self.generation == ticket
    }

    fn accepts_replay(&self, request: u64) -> bool {
        !self.exhausted && self.selector.is_none() && self.generation == request
    }

    pub(super) fn accepts_snapshot(&self, observed: u64) -> bool {
        self.accepts_replay(observed)
    }
}

#[cfg(debug_assertions)]
pub(super) static DIAGNOSTIC_ADMISSION: Mutex<DiagnosticAdmission> =
    Mutex::new(DiagnosticAdmission {
        generation: 0,
        exhausted: false,
        selector: None,
    });

#[cfg(debug_assertions)]
fn reserve_replay_request(
    request: &Mutex<DiagnosticAdmission>,
    observed: u64,
    busy: impl FnOnce() -> bool,
) -> Result<u64> {
    let mut admission = request
        .lock()
        .map_err(|_| anyhow::anyhow!("diagnostic request lock poisoned"))?;
    anyhow::ensure!(
        !busy() && !admission.exhausted && admission.generation == observed,
        "diagnostic request was superseded"
    );
    admission.reserve_replay()
}

#[cfg(debug_assertions)]
pub(in crate::overlay::screen_translate) fn diagnostic_idle_snapshot() -> Option<u64> {
    let admission = DIAGNOSTIC_ADMISSION.lock().ok()?;
    if crate::overlay::is_busy() || admission.selector.is_some() || admission.exhausted {
        return None;
    }
    Some(admission.generation)
}

#[cfg(debug_assertions)]
pub(in crate::overlay::screen_translate) fn reserve_lab_request(
    observed: u64,
    validate: impl FnOnce() -> Option<()>,
) -> Option<u64> {
    reserve_lab_request_on(&DIAGNOSTIC_ADMISSION, observed, validate)
}

#[cfg(debug_assertions)]
fn reserve_lab_request_on(
    gate: &Mutex<DiagnosticAdmission>,
    observed: u64,
    validate: impl FnOnce() -> Option<()>,
) -> Option<u64> {
    let mut admission = gate.lock().ok()?;
    if !admission.accepts_snapshot(observed) {
        return None;
    }
    validate()?;
    admission.reserve_replay().ok()
}

#[cfg(debug_assertions)]
pub(in crate::overlay::screen_translate) fn admit_lab_effect<T>(
    observed: u64,
    effect: impl FnOnce() -> Option<T>,
) -> Option<(T, Option<u64>)> {
    let _validation = super::super::lab_queue::validation_permit();
    admit_lab_effect_on(&DIAGNOSTIC_ADMISSION, observed, effect)
}

#[cfg(debug_assertions)]
fn admit_lab_effect_on<T>(
    gate: &Mutex<DiagnosticAdmission>,
    observed: u64,
    effect: impl FnOnce() -> Option<T>,
) -> Option<(T, Option<u64>)> {
    let mut admission = gate.lock().ok()?;
    if !admission.accepts_snapshot(observed) {
        return None;
    }
    let result = effect()?;
    admission.invalidate();
    Some((
        result,
        (!admission.exhausted).then_some(admission.generation),
    ))
}

#[cfg(debug_assertions)]
pub(in crate::overlay::screen_translate) fn with_lab_generation<T>(
    observed: u64,
    effect: impl FnOnce() -> T,
) -> Option<T> {
    let _validation = super::super::lab_queue::validation_permit();
    with_lab_generation_on(&DIAGNOSTIC_ADMISSION, observed, effect)
}

#[cfg(debug_assertions)]
fn with_lab_generation_on<T>(
    gate: &Mutex<DiagnosticAdmission>,
    observed: u64,
    effect: impl FnOnce() -> T,
) -> Option<T> {
    let admission = gate.lock().ok()?;
    admission.accepts_snapshot(observed).then(effect)
}

#[cfg(debug_assertions)]
pub(super) fn complete_interactive_selector(ticket: u64, finish: impl FnOnce(bool)) -> Result<()> {
    let mut admission = DIAGNOSTIC_ADMISSION
        .lock()
        .map_err(|_| anyhow::anyhow!("diagnostic request lock poisoned"))?;
    finish(admission.finish_selector(ticket));
    Ok(())
}

mod lifecycle;
pub(super) use lifecycle::start;

#[cfg(debug_assertions)]
fn diagnostic_session(
    slot: &mut Option<Arc<Session>>,
    rect: windows::Win32::Foundation::RECT,
) -> Result<(Arc<Session>, bool)> {
    if let Some(session) = slot {
        anyhow::ensure!(
            session.kind == super::SessionKind::Diagnostic,
            "stop interactive subtitle session before replay"
        );
        anyhow::ensure!(
            !session.stop.load(Ordering::Acquire),
            "diagnostic subtitle session is stopping"
        );
        return Ok((Arc::clone(session), false));
    }
    let session = super::new_session(
        rect,
        rect,
        "en".into(),
        String::new(),
        super::SessionKind::Diagnostic,
    );
    session.started.store(true, Ordering::Release);
    *slot = Some(Arc::clone(&session));
    Ok((session, true))
}

fn observe(session: &Arc<Session>) -> Result<()> {
    let mut capture = Capture::new(session.view.lock().unwrap().rect)?;
    let queue: Queue = Arc::new((Mutex::new(None), Condvar::new()));
    let (sender, receiver) = mpsc::channel();
    let worker = Arc::clone(session);
    let worker_queue = Arc::clone(&queue);
    std::thread::spawn(move || {
        loop {
            let mut pending = worker_queue.0.lock().unwrap();
            while pending.is_none() && !worker.stop.load(Ordering::Acquire) {
                pending = worker_queue
                    .1
                    .wait_timeout(pending, Duration::from_millis(100))
                    .unwrap()
                    .0;
            }
            if worker.stop.load(Ordering::Acquire) {
                return;
            }
            let task = pending.take().unwrap();
            drop(pending);
            let result = observation::read(&task.image, &worker.stop).and_then(|o| {
                let watch = TextWatch::new(&task.image, &o.events, task.at);
                observation::prepare(task.image, task.capture_id, &o, task.at, task.origin)
                    .map(|frame| (frame, watch))
            });
            if sender.send((task.epoch, task.capture_id, result)).is_err() {
                return;
            }
        }
    });
    let origin = Instant::now();
    let mut epoch = 0;
    let mut latest: Option<CapturedFrame> = None;
    let mut requested: Option<CapturedFrame> = None;
    let mut capture_id = 0u64;
    let mut frame: Option<Arc<observation::Frame>> = None;
    let mut watch: Option<TextWatch> = None;
    let mut empty_confirmation = EmptyConfirmation::default();
    let mut retry_after_failure = false;
    let mut last_request = Instant::now() - Duration::from_secs(1);
    let mut exclusion_wait = Instant::now();
    let mut settings_scope = None;
    while !session.stop.load(Ordering::Acquire) {
        if !crate::overlay::result::scene_compositor::capture_is_excluded() {
            anyhow::ensure!(
                exclusion_wait.elapsed() < Duration::from_secs(15),
                "result capture exclusion did not become ready"
            );
            std::thread::sleep(Duration::from_millis(30));
            continue;
        }
        exclusion_wait = Instant::now();
        let settings = crate::APP
            .lock()
            .map_err(|_| anyhow::anyhow!("settings unavailable"))?
            .config
            .screen_translate
            .clone();
        let scope = (
            settings.target_language,
            settings.translation_model,
            settings.translation_prompt,
        );
        let view = {
            let mut view = session.view.lock().unwrap();
            if settings_scope.as_ref().is_some_and(|old| old != &scope) {
                view.epoch += 1;
            }
            settings_scope = Some(scope);
            view.clone()
        };
        if view.epoch != epoch {
            epoch = view.epoch;
            session.scene.lock().unwrap().clear(epoch);
            latest = None;
            requested = None;
            frame = None;
            watch = None;
            empty_confirmation = EmptyConfirmation::default();
            retry_after_failure = false;
            queue.0.lock().unwrap().take();
        }
        if let Some(image) = capture.frame(view.rect)? {
            capture_id = capture_id
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("subtitle capture identity exhausted"))?;
            latest = Some(CapturedFrame {
                id: capture_id,
                image: Arc::new(image),
            });
        }
        while let Ok((result_epoch, result_capture_id, result)) = receiver.try_recv() {
            if result_epoch != epoch {
                continue;
            }
            match result {
                Ok((ready, ready_watch)) => {
                    empty_confirmation.observe(&ready);
                    frame = Some(ready);
                    watch = Some(ready_watch);
                }
                Err(error) => {
                    if is_current_request(requested.as_ref(), result_capture_id) {
                        retry_after_failure = true;
                    }
                    crate::log_info!("[Subtitles] OCR uncertain: {error:#}");
                }
            }
        }
        if let Some(image) = &latest {
            let scene = session.scene.lock().unwrap();
            let empty_retry = scene.tracks.is_empty() && empty_confirmation.pending();
            let confirmation = scene.needs_confirmation() || empty_retry || retry_after_failure;
            drop(scene);
            if should_request_ocr(
                requested.as_ref(),
                image,
                watch.as_ref(),
                last_request.elapsed(),
                Instant::now(),
                confirmation,
            ) {
                *queue.0.lock().unwrap() = Some(Task {
                    epoch,
                    capture_id: image.id,
                    image: Arc::clone(&image.image),
                    at: Instant::now(),
                    origin: (view.rect.left, view.rect.top),
                });
                queue.1.notify_one();
                requested = Some(image.clone());
                retry_after_failure = false;
                if empty_retry {
                    empty_confirmation.retry_requested = true;
                }
                last_request = Instant::now();
            }
            if let Some(frame) = &frame {
                apply(
                    session,
                    epoch,
                    Arc::clone(frame),
                    origin.elapsed().as_millis() as u64,
                    &image.image,
                );
            }
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    Ok(())
}

fn should_request_ocr(
    requested: Option<&CapturedFrame>,
    image: &CapturedFrame,
    watch: Option<&TextWatch>,
    elapsed: Duration,
    now: Instant,
    confirmation: bool,
) -> bool {
    if elapsed < Duration::from_millis(180) {
        return false;
    }
    if confirmation {
        // An unchanged screenshot is still a new observation only when capture
        // produced a new frame. Never enqueue the same retained frame twice.
        return requested.is_none_or(|old| image.id > old.id);
    }
    requested.is_none_or(|old| image.id > old.id && old.image.as_ref() != image.image.as_ref())
        && watch.is_none_or(|w| w.needs_ocr(&image.image, now))
}

pub(super) fn apply(
    session: &Session,
    epoch: u64,
    frame: Arc<observation::Frame>,
    now: u64,
    image: &image::RgbaImage,
) {
    let view = session.view.lock().unwrap();
    if view.epoch != epoch || session.stop.load(Ordering::Acquire) {
        return;
    }
    let mut scene = session.scene.lock().unwrap();
    if frame.observed_at.elapsed() > Duration::from_secs(1)
        && !frame.groups.is_empty()
        && frame.groups.iter().all(|g| {
            !super::change_detection::same_edges(&frame.image, image, g.candidate.bounds)
                && !super::change_detection::cleared(&frame.image, image, g.candidate.bounds)
        })
    {
        scene.uncertain(now);
    } else {
        scene.observe(epoch, frame, now, image);
    }
    session.wake.notify_one();
}

// This diagnostic explicitly simulates two ordered captures of the supplied
// image. Production observations still require distinct captured frames.
#[cfg(debug_assertions)]
pub(super) fn replay_test_frame(
    session: &Session,
    epoch: u64,
    template: &observation::Frame,
    image: &image::RgbaImage,
) -> Result<()> {
    anyhow::ensure!(
        session.kind == super::SessionKind::Diagnostic && !session.stop.load(Ordering::Acquire),
        "diagnostic subtitle session is unavailable"
    );
    static CLOCK: Mutex<(u64, u64)> = Mutex::new((0, 1));
    let mut clock = CLOCK.lock().unwrap();
    let now = clock.0;
    let first_id = clock.1;
    let second_id = first_id
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("diagnostic capture identity exhausted"))?;
    let next_id = second_id
        .checked_add(1)
        .ok_or_else(|| anyhow::anyhow!("diagnostic capture identity exhausted"))?;
    let next_now = now
        .checked_add(1000)
        .ok_or_else(|| anyhow::anyhow!("diagnostic clock exhausted"))?;
    for (id, offset) in [(first_id, 0), (second_id, 400)] {
        let mut frame = template.clone();
        frame.capture_id = id;
        apply(session, epoch, Arc::new(frame), now + offset, image);
    }
    *clock = (next_now, next_id);
    Ok(())
}

#[cfg(debug_assertions)]
pub(in crate::overlay::screen_translate) fn test_image(
    path: std::path::PathBuf,
    observed: super::super::LabSnapshot,
) {
    let request = match reserve_replay_request(
        &DIAGNOSTIC_ADMISSION,
        observed.subtitle,
        crate::overlay::is_busy,
    ) {
        Ok(request) => request,
        Err(error) => {
            crate::log_info!("[Subtitles] tracked-image request failed: {error:#}");
            return;
        }
    };
    std::thread::spawn(move || {
        let result = (|| -> Result<()> {
            anyhow::ensure!(path.is_absolute(), "absolute image path required");
            let image = Arc::new(image::open(path)?.to_rgba8());
            let observation =
                observation::read(&image, &Arc::new(std::sync::atomic::AtomicBool::new(false)))?;
            let origin = super::super::capture::image_origin(image.width(), image.height());
            let frame =
                observation::prepare(Arc::clone(&image), 1, &observation, Instant::now(), origin)?;
            // A later request may have completed OCR first. Serialize the
            // admission and session mutation with new request publication.
            let _validation = super::super::lab_queue::validation_permit();
            let current = DIAGNOSTIC_ADMISSION
                .lock()
                .map_err(|_| anyhow::anyhow!("diagnostic request lock poisoned"))?;
            anyhow::ensure!(
                current.accepts_replay(request),
                "diagnostic request was superseded"
            );
            let _capture_claim = super::super::claim_lab_capture(observed)
                .ok_or_else(|| anyhow::anyhow!("diagnostic request was superseded"))?;
            let rect = windows::Win32::Foundation::RECT {
                left: origin.0,
                top: origin.1,
                right: origin.0 + image.width() as i32,
                bottom: origin.1 + image.height() as i32,
            };
            let session =
                super::super::runtime::with_lab_start_guard(observed.jobs, || -> Result<_> {
                    let mut slot = super::SESSION.lock().unwrap();
                    let (session, created) = diagnostic_session(&mut slot, rect)?;
                    if created {
                        let translator = Arc::clone(&session);
                        std::thread::spawn(move || super::pipeline::run(translator));
                    }
                    let epoch = {
                        let mut view = session.view.lock().unwrap();
                        if view.rect != rect {
                            view.rect = rect;
                            view.epoch += 1;
                        }
                        view.epoch
                    };
                    replay_test_frame(&session, epoch, &frame, &image)?;
                    Ok(session)
                })
                .ok_or_else(|| anyhow::anyhow!("diagnostic request was superseded"))??;
            drop(current);
            let scene = session.scene.lock().unwrap();
            crate::log_info!(
                "[Subtitles] replay groups={} tracks={} live={} owned_results={}",
                frame.groups.len(),
                scene.tracks.len(),
                scene
                    .tracks
                    .iter()
                    .filter(|t| scene.live(t.ticket()))
                    .count(),
                scene
                    .tracks
                    .iter()
                    .filter(|t| t.job.is_some_and(super::super::runtime::is_current))
                    .count()
            );
            Ok(())
        })();
        crate::log_info!("[Subtitles] tracked-image pipeline result={result:?}");
    });
}
#[cfg(test)]
#[path = "engine_tests.rs"]
mod scheduling_tests;
