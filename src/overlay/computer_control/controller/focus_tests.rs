//! The controller's own pinned-target raise is not an effect of the action.

use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use super::super::human_input::HumanProfile;
use super::verify::ReadBack;
use super::world::{ElHandle, IndexedElement, SurfaceIdentity, WorldState};
use super::{ActCtx, Controller, Surface, Verb};

const OTHER_WINDOW: i64 = 1;
const PINNED_WINDOW: i64 = 2;

/// A pinned native surface that starts behind another window. `raise_in_execute`
/// chooses whether its raise happens in `prepare_input` (the controller's
/// own focus step) or inside `execute` (an effect of the action itself).
struct PinnedSurface {
    world: WorldState,
    foreground: i64,
    raise_in_execute: bool,
}

impl PinnedSurface {
    fn raise(&mut self) {
        self.foreground = PINNED_WINDOW;
    }
}

impl Surface for PinnedSurface {
    fn identity(&mut self) -> anyhow::Result<SurfaceIdentity> {
        Ok(self.world.identity.clone())
    }

    fn observe(&mut self) -> anyhow::Result<WorldState> {
        Ok(self.world.clone())
    }

    fn prepare_input(&mut self, _: Verb, act: &ActCtx, _: &SurfaceIdentity) -> anyhow::Result<()> {
        if !act.dry && !self.raise_in_execute {
            self.raise();
        }
        Ok(())
    }

    fn input_snapshot(&mut self) -> Value {
        json!({"hwnd": self.foreground, "pid": self.foreground, "title": "same"})
    }

    fn execute(
        &mut self,
        _: &IndexedElement,
        _: Verb,
        _: Option<&str>,
        _: &ActCtx,
        _: &SurfaceIdentity,
    ) -> anyhow::Result<Value> {
        if self.raise_in_execute {
            self.raise();
        }
        Ok(json!({"ok": true, "input_injection": {"calls": 1}}))
    }

    fn read_back(&mut self, _: &IndexedElement, _: &SurfaceIdentity) -> ReadBack {
        ReadBack::default()
    }
}

fn pinned_world() -> WorldState {
    WorldState {
        elements: vec![IndexedElement {
            id: 1,
            role: "button".into(),
            name: "Apply".into(),
            value: None,
            value_preview: None,
            editable: false,
            protected: false,
            state: None,
            enabled: true,
            required: false,
            submit: false,
            form: None,
            risk: None,
            handle: ElHandle::Native {
                cx: 10,
                cy: 10,
                provider_name: "Apply".into(),
                automation_id: "apply".into(),
                runtime_id: vec![1],
            },
        }],
        url: None,
        title: Some("Settings".into()),
        identity: SurfaceIdentity::Native {
            hwnd: PINNED_WINDOW as u64,
            pid: PINNED_WINDOW as u64,
            generation: 1,
        },
    }
}

fn click_transition(raise_in_execute: bool) -> Value {
    let world = pinned_world();
    let mut controller = Controller {
        last: Some(world.clone()),
        ..Controller::default()
    };
    let mut surface = PinnedSurface {
        world,
        foreground: OTHER_WINDOW,
        raise_in_execute,
    };
    let cancel = AtomicBool::new(false);
    let profile = HumanProfile::instant();
    let act = ActCtx {
        profile: &profile,
        cancel: &cancel,
        dry: false,
    };
    let result = controller.act_on(1, "click", None, false, &act, &mut surface);
    assert_eq!(result["dispatch_ok"], true, "{result}");
    result["transition"].clone()
}

#[test]
fn raising_the_pinned_target_before_the_snapshot_is_not_a_context_change() {
    let transition = click_transition(false);
    assert_eq!(transition["foreground_changed"], false);
    assert_eq!(transition["context_changed"], false);
}

#[test]
fn a_foreground_change_made_by_the_action_still_counts() {
    let transition = click_transition(true);
    assert_eq!(transition["foreground_changed"], true);
    assert_eq!(transition["context_changed"], true);
}
