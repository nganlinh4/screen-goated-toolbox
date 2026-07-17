use std::sync::atomic::AtomicBool;

use serde_json::{Value, json};

use super::super::human_input::HumanProfile;
use super::verify::ReadBack;
use super::world::{BrowserWindowIdentity, ElHandle, IndexedElement, SurfaceIdentity, WorldState};
use super::{ActCtx, Controller, Surface, Verb};

fn browser_window() -> BrowserWindowIdentity {
    BrowserWindowIdentity {
        browser_window_id: 7,
        hwnd: 8,
        pid: 9,
        generation: 10,
    }
}

struct FakeSurface {
    world: WorldState,
    executed: Vec<Verb>,
}

impl Surface for FakeSurface {
    fn identity(&mut self) -> anyhow::Result<SurfaceIdentity> {
        Ok(self.world.identity.clone())
    }

    fn observe(&mut self) -> anyhow::Result<WorldState> {
        Ok(self.world.clone())
    }

    fn execute(
        &mut self,
        element: &IndexedElement,
        verb: Verb,
        value: Option<&str>,
        _act: &ActCtx,
        expected: &SurfaceIdentity,
    ) -> anyhow::Result<Value> {
        anyhow::ensure!(&self.world.identity == expected, "stale identity");
        self.executed.push(verb);
        if verb == Verb::Fill {
            let current = self
                .world
                .elements
                .iter_mut()
                .find(|item| item.name == element.name)
                .expect("planned field remains present");
            current.value = value.map(str::to_string);
        }
        Ok(json!({
            "ok": true,
            "input_injection": {"fully_inserted": true, "calls": 1}
        }))
    }

    fn read_back(&mut self, element: &IndexedElement) -> ReadBack {
        ReadBack {
            value: self
                .world
                .elements
                .iter()
                .find(|item| item.name == element.name)
                .and_then(|item| item.value.clone()),
            validity: None,
        }
    }
}

fn element(
    id: u32,
    role: &str,
    name: &str,
    value: Option<&str>,
    required: bool,
    submit: bool,
) -> IndexedElement {
    IndexedElement {
        id,
        role: role.to_string(),
        name: name.to_string(),
        value: value.map(str::to_string),
        editable: role == "textbox",
        state: None,
        enabled: true,
        required,
        submit,
        form: Some(7),
        risk: None,
        handle: ElHandle::Native {
            cx: id as i32,
            cy: id as i32,
            provider_name: name.into(),
            automation_id: format!("element-{id}"),
            runtime_id: vec![id as i32],
        },
    }
}

fn world(identity: SurfaceIdentity) -> WorldState {
    WorldState {
        elements: vec![
            element(1, "textbox", "field", Some(""), true, false),
            element(2, "button", "submit", None, false, true),
        ],
        url: None,
        title: None,
        identity,
    }
}

#[test]
fn do_steps_regates_submit_against_post_fill_world_and_keeps_receipts() {
    let identity = SurfaceIdentity::Native {
        hwnd: 9,
        pid: 4,
        generation: 1,
    };
    let initial = world(identity.clone());
    let mut controller = Controller {
        last: Some(initial.clone()),
        ..Controller::default()
    };
    let mut surface = FakeSurface {
        world: initial,
        executed: Vec::new(),
    };
    let cancel = AtomicBool::new(false);
    let profile = HumanProfile::instant();
    let act = ActCtx {
        profile: &profile,
        cancel: &cancel,
        dry: false,
    };
    let result = controller.do_steps_on(
        &[
            json!({"id": 1, "verb": "fill", "value": "ready"}),
            json!({"id": 2, "verb": "submit"}),
        ],
        &act,
        &mut surface,
    );

    assert_eq!(result["ok"], true);
    assert_eq!(result["completed"], "2/2");
    assert_eq!(result["receipts"].as_array().map(Vec::len), Some(2));
    assert_eq!(surface.executed, vec![Verb::Fill, Verb::Submit]);
}

#[test]
fn do_steps_rejects_same_hwnd_pid_with_a_new_generation_before_any_effect() {
    let planned = world(SurfaceIdentity::Native {
        hwnd: 1,
        pid: 2,
        generation: 1,
    });
    let mut controller = Controller {
        last: Some(planned),
        ..Controller::default()
    };
    let mut surface = FakeSurface {
        world: world(SurfaceIdentity::Native {
            hwnd: 1,
            pid: 2,
            generation: 2,
        }),
        executed: Vec::new(),
    };
    let cancel = AtomicBool::new(false);
    let profile = HumanProfile::instant();
    let act = ActCtx {
        profile: &profile,
        cancel: &cancel,
        dry: false,
    };
    let result = controller.do_steps_on(
        &[json!({"id": 1, "verb": "fill", "value": "x"})],
        &act,
        &mut surface,
    );

    assert_eq!(result["ok"], false);
    assert_eq!(result["stale"], true);
    assert!(surface.executed.is_empty());
}

#[test]
fn exact_browser_target_is_turn_scoped_and_invalidates_cached_ids() {
    let mut controller = Controller {
        last: Some(world(SurfaceIdentity::Native {
            hwnd: 1,
            pid: 2,
            generation: 1,
        })),
        ..Controller::default()
    };
    controller.set_browser_tab_target(Some(73));
    assert_eq!(controller.browser_tab_id, Some(73));
    assert!(controller.last.is_none());

    controller.set_browser_tab_target(None);
    assert_eq!(controller.browser_tab_id, None);
}

#[test]
fn first_exact_browser_observation_is_adopted_without_losing_cached_ids() {
    let identity = SurfaceIdentity::Browser {
        tab_id: 73,
        document_id: "document-1".into(),
        window: browser_window(),
    };
    let mut controller = Controller {
        last: Some(WorldState {
            elements: Vec::new(),
            url: Some("https://example.invalid/".into()),
            title: Some("page".into()),
            identity: identity.clone(),
        }),
        ..Controller::default()
    };

    assert!(controller.adopt_observed_browser_target(&identity));
    assert_eq!(controller.browser_tab_id, Some(73));
    assert_eq!(controller.observed_identity(), Some(&identity));

    let other = SurfaceIdentity::Browser {
        tab_id: 74,
        document_id: "document-2".into(),
        window: browser_window(),
    };
    assert!(!controller.adopt_observed_browser_target(&other));
    assert_eq!(controller.browser_tab_id, Some(73));
    assert_eq!(controller.observed_identity(), Some(&identity));
}

#[test]
fn turn_retirement_and_exact_source_rebinding_preserve_only_matching_ids() {
    let identity = SurfaceIdentity::Browser {
        tab_id: 73,
        document_id: "document-1".into(),
        window: browser_window(),
    };
    let mut controller = Controller {
        last: Some(WorldState {
            elements: Vec::new(),
            url: Some("https://example.invalid/".into()),
            title: Some("page".into()),
            identity: identity.clone(),
        }),
        browser_tab_id: Some(73),
        ..Controller::default()
    };

    controller.release_turn_target();
    assert_eq!(controller.browser_tab_id, None);
    assert_eq!(controller.observed_identity(), Some(&identity));
    assert!(controller.bind_source_surface(Some(&identity)));
    assert_eq!(controller.browser_tab_id, Some(73));
    assert_eq!(controller.observed_identity(), Some(&identity));

    let changed = SurfaceIdentity::Browser {
        tab_id: 73,
        document_id: "document-2".into(),
        window: browser_window(),
    };
    assert!(!controller.bind_source_surface(Some(&changed)));
    assert!(controller.observed_identity().is_none());
}
