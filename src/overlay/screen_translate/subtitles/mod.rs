//! Persistent subtitle session. Editor visibility is independent from processing lifetime.
mod capture;
mod change_detection;
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
pub(super) use engine::test_image;

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

pub(super) struct Session {
    native_window: AtomicIsize,
    pub stop: AtomicBool,
    pub started: AtomicBool,
    pub view: Mutex<View>,
    scene: Mutex<tracks::Scene>,
    wake: Condvar,
}

pub(crate) fn toggle(binding: usize) {
    let hotkey = crate::APP
        .lock()
        .ok()
        .and_then(|app| {
            app.config
                .screen_translate
                .subtitle_hotkeys
                .get(binding)
                .map(|key| key.display_name())
        })
        .unwrap_or_default();
    let slot = SESSION.lock().unwrap();
    if let Some(session) = slot.as_ref() {
        let mut view = session.view.lock().unwrap();
        view.editing = !view.editing;
        view.hotkey = hotkey;
    } else {
        drop(slot);
        select_region(hotkey);
        return;
    }
    drop(slot);
    repaint();
}

fn select_region(hotkey: String) {
    if !crate::overlay::try_claim_capture() {
        return;
    }
    std::thread::spawn(move || {
        crate::initialization::init_com_and_dpi();
        let result = (|| -> anyhow::Result<Option<RECT>> {
            let capture = crate::screen_capture::capture_screen_fast()?;
            crate::APP
                .lock()
                .map_err(|_| anyhow::anyhow!("settings unavailable"))?
                .screenshot_handle = Some(capture);
            crate::overlay::selection::draw_region()
        })();
        if let Ok(mut app) = crate::APP.lock() {
            app.screenshot_handle = None;
        }
        match result {
            Ok(Some(rect)) => {
                if let Err(error) = start_region(rect, hotkey) {
                    super::capture::notify_error(&error.to_string());
                }
            }
            Ok(None) => {}
            Err(error) => super::capture::notify_error(&error.to_string()),
        }
        crate::overlay::set_is_busy(false);
        unsafe {
            windows::Win32::System::Com::CoUninitialize();
        }
    });
}

fn start_region(rect: RECT, hotkey: String) -> anyhow::Result<()> {
    let mut slot = SESSION.lock().unwrap();
    if slot.is_some() {
        return Ok(());
    }
    let (rect, bounds) = monitor_region(rect)?;
    let language = crate::APP
        .lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".into());
    let session = new_session(rect, bounds, language, hotkey);
    *slot = Some(Arc::clone(&session));
    ui::launch(session);
    drop(slot);
    repaint();
    Ok(())
}

fn new_session(rect: RECT, bounds: RECT, language: String, hotkey: String) -> Arc<Session> {
    Arc::new(Session {
        native_window: AtomicIsize::new(0),
        stop: AtomicBool::new(false),
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
    })
}

pub(crate) fn stop() {
    stop_expected(None);
}

fn stop_expected(expected: Option<&Arc<Session>>) {
    let mut slot = SESSION.lock().unwrap();
    if expected.is_some_and(|expected| {
        !slot
            .as_ref()
            .is_some_and(|live| Arc::ptr_eq(live, expected))
    }) {
        return;
    }
    let session = slot.take();
    if let Some(session) = session {
        session.shutdown();
        ui::close(&session);
        crate::log_info!("[Subtitles] stopped; queued work and owned result cancelled");
    }
    crate::overlay::result::scene_compositor::exclude_from_capture(false);
    drop(slot);
    repaint();
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
