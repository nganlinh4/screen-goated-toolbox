//! Transient, render-only processing effects. They never enter card/input state.
use super::protocol::{HostCommand, ProcessingVisual, SceneRect};
use std::collections::BTreeMap;
use std::sync::{
    LazyLock, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::RECT;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

struct Active {
    visual: ProcessingVisual,
    started: Instant,
}
static ACTIVE: LazyLock<Mutex<BTreeMap<u64, Active>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
static CHILD: LazyLock<Mutex<BTreeMap<u64, ProcessingVisual>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static CONTROL_RECTS: LazyLock<Mutex<BTreeMap<u64, Vec<SceneRect>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

pub struct ProcessingGlow {
    id: u64,
}

impl ProcessingGlow {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn bind_controls(id: u64, controller: isize) {
        let _dispatch = super::parent::SCENE_DISPATCH.lock().unwrap();
        let mut active = ACTIVE.lock().unwrap();
        if let Some(entry) = active.get_mut(&id) {
            entry.visual.control_id = Some(controller);
        }
        drop(active);
        publish();
    }

    pub fn show(rect: RECT) -> anyhow::Result<Self> {
        anyhow::ensure!(
            rect.right > rect.left && rect.bottom > rect.top,
            "empty processing rectangle"
        );
        let _dispatch = super::parent::SCENE_DISPATCH.lock().unwrap();
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let visual = ProcessingVisual {
            id,
            control_id: None,
            rect: SceneRect {
                x: rect
                    .left
                    .saturating_sub(unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) }),
                y: rect
                    .top
                    .saturating_sub(unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) }),
                width: rect.right.saturating_sub(rect.left),
                height: rect.bottom.saturating_sub(rect.top),
            },
            cells: Vec::new(),
            revision: 0,
            elapsed_ms: 0,
            closing: false,
            finish: false,
        };
        {
            let mut active = ACTIVE.lock().unwrap();
            // A new capture can overlap the preceding effect's short exit only.
            while active.len() >= 4 {
                active.pop_first();
            }
            active.insert(
                id,
                Active {
                    visual,
                    started: Instant::now(),
                },
            );
        }
        publish();
        Ok(Self { id })
    }

    pub fn set_cells(&self, cells: Vec<[i32; 4]>) {
        let _dispatch = super::parent::SCENE_DISPATCH.lock().unwrap();
        let mut active = ACTIVE.lock().unwrap();
        let Some(entry) = active.get_mut(&self.id) else {
            return;
        };
        if entry.visual.closing {
            return;
        }
        let cells = cells
            .into_iter()
            .filter(|r| r[2] > 0 && r[3] > 0)
            .collect::<Vec<_>>();
        if entry.visual.revision > 0 && entry.visual.cells == cells {
            return;
        }
        entry.visual.cells = cells;
        entry.visual.revision += 1;
        drop(active);
        publish();
    }

    pub fn close(mut self) {
        self.begin_close(true);
    }

    fn begin_close(&mut self, finish: bool) {
        let id = std::mem::take(&mut self.id);
        if id == 0 {
            return;
        }
        let _dispatch = super::parent::SCENE_DISPATCH.lock().unwrap();
        let mut active = ACTIVE.lock().unwrap();
        let Some(entry) = active.get_mut(&id) else {
            return;
        };
        entry.visual.closing = true;
        entry.visual.finish = finish;
        drop(active);
        publish();
        // The renderer acknowledges its animated exit. This upper bound also
        // removes an effect whose renderer vanished before acknowledging it.
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            finished(id);
        });
    }
}

impl Drop for ProcessingGlow {
    fn drop(&mut self) {
        self.begin_close(false);
    }
}

pub(super) fn snapshot() -> HostCommand {
    HostCommand::Processing {
        effects: ACTIVE
            .lock()
            .unwrap()
            .values()
            .map(|entry| {
                let mut visual = entry.visual.clone();
                visual.elapsed_ms = entry
                    .started
                    .elapsed()
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64;
                visual
            })
            .collect(),
    }
}

fn publish() {
    super::delivery::send_command(snapshot());
}

pub(super) fn finished(id: u64) {
    let _dispatch = super::parent::SCENE_DISPATCH.lock().unwrap();
    let mut active = ACTIVE.lock().unwrap();
    if !active.get(&id).is_some_and(|entry| entry.visual.closing) {
        return;
    }
    active.remove(&id);
    drop(active);
    publish();
}

pub(super) fn apply(effects: &[ProcessingVisual]) {
    CONTROL_RECTS
        .lock()
        .unwrap()
        .retain(|id, _| effects.iter().any(|effect| effect.id == *id));
    *CHILD.lock().unwrap() = effects
        .iter()
        .cloned()
        .map(|effect| (effect.id, effect))
        .collect();
}

pub(super) fn child_finished(id: u64) {
    let mut effects = CHILD.lock().unwrap();
    if effects.get(&id).is_some_and(|effect| effect.closing) {
        effects.remove(&id);
        CONTROL_RECTS.lock().unwrap().remove(&id);
    }
}

pub(super) fn visual_rects() -> Vec<SceneRect> {
    let mut rects: Vec<_> = CHILD
        .lock()
        .unwrap()
        .values()
        .map(|effect| effect.rect.clone())
        .collect();
    rects.extend(CONTROL_RECTS.lock().unwrap().values().flatten().cloned());
    rects
}

pub(super) fn control_regions(message: &serde_json::Value) {
    let Some(id) = message.get("id").and_then(|value| value.as_u64()) else {
        return;
    };
    let effects = CHILD.lock().unwrap();
    if !effects
        .get(&id)
        .is_some_and(|effect| effect.control_id.is_some())
    {
        return;
    }
    let rects = message
        .get("rects")
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<SceneRect>>(value).ok())
        .unwrap_or_default();
    CONTROL_RECTS.lock().unwrap().insert(
        id,
        rects
            .into_iter()
            .filter(|rect| rect.width > 0 && rect.height > 0)
            .take(32)
            .collect(),
    );
}

pub(super) fn is_visible() -> bool {
    !CHILD.lock().unwrap().is_empty()
}

#[cfg(test)]
mod tests {
    use super::super::input_surface::{REGION_LOCK, create_input_surface, update_input_regions};
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, IsWindowVisible};

    #[test]
    fn contour_is_visual_only_and_retires_only_after_its_exit() {
        let _guard = REGION_LOCK.lock().unwrap();
        let mut effect = ProcessingVisual {
            id: 1,
            control_id: Some(7),
            rect: SceneRect {
                x: 50,
                y: 40,
                width: 500,
                height: 300,
            },
            cells: vec![[80, 60, 120, 30]],
            revision: 1,
            elapsed_ms: 200,
            closing: false,
            finish: false,
        };
        let input = create_input_surface(-1920, -200, 3840, 2160).unwrap();
        apply(std::slice::from_ref(&effect));
        assert_eq!(visual_rects(), vec![effect.rect.clone()]);
        control_regions(
            &serde_json::json!({"id": 1, "rects": [{"x": 600, "y": 400, "width": 40, "height": 40}]}),
        );
        assert_eq!(visual_rects().len(), 2);
        let state = update_input_regions(
            input,
            &Default::default(),
            &visual_rects(),
            -1920,
            -200,
            3840,
            2160,
        );
        assert!(state.is_hidden);
        assert_eq!(state.input_region_count, 0);
        assert!(!unsafe { IsWindowVisible(input).as_bool() });
        child_finished(1);
        assert!(
            is_visible(),
            "active processing must not disappear on a stale exit"
        );
        effect.closing = true;
        apply(&[effect]);
        child_finished(1);
        assert!(!is_visible());
        assert!(visual_rects().is_empty());
        control_regions(
            &serde_json::json!({"id": 1, "rects": [{"x": 600, "y": 400, "width": 40, "height": 40}]}),
        );
        assert!(
            visual_rects().is_empty(),
            "late preview cannot revive a completed effect"
        );
        unsafe {
            DestroyWindow(input).unwrap();
        }
    }
}
