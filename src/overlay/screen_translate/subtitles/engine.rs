//! Capture and visibility never wait for OCR or translation network work.
use super::{Session, capture::Capture, change_detection::TextWatch, observation};
use anyhow::Result;
use std::{
    sync::{Arc, Condvar, Mutex, atomic::Ordering, mpsc},
    time::{Duration, Instant},
};

struct Task {
    epoch: u64,
    image: Arc<image::RgbaImage>,
    at: Instant,
    origin: (i32, i32),
}
type Queue = Arc<(Mutex<Option<Task>>, Condvar)>;

pub(super) fn start(session: Arc<Session>) {
    let view = session.view.lock().unwrap();
    if session.stop.load(Ordering::Acquire) || session.started.swap(true, Ordering::AcqRel) {
        return;
    }
    crate::overlay::result::scene_compositor::exclude_from_capture(true);
    drop(view);
    let translator = Arc::clone(&session);
    std::thread::spawn(move || super::pipeline::run(translator));
    std::thread::spawn(move || {
        if let Err(error) = observe(&session)
            && !session.stop.load(Ordering::Acquire)
        {
            super::stop_expected(Some(&session));
            super::super::capture::notify_error(&error.to_string());
        }
    });
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
                observation::prepare(task.image, &o, task.at, task.origin)
                    .map(|frame| (frame, watch))
            });
            if sender.send((task.epoch, result)).is_err() {
                return;
            }
        }
    });
    let origin = Instant::now();
    let mut epoch = 0;
    let mut latest: Option<Arc<image::RgbaImage>> = None;
    let mut requested: Option<Arc<image::RgbaImage>> = None;
    let mut frame: Option<Arc<observation::Frame>> = None;
    let mut watch: Option<TextWatch> = None;
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
            queue.0.lock().unwrap().take();
        }
        if let Some(image) = capture.frame(view.rect)? {
            latest = Some(Arc::new(image));
        }
        while let Ok((result_epoch, result)) = receiver.try_recv() {
            if result_epoch != epoch {
                continue;
            }
            match result {
                Ok((ready, ready_watch)) => {
                    frame = Some(ready);
                    watch = Some(ready_watch);
                }
                Err(error) => {
                    requested = None;
                    crate::log_info!("[Subtitles] OCR uncertain: {error:#}");
                }
            }
        }
        if let Some(image) = &latest {
            if last_request.elapsed() >= Duration::from_millis(180)
                && requested
                    .as_ref()
                    .is_none_or(|old| old.as_ref() != image.as_ref())
                && watch
                    .as_ref()
                    .is_none_or(|w| w.needs_ocr(image, Instant::now()))
            {
                *queue.0.lock().unwrap() = Some(Task {
                    epoch,
                    image: Arc::clone(image),
                    at: Instant::now(),
                    origin: (view.rect.left, view.rect.top),
                });
                queue.1.notify_one();
                requested = Some(Arc::clone(image));
                last_request = Instant::now();
            }
            if let Some(frame) = &frame {
                apply(
                    session,
                    epoch,
                    Arc::clone(frame),
                    origin.elapsed().as_millis() as u64,
                    image,
                );
            }
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    Ok(())
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

#[cfg(debug_assertions)]
pub(in crate::overlay::screen_translate) fn test_image(path: std::path::PathBuf) {
    std::thread::spawn(move || {
        let result = (|| -> Result<()> {
            anyhow::ensure!(path.is_absolute(), "absolute image path required");
            let image = Arc::new(image::open(path)?.to_rgba8());
            let observation =
                observation::read(&image, &std::sync::atomic::AtomicBool::new(false))?;
            let origin = super::super::capture::image_origin(image.width(), image.height());
            let frame =
                observation::prepare(Arc::clone(&image), &observation, Instant::now(), origin)?;
            let mut slot = super::SESSION.lock().unwrap();
            let rect = windows::Win32::Foundation::RECT {
                left: origin.0,
                top: origin.1,
                right: origin.0 + image.width() as i32,
                bottom: origin.1 + image.height() as i32,
            };
            let session = if let Some(session) = slot.as_ref() {
                anyhow::ensure!(
                    session.native_window.load(Ordering::Acquire) == 0,
                    "stop interactive subtitle session before replay"
                );
                Arc::clone(session)
            } else {
                let session = super::new_session(rect, rect, "en".into(), String::new());
                session.started.store(true, Ordering::Release);
                *slot = Some(Arc::clone(&session));
                let translator = Arc::clone(&session);
                std::thread::spawn(move || super::pipeline::run(translator));
                session
            };
            drop(slot);
            let epoch = {
                let mut view = session.view.lock().unwrap();
                if view.rect != rect {
                    view.rect = rect;
                    view.epoch += 1;
                }
                view.epoch
            };
            static CLOCK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let now = CLOCK.fetch_add(1000, Ordering::Relaxed);
            for offset in [0, 400] {
                apply(&session, epoch, Arc::clone(&frame), now + offset, &image);
            }
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
