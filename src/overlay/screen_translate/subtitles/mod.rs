//! Persistent subtitle session. Editor visibility is independent from processing lifetime.
#[cfg(debug_assertions)]
pub(super) mod acceptance;
mod capture;
mod capture_lifetime;
mod change_detection;
mod display_change;
mod editor_font;
mod editor_layout;
mod editor_paint;
mod editor_surface;
mod engine;
mod evidence;
mod history;
mod observation;
mod pipeline;
mod temporal;
#[cfg(test)]
mod tests;
mod tracks;
#[cfg(test)]
mod tracks_tests;
pub(crate) mod ui;
#[cfg(debug_assertions)]
pub(super) use engine::{
    admit_lab_effect, diagnostic_idle_snapshot, reserve_lab_request, test_image,
    with_lab_generation,
};

use std::sync::{
    Arc, Condvar, LazyLock, Mutex,
    atomic::{AtomicBool, AtomicIsize, Ordering},
};
use windows::Win32::{Foundation::*, Graphics::Gdi::*};

static SESSION: LazyLock<Mutex<Option<Arc<Session>>>> = LazyLock::new(|| Mutex::new(None));

#[derive(Clone)]
pub(super) struct View {
    pub rect: RECT,
    pub bounds: RECT,
    pub editing: bool,
    pub error: String,
    pub epoch: u64,
    pub language: String,
    pub hotkey: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SessionKind {
    Interactive,
    #[cfg(debug_assertions)]
    Diagnostic,
}

pub(super) struct Session {
    kind: SessionKind,
    native_window: AtomicIsize,
    pub stop: Arc<AtomicBool>,
    pub started: AtomicBool,
    pub view: Mutex<View>,
    scene: Mutex<tracks::Scene>,
    wake: Condvar,
    /// Renderer failures repeat per subtitle line; the user is told once.
    renderer_failure_reported: Arc<AtomicBool>,
    capture_lifetime: capture_lifetime::CaptureLifetime,
}

pub(crate) fn toggle(binding: usize) {
    let hotkey = hotkey_name(binding);
    #[cfg(debug_assertions)]
    toggle_debug(hotkey, None);
    #[cfg(not(debug_assertions))]
    {
        let slot = SESSION.lock().unwrap();
        if !toggle_existing(&slot, &hotkey) {
            drop(slot);
            select_region(hotkey);
            return;
        }
        drop(slot);
        repaint();
    }
}

fn hotkey_name(binding: usize) -> String {
    crate::APP
        .lock()
        .ok()
        .and_then(|app| {
            app.config
                .screen_translate
                .subtitle_hotkeys
                .get(binding)
                .map(|key| key.display_name())
        })
        .unwrap_or_default()
}

#[cfg(debug_assertions)]
pub(super) fn toggle_from_lab(binding: usize, observed: super::LabSnapshot) {
    let hotkey = hotkey_name(binding);
    let mut admission = engine::DIAGNOSTIC_ADMISSION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if !admission.accepts_snapshot(observed.subtitle) {
        return;
    }
    let Some(claim) = super::claim_lab_capture(observed) else {
        return;
    };
    let effect = super::runtime::with_lab_start_guard(observed.jobs, || {
        let slot = SESSION.lock().unwrap();
        if toggle_existing(&slot, &hotkey) {
            admission.invalidate();
            return Some(None);
        }
        drop(slot);
        admission.begin_selector().ok().map(Some)
    });
    match effect.flatten() {
        Some(Some(selector)) => {
            claim.transfer();
            drop(admission);
            launch_selector(hotkey, selector);
        }
        Some(None) => {
            drop(claim);
            drop(admission);
            repaint();
        }
        None => {}
    }
}

#[cfg(debug_assertions)]
fn toggle_debug(hotkey: String, observed: Option<u64>) {
    let mut admission = engine::DIAGNOSTIC_ADMISSION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let slot = SESSION.lock().unwrap();
    match toggle_existing_admitted(
        &mut admission,
        &slot,
        &hotkey,
        observed,
        crate::overlay::is_busy(),
    ) {
        None => return,
        Some(true) => {
            drop(slot);
            drop(admission);
            repaint();
            return;
        }
        Some(false) => {}
    }
    drop(slot);
    if !crate::overlay::try_claim_capture() {
        return;
    }
    let selector = match admission.begin_selector() {
        Ok(selector) => selector,
        Err(error) => {
            crate::overlay::set_is_busy(false);
            drop(admission);
            super::capture::notify_error(&error.to_string());
            return;
        }
    };
    drop(admission);
    launch_selector(hotkey, selector);
}

#[cfg(debug_assertions)]
fn toggle_existing_admitted(
    admission: &mut engine::DiagnosticAdmission,
    slot: &Option<Arc<Session>>,
    hotkey: &str,
    observed: Option<u64>,
    busy: bool,
) -> Option<bool> {
    if observed.is_some_and(|token| !lab_request_current(admission, token, busy)) {
        return None;
    }
    let toggled = toggle_existing(slot, hotkey);
    if toggled {
        admission.invalidate();
    }
    Some(toggled)
}

#[cfg(debug_assertions)]
fn lab_request_current(admission: &engine::DiagnosticAdmission, observed: u64, busy: bool) -> bool {
    !busy && admission.accepts_snapshot(observed)
}

fn toggle_existing(slot: &Option<Arc<Session>>, hotkey: &str) -> bool {
    let Some(session) = slot.as_ref() else {
        return false;
    };
    if session.kind != SessionKind::Interactive {
        return false;
    }
    let mut view = session.view.lock().unwrap();
    view.editing = !view.editing;
    view.hotkey = hotkey.to_owned();
    true
}

#[cfg(not(debug_assertions))]
fn select_region(hotkey: String) {
    if !crate::overlay::try_claim_capture() {
        return;
    }
    launch_selector(hotkey);
}

fn launch_selector(hotkey: String, #[cfg(debug_assertions)] selector: u64) {
    std::thread::spawn(move || {
        crate::initialization::init_com_and_dpi();
        let result = (|| -> anyhow::Result<Option<RECT>> {
            let capture = crate::screen_capture::capture_screen_fast()?;
            crate::screen_capture::with_published_capture(
                capture,
                crate::overlay::selection::draw_region,
            )?
        })();
        let finish = |authorized| {
            if authorized {
                match result {
                    Ok(Some(rect)) => {
                        if let Err(error) = start_region(rect, hotkey) {
                            super::capture::notify_error(&error.to_string());
                        }
                    }
                    Ok(None) => {}
                    Err(error) => super::capture::notify_error(&error.to_string()),
                }
            }
        };
        #[cfg(debug_assertions)]
        if let Err(error) = engine::complete_interactive_selector(selector, finish) {
            super::capture::notify_error(&error.to_string());
        }
        #[cfg(not(debug_assertions))]
        finish(true);
        crate::overlay::set_is_busy(false);
        unsafe {
            windows::Win32::System::Com::CoUninitialize();
        }
    });
}

fn start_region(rect: RECT, hotkey: String) -> anyhow::Result<()> {
    let mut slot = SESSION.lock().unwrap();
    if slot
        .as_ref()
        .is_some_and(|session| session.kind == SessionKind::Interactive)
    {
        return Ok(());
    }
    let (rect, bounds) = monitor_region(rect)?;
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".into());
    let session = new_session(rect, bounds, language, hotkey, SessionKind::Interactive);
    if !install_interactive(&mut slot, Arc::clone(&session)) {
        return Ok(());
    }
    ui::launch(session);
    drop(slot);
    repaint();
    Ok(())
}

fn install_interactive(slot: &mut Option<Arc<Session>>, session: Arc<Session>) -> bool {
    if slot
        .as_ref()
        .is_some_and(|existing| existing.kind == SessionKind::Interactive)
    {
        return false;
    }
    if let Some(existing) = slot.take() {
        existing.shutdown();
    }
    *slot = Some(session);
    true
}

fn new_session(
    rect: RECT,
    bounds: RECT,
    language: String,
    hotkey: String,
    kind: SessionKind,
) -> Arc<Session> {
    Arc::new(Session {
        kind,
        native_window: AtomicIsize::new(0),
        stop: Arc::new(AtomicBool::new(false)),
        started: AtomicBool::new(false),
        view: Mutex::new(View {
            rect,
            bounds,
            editing: true,
            error: String::new(),
            epoch: 1,
            language,
            hotkey,
        }),
        scene: Mutex::new(tracks::Scene::default()),
        wake: Condvar::new(),
        renderer_failure_reported: Arc::new(AtomicBool::new(false)),
        capture_lifetime: Default::default(),
    })
}

pub(crate) fn stop() {
    stop_expected(None);
}

#[cfg(debug_assertions)]
pub(super) fn stop_from_lab(observed: super::LabSnapshot) {
    let mut admission = engine::DIAGNOSTIC_ADMISSION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if !admission.accepts_snapshot(observed.subtitle) {
        return;
    }
    let Some(claim) = super::claim_lab_capture(observed) else {
        return;
    };
    let stopped = super::runtime::with_lab_start_guard(observed.jobs, || {
        let mut slot = SESSION.lock().unwrap();
        admission.invalidate();
        stop_locked(&mut slot);
    });
    drop(claim);
    drop(admission);
    if stopped.is_some() {
        repaint();
    }
}

fn matches_stop_owner(slot: &Option<Arc<Session>>, expected: Option<&Arc<Session>>) -> bool {
    expected.is_none_or(|expected| {
        slot.as_ref()
            .is_some_and(|live| Arc::ptr_eq(live, expected))
    })
}

#[cfg(debug_assertions)]
fn admit_stop(
    admission: &mut engine::DiagnosticAdmission,
    slot: &Option<Arc<Session>>,
    expected: Option<&Arc<Session>>,
) -> bool {
    if !matches_stop_owner(slot, expected) {
        return false;
    }
    admission.invalidate();
    true
}

fn stop_expected(expected: Option<&Arc<Session>>) -> bool {
    // Diagnostic replay takes this lock before SESSION. Holding both makes a
    // stop and replay admission one ordered operation, including an empty slot.
    #[cfg(debug_assertions)]
    let mut admission = engine::DIAGNOSTIC_ADMISSION
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let mut slot = SESSION.lock().unwrap();
    #[cfg(debug_assertions)]
    if !admit_stop(&mut admission, &slot, expected) {
        return false;
    }
    #[cfg(not(debug_assertions))]
    if !matches_stop_owner(&slot, expected) {
        return false;
    }
    stop_locked(&mut slot);
    drop(slot);
    #[cfg(debug_assertions)]
    drop(admission);
    repaint();
    true
}

fn stop_locked(slot: &mut Option<Arc<Session>>) {
    let session = slot.take();
    if let Some(session) = session {
        session.shutdown();
        ui::close(&session);
        crate::log_info!("[Subtitles] stopped; queued work and owned result cancelled");
    }
    crate::overlay::result::scene_compositor::exclude_from_capture(false);
}

fn monitor_region(rect: RECT) -> anyhow::Result<(RECT, RECT)> {
    unsafe {
        let monitor = MonitorFromRect(&rect, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        anyhow::ensure!(
            GetMonitorInfoW(monitor, &mut info).as_bool(),
            "selected display is unavailable"
        );
        let b = info.rcMonitor;
        anyhow::ensure!(
            b.right - b.left >= 16 && b.bottom - b.top >= 16,
            "selected display is too small"
        );
        let left = rect.left.clamp(b.left, b.right - 16);
        let top = rect.top.clamp(b.top, b.bottom - 16);
        Ok((
            RECT {
                left,
                right: rect.right.clamp(left + 16, b.right),
                top,
                bottom: rect.bottom.clamp(top + 16, b.bottom),
            },
            b,
        ))
    }
}

pub(super) fn repaint() {
    if let Some(session) = SESSION.lock().unwrap().as_ref() {
        ui::refresh(session);
    }
}

impl Session {
    fn shutdown(&self) {
        let _view = self.view.lock().unwrap();
        self.stop.store(true, Ordering::Release);
        self.cancel_job();
        self.wake.notify_all();
        drop(_view);
        if !self
            .capture_lifetime
            .wait(std::time::Duration::from_secs(5))
        {
            crate::log_info!("[Subtitles] native capture teardown exceeded its deadline");
        }
    }

    fn cancel_job(&self) {
        let mut scene = self.scene.lock().unwrap();
        let epoch = scene.epoch;
        scene.clear(epoch);
    }

    pub(super) fn move_region(&self, rect: RECT) {
        let mut view = self.view.lock().unwrap();
        if view.rect != rect {
            view.rect = rect;
            view.epoch += 1;
            self.cancel_job();
        }
    }
}
