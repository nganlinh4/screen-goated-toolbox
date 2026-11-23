//! Download presentation owned by a realtime session and transcription engine.
//!
//! No current engine downloads a model; `begin` is the slot an offline engine
//! uses when one returns (docs/OFFLINE-ENGINES.md).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use super::protocol::DownloadState;
use super::state::{REALTIME_SESSION_ID, TRANSCRIPTION_ENGINE_REVISION};

#[derive(Default)]
struct Activities {
    #[expect(
        dead_code,
        reason = "extension point for a returning offline engine; see docs/OFFLINE-ENGINES.md"
    )]
    next: u64,
    entries: BTreeMap<u64, Activity>,
}

struct Activity {
    session: u64,
    engine: u64,
    stop: Arc<AtomicBool>,
    key: Option<&'static str>,
    state: DownloadState,
}

static ACTIVITIES: LazyLock<Mutex<Activities>> =
    LazyLock::new(|| Mutex::new(Activities::default()));

#[expect(
    dead_code,
    reason = "extension point for a returning offline engine; see docs/OFFLINE-ENGINES.md"
)]
pub(crate) struct Guard(u64);

#[expect(
    dead_code,
    reason = "extension point for a returning offline engine; see docs/OFFLINE-ENGINES.md"
)]
pub(crate) fn begin(
    session: u64,
    engine: u64,
    stop: Arc<AtomicBool>,
    key: Option<&'static str>,
    title: &str,
    message: &str,
) -> Guard {
    let mut activities = ACTIVITIES.lock().unwrap_or_else(|error| error.into_inner());
    activities.next = activities
        .next
        .checked_add(1)
        .expect("download identity exhausted");
    let id = activities.next;
    activities.entries.insert(
        id,
        Activity {
            session,
            engine,
            stop,
            key,
            state: DownloadState {
                active: true,
                title: title.to_owned(),
                message: message.to_owned(),
                progress: 0.0,
            },
        },
    );
    drop(activities);
    notify();
    Guard(id)
}

impl Guard {
    #[expect(
        dead_code,
        reason = "extension point for a returning offline engine; see docs/OFFLINE-ENGINES.md"
    )]
    pub(crate) fn report(&self, progress: f32) {
        if !progress.is_finite() {
            return;
        }
        if let Some(activity) = ACTIVITIES
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entries
            .get_mut(&self.0)
        {
            activity.state.progress = progress.clamp(0.0, 100.0);
        }
        notify();
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        ACTIVITIES
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entries
            .remove(&self.0);
        notify();
    }
}

fn select(
    activities: &Activities,
    session: u64,
    engine: u64,
    snapshot: impl Fn(&str) -> Option<f32>,
) -> DownloadState {
    for activity in activities.entries.values().rev() {
        if activity.session != session
            || activity.engine != engine
            || activity.stop.load(Ordering::SeqCst)
        {
            continue;
        }
        let mut state = activity.state.clone();
        if let Some(key) = activity.key {
            let Some(progress) = snapshot(key) else {
                continue;
            };
            state.progress = progress.max(0.0);
        }
        return state;
    }
    DownloadState::default()
}

pub(crate) fn current() -> DownloadState {
    let activities = ACTIVITIES.lock().unwrap_or_else(|error| error.into_inner());
    select(
        &activities,
        REALTIME_SESSION_ID.load(Ordering::SeqCst),
        TRANSCRIPTION_ENGINE_REVISION.load(Ordering::SeqCst),
        crate::install_progress::snapshot,
    )
}

pub(crate) fn notify() {
    unsafe {
        let hwnd = std::ptr::addr_of!(super::state::REALTIME_HWND).read();
        if !hwnd.is_invalid() {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                Some(hwnd),
                crate::api::realtime_audio::WM_DOWNLOAD_PROGRESS,
                windows::Win32::Foundation::WPARAM(0),
                windows::Win32::Foundation::LPARAM(0),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn activity(session: u64, engine: u64, key: Option<&'static str>, title: &str) -> Activity {
        Activity {
            session,
            engine,
            stop: Arc::new(AtomicBool::new(false)),
            key,
            state: DownloadState {
                active: true,
                title: title.into(),
                message: String::new(),
                progress: 5.0,
            },
        }
    }

    #[test]
    fn retired_engine_cannot_hide_or_replace_current_download() {
        let mut activities = Activities::default();
        activities.entries.insert(1, activity(7, 2, None, "old"));
        activities
            .entries
            .insert(2, activity(7, 3, None, "current"));
        activities
            .entries
            .insert(3, activity(7, 2, None, "delayed old"));
        assert_eq!(select(&activities, 7, 3, |_| None).title, "current");
        // Source, language, and model changes all advance the same engine revision.
        assert!(!select(&activities, 7, 4, |_| None).active);
        activities.entries.remove(&1);
        activities.entries.remove(&3);
        assert_eq!(select(&activities, 7, 3, |_| None).title, "current");
        assert!(!select(&activities, 8, 3, |_| None).active);
        activities
            .entries
            .get(&2)
            .unwrap()
            .stop
            .store(true, Ordering::SeqCst);
        assert!(!select(&activities, 7, 3, |_| None).active);
    }

    #[test]
    fn keyed_scope_only_displays_while_that_component_is_active() {
        let mut activities = Activities::default();
        activities
            .entries
            .insert(1, activity(1, 1, Some("component"), "model"));
        assert!(!select(&activities, 1, 1, |_| None).active);
        let selected = select(&activities, 1, 1, |key| {
            (key == "component").then_some(42.0)
        });
        assert_eq!(selected.progress, 42.0);
    }
}
