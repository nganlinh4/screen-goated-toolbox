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
    protected: bool,
    drift_on_fill: bool,
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
            if self.drift_on_fill {
                self.world.title = Some("changed".to_string());
            }
        }
        Ok(json!({
            "ok": true,
            "input_injection": {"fully_inserted": true, "calls": 1}
        }))
    }

    fn read_back(&mut self, element: &IndexedElement, _expected: &SurfaceIdentity) -> ReadBack {
        ReadBack {
            value: (!self.protected)
                .then(|| {
                    self.world
                        .elements
                        .iter()
                        .find(|item| item.name == element.name)
                        .and_then(|item| item.value.clone())
                })
                .flatten(),
            validity: None,
            selected_text: None,
            protected: self.protected,
        }
    }
}

struct CoverageFailureSurface {
    world: WorldState,
    observations_before_failure: usize,
    executed: bool,
}

impl Surface for CoverageFailureSurface {
    fn identity(&mut self) -> anyhow::Result<SurfaceIdentity> {
        Ok(self.world.identity.clone())
    }

    fn observe(&mut self) -> anyhow::Result<WorldState> {
        if self.observations_before_failure == 0 {
            return Err(super::super::browser::incomplete_frame_coverage(
                "frame unavailable",
            ));
        }
        self.observations_before_failure -= 1;
        Ok(self.world.clone())
    }

    fn execute(
        &mut self,
        _: &IndexedElement,
        _: Verb,
        _: Option<&str>,
        _: &ActCtx,
        _: &SurfaceIdentity,
    ) -> anyhow::Result<Value> {
        self.executed = true;
        Ok(json!({"ok": true}))
    }

    fn read_back(&mut self, _: &IndexedElement, _: &SurfaceIdentity) -> ReadBack {
        ReadBack::default()
    }
}

#[test]
fn sequence_preserves_frame_coverage_errors_before_and_after_dispatch() {
    let initial = world(SurfaceIdentity::Native {
        hwnd: 9,
        pid: 4,
        generation: 1,
    });
    let cancel = AtomicBool::new(false);
    let profile = HumanProfile::instant();
    let act = ActCtx {
        profile: &profile,
        cancel: &cancel,
        dry: false,
    };
    for planned in [false, true] {
        let mut controller = Controller {
            last: planned.then(|| initial.clone()),
            ..Controller::default()
        };
        let mut surface = CoverageFailureSurface {
            world: initial.clone(),
            observations_before_failure: 0,
            executed: false,
        };
        let result =
            controller.do_steps_on(&[json!({"id": 1, "verb": "click"})], &act, &mut surface);
        assert_eq!(result["code"], "ERR_BROWSER_FRAME_COVERAGE_INCOMPLETE");
        assert_eq!(result["effect_may_have_occurred"], false);
        assert!(!surface.executed);
        assert!(controller.last.is_none());
    }
    let mut controller = Controller {
        last: Some(initial.clone()),
        ..Controller::default()
    };
    let mut surface = CoverageFailureSurface {
        world: initial,
        observations_before_failure: 1,
        executed: false,
    };
    let result = controller.do_steps_on(&[json!({"id": 1, "verb": "click"})], &act, &mut surface);
    assert!(surface.executed);
    assert_eq!(result["ok"], false);
    assert_eq!(result["effect_may_have_occurred"], true);
    assert_eq!(
        result["receipts"][0]["observation_error"]["code"],
        "ERR_BROWSER_FRAME_COVERAGE_INCOMPLETE"
    );
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
        value_preview: None,
        editable: role == "textbox",
        protected: false,
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
        protected: false,
        drift_on_fill: false,
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
fn protected_fill_continues_with_an_explicitly_unverified_receipt() {
    let initial = world(SurfaceIdentity::Native {
        hwnd: 9,
        pid: 4,
        generation: 1,
    });
    let mut controller = Controller {
        last: Some(initial.clone()),
        ..Controller::default()
    };
    let mut surface = FakeSurface {
        world: initial,
        executed: Vec::new(),
        protected: true,
        drift_on_fill: false,
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
    assert_eq!(surface.executed, vec![Verb::Fill, Verb::Submit]);
    assert_eq!(
        result["receipts"][0]["verification_status"],
        "protected_unverified"
    );
    assert_eq!(result["receipts"][0]["effect_verified"], false);
    assert_eq!(result["effect_status"], "may_have_occurred");
}

#[test]
fn single_protected_fill_reports_dispatch_without_claiming_verification() {
    let initial = world(SurfaceIdentity::Native {
        hwnd: 9,
        pid: 4,
        generation: 1,
    });
    let mut controller = Controller {
        last: Some(initial.clone()),
        ..Controller::default()
    };
    let mut surface = FakeSurface {
        world: initial,
        executed: Vec::new(),
        protected: true,
        drift_on_fill: false,
    };
    let cancel = AtomicBool::new(false);
    let profile = HumanProfile::instant();
    let act = ActCtx {
        profile: &profile,
        cancel: &cancel,
        dry: false,
    };
    let result = controller.act_on(1, "fill", Some("ready"), false, &act, &mut surface);
    assert_eq!(surface.executed, vec![Verb::Fill]);
    assert_eq!(result["ok"], true);
    assert_eq!(result["dispatch_ok"], true);
    assert_eq!(result["effect_verified"], false);
    assert_eq!(result["effect_may_have_occurred"], true);
    assert_eq!(result["verification_status"], "protected_unverified");
}

#[test]
fn unrelated_context_change_does_not_verify_protected_fill() {
    let initial = world(SurfaceIdentity::Native {
        hwnd: 9,
        pid: 4,
        generation: 1,
    });
    let mut controller = Controller {
        last: Some(initial.clone()),
        ..Controller::default()
    };
    let mut surface = FakeSurface {
        world: initial,
        executed: Vec::new(),
        protected: true,
        drift_on_fill: true,
    };
    let cancel = AtomicBool::new(false);
    let profile = HumanProfile::instant();
    let act = ActCtx {
        profile: &profile,
        cancel: &cancel,
        dry: false,
    };
    let result = controller.do_steps_on(
        &[json!({"id": 1, "verb": "fill", "value": "ready"})],
        &act,
        &mut surface,
    );
    assert_eq!(result["receipts"][0]["transition"]["context_changed"], true);
    assert_eq!(result["receipts"][0]["effect_verified"], false);
    assert_eq!(result["effect_status"], "may_have_occurred");
}

#[test]
fn planned_fill_rejects_protection_changes_and_replaced_controls_before_dispatch() {
    let identity = SurfaceIdentity::Native {
        hwnd: 9,
        pid: 4,
        generation: 1,
    };
    for (before, after, replace) in [
        (false, true, false),
        (true, false, false),
        (false, false, true),
    ] {
        let mut planned = world(identity.clone());
        planned.elements[0].protected = before;
        let mut current = planned.clone();
        current.elements[0].protected = after;
        if replace {
            let ElHandle::Native { runtime_id, .. } = &mut current.elements[0].handle else {
                unreachable!()
            };
            *runtime_id = vec![99, 7];
        }
        let mut controller = Controller {
            last: Some(planned),
            ..Controller::default()
        };
        let mut surface = FakeSurface {
            world: current,
            executed: Vec::new(),
            protected: after,
            drift_on_fill: false,
        };
        let cancel = AtomicBool::new(false);
        let profile = HumanProfile::instant();
        let act = ActCtx {
            profile: &profile,
            cancel: &cancel,
            dry: false,
        };
        let result = controller.do_steps_on(
            &[json!({"id": 1, "verb": "fill", "value": "ready"})],
            &act,
            &mut surface,
        );
        assert_eq!(result["ok"], false);
        assert_eq!(result["effect_may_have_occurred"], false);
        assert!(surface.executed.is_empty());
    }
}

#[test]
fn browser_sequence_allows_renumbering_but_rejects_same_label_replacement() {
    let identity = SurfaceIdentity::Browser {
        tab_id: 7,
        document_id: "doc".into(),
        window: browser_window(),
    };
    for replaced in [false, true] {
        let mut planned = world(identity.clone());
        planned.elements[0].handle = ElHandle::Browser {
            selector: "[data-sgt-id=\"1\"]".into(),
            session: None,
            tab_id: 7,
            document_id: "doc".into(),
            element_id: "original".into(),
        };
        let mut current = planned.clone();
        current.elements[0].handle = ElHandle::Browser {
            selector: "[data-sgt-id=\"9\"]".into(),
            session: None,
            tab_id: 7,
            document_id: "doc".into(),
            element_id: if replaced { "replacement" } else { "original" }.into(),
        };
        let mut controller = Controller {
            last: Some(planned),
            ..Controller::default()
        };
        let mut surface = FakeSurface {
            world: current,
            executed: Vec::new(),
            protected: false,
            drift_on_fill: false,
        };
        let cancel = AtomicBool::new(false);
        let profile = HumanProfile::instant();
        let act = ActCtx {
            profile: &profile,
            cancel: &cancel,
            dry: false,
        };
        let result = controller.do_steps_on(
            &[json!({"id": 1, "verb": "fill", "value": "ready"})],
            &act,
            &mut surface,
        );
        assert_eq!(result["ok"], !replaced);
        assert_eq!(surface.executed.len(), usize::from(!replaced));
    }
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
        protected: false,
        drift_on_fill: false,
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
