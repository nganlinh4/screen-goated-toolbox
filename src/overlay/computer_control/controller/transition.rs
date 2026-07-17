use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::world::{IndexedElement, SurfaceIdentity, WorldState};
use super::{Surface, Verb};

const ACTIVATION_VERIFY_TIMEOUT: Duration = Duration::from_millis(300);
const ACTIVATION_POLL_INTERVAL: Duration = Duration::from_millis(30);

pub(super) struct Observation {
    pub world: Option<WorldState>,
    pub observation_error: Option<String>,
    pub foreground_changed: bool,
    pub title_changed: bool,
    pub structure_changed: bool,
    pub context_changed: bool,
}

pub(super) fn requires_context_change(
    verb: Verb,
    target: &IndexedElement,
    identity: &SurfaceIdentity,
) -> bool {
    let browser_link = matches!(identity, SurfaceIdentity::Browser { .. })
        && target.role == "link"
        && matches!(verb, Verb::Click | Verb::Activate);
    let native_collection = matches!(identity, SurfaceIdentity::Native { .. })
        && verb == Verb::Activate
        && matches!(target.role.as_str(), "listitem" | "treeitem");
    browser_link || native_collection
}

pub(super) fn dispatched_effect_status(
    context_required: bool,
    context_changed: bool,
) -> (bool, bool) {
    (!context_required || context_changed, true)
}

/// Observe once for ordinary actions. Collection activation is the sole bounded
/// polling case because the opened view may not become accessible synchronously.
pub(super) fn observe_after_action(
    surface: &mut dyn Surface,
    before: &WorldState,
    target: &IndexedElement,
    verb: Verb,
    before_surface: &Value,
) -> Observation {
    observe_after_action_with(surface, before, target, verb, before_surface, || {
        super::super::uia::input_target_snapshot()
    })
}

fn observe_after_action_with(
    surface: &mut dyn Surface,
    before: &WorldState,
    target: &IndexedElement,
    verb: Verb,
    before_surface: &Value,
    mut snapshot: impl FnMut() -> Value,
) -> Observation {
    let poll = requires_context_change(verb, target, &before.identity);
    let deadline = Instant::now() + ACTIVATION_VERIFY_TIMEOUT;
    let before_structure = world_structure(before);
    loop {
        let (world, observation_error) = match surface.observe() {
            Ok(world) => (Some(world), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let after_surface = snapshot();
        let foreground_changed = foreground_changed(before_surface, &after_surface);
        let world_location_changed = world
            .as_ref()
            .is_some_and(|after| after.title != before.title || after.url != before.url);
        let native_surface = matches!(before.identity, SurfaceIdentity::Native { .. });
        let title_changed = world_location_changed
            || (native_surface && before_surface.get("title") != after_surface.get("title"));
        let structure_changed = world
            .as_ref()
            .is_some_and(|after| world_structure(after) != before_structure);
        let identity_changed = world
            .as_ref()
            .is_some_and(|after| after.identity != before.identity);
        let context_changed = (native_surface && foreground_changed)
            || title_changed
            || structure_changed
            || identity_changed;
        if !poll || context_changed || Instant::now() >= deadline {
            return Observation {
                world,
                observation_error,
                foreground_changed,
                title_changed,
                structure_changed,
                context_changed,
            };
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        thread::sleep(ACTIVATION_POLL_INTERVAL.min(remaining));
    }
}

pub(super) fn world_structure(world: &WorldState) -> Vec<(String, String, Option<i32>, bool)> {
    let mut structure: Vec<_> = world
        .elements
        .iter()
        .map(|element| {
            (
                element.role.clone(),
                element.name.clone(),
                element.form,
                element.submit,
            )
        })
        .collect();
    structure.sort_unstable();
    structure
}

pub(super) fn matching_state<'a>(
    world: &'a WorldState,
    target: &IndexedElement,
) -> Option<&'a str> {
    world
        .elements
        .iter()
        .find(|element| element.role == target.role && element.name == target.name)
        .and_then(|element| element.state.as_deref())
}

fn foreground_changed(before: &Value, after: &Value) -> bool {
    before.get("hwnd") != after.get("hwnd") || before.get("pid") != after.get("pid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::computer_control::controller::verify::ReadBack;
    use crate::overlay::computer_control::controller::world::{
        BrowserWindowIdentity, ElHandle, SurfaceIdentity,
    };
    use crate::overlay::computer_control::controller::{ActCtx, Surface};

    fn element(id: u32, role: &str, name: &str) -> IndexedElement {
        IndexedElement {
            id,
            role: role.to_string(),
            name: name.to_string(),
            value: None,
            editable: role == "textbox",
            state: None,
            enabled: true,
            required: false,
            submit: false,
            form: None,
            risk: None,
            handle: ElHandle::Native {
                cx: 1,
                cy: 1,
                provider_name: name.into(),
                automation_id: format!("element-{id}"),
                runtime_id: vec![id as i32],
            },
        }
    }

    fn browser_identity(document_id: &str) -> SurfaceIdentity {
        SurfaceIdentity::Browser {
            tab_id: 19,
            document_id: document_id.to_string(),
            window: BrowserWindowIdentity {
                browser_window_id: 7,
                hwnd: 1,
                pid: 2,
                generation: 3,
            },
        }
    }

    #[test]
    fn structure_comparison_ignores_enumeration_order() {
        let identity = SurfaceIdentity::Native {
            hwnd: 1,
            pid: 2,
            generation: 3,
        };
        let first = WorldState {
            elements: vec![element(1, "button", "one"), element(2, "link", "two")],
            url: None,
            title: None,
            identity: identity.clone(),
        };
        let second = WorldState {
            elements: vec![element(9, "link", "two"), element(4, "button", "one")],
            url: None,
            title: None,
            identity,
        };

        assert_eq!(world_structure(&first), world_structure(&second));
    }

    struct DelayedSurface {
        calls: usize,
        before: WorldState,
        after: WorldState,
        transition_on: usize,
    }

    impl Surface for DelayedSurface {
        fn identity(&mut self) -> anyhow::Result<SurfaceIdentity> {
            Ok(self.before.identity.clone())
        }

        fn observe(&mut self) -> anyhow::Result<WorldState> {
            self.calls += 1;
            Ok(if self.calls >= self.transition_on {
                self.after.clone()
            } else {
                self.before.clone()
            })
        }

        fn execute(
            &mut self,
            _el: &IndexedElement,
            _verb: Verb,
            _value: Option<&str>,
            _act: &ActCtx,
            _expected: &SurfaceIdentity,
        ) -> anyhow::Result<Value> {
            unreachable!()
        }

        fn read_back(&mut self, _el: &IndexedElement) -> ReadBack {
            ReadBack::default()
        }
    }

    fn delayed_surface() -> (DelayedSurface, IndexedElement) {
        let item = element(1, "listitem", "entry");
        let before = WorldState {
            elements: vec![item.clone()],
            url: None,
            title: None,
            identity: SurfaceIdentity::Native {
                hwnd: 1,
                pid: 2,
                generation: 3,
            },
        };
        let after = WorldState {
            elements: vec![element(2, "button", "opened")],
            url: None,
            title: None,
            identity: SurfaceIdentity::Native {
                hwnd: 3,
                pid: 2,
                generation: 4,
            },
        };
        (
            DelayedSurface {
                calls: 0,
                before,
                after,
                transition_on: 3,
            },
            item,
        )
    }

    #[test]
    fn activation_polls_until_delayed_context_is_observable() {
        let (mut surface, item) = delayed_surface();
        let before = surface.before.clone();
        let stable = serde_json::json!({"hwnd": 1, "pid": 2, "title": "same"});
        let observed = observe_after_action_with(
            &mut surface,
            &before,
            &item,
            Verb::Activate,
            &stable,
            || stable.clone(),
        );

        assert!(observed.context_changed);
        assert_eq!(surface.calls, 3);
    }

    #[test]
    fn ordinary_click_does_not_pay_activation_poll_latency() {
        let (mut surface, item) = delayed_surface();
        let before = surface.before.clone();
        let stable = serde_json::json!({"hwnd": 1, "pid": 2, "title": "same"});
        let observed =
            observe_after_action_with(&mut surface, &before, &item, Verb::Click, &stable, || {
                stable.clone()
            });

        assert!(!observed.context_changed);
        assert_eq!(surface.calls, 1);
    }

    #[test]
    fn browser_link_click_polls_until_location_changes() {
        let link = element(1, "link", "destination");
        let identity = browser_identity("document-a");
        let before = WorldState {
            elements: vec![link.clone()],
            url: Some("https://example.invalid/one".to_string()),
            title: Some("one".to_string()),
            identity: identity.clone(),
        };
        let after = WorldState {
            elements: vec![link.clone()],
            url: Some("https://example.invalid/two".to_string()),
            title: Some("two".to_string()),
            identity,
        };
        let mut surface = DelayedSurface {
            calls: 0,
            before: before.clone(),
            after,
            transition_on: 3,
        };
        let stable = serde_json::json!({"hwnd": 1, "pid": 2, "title": "browser"});
        let observed =
            observe_after_action_with(&mut surface, &before, &link, Verb::Click, &stable, || {
                stable.clone()
            });

        assert!(observed.context_changed);
        assert_eq!(surface.calls, 3);
    }

    #[test]
    fn only_semantic_browser_links_require_click_context_change() {
        let identity = browser_identity("document-a");
        assert!(requires_context_change(
            Verb::Click,
            &element(1, "link", "destination"),
            &identity,
        ));
        assert!(!requires_context_change(
            Verb::Click,
            &element(2, "button", "menu"),
            &identity,
        ));
    }

    #[test]
    fn unobserved_required_effect_is_not_verified_or_safe_to_repeat() {
        assert_eq!(dispatched_effect_status(true, false), (false, true));
        assert_eq!(dispatched_effect_status(true, true), (true, true));
        assert_eq!(dispatched_effect_status(false, false), (true, true));
    }

    #[test]
    fn unrelated_desktop_focus_does_not_verify_a_browser_transition() {
        let item = element(1, "listitem", "entry");
        let world = WorldState {
            elements: vec![item.clone()],
            url: Some("https://example.invalid/one".to_string()),
            title: Some("stable".to_string()),
            identity: browser_identity("doc-a"),
        };
        let mut surface = DelayedSurface {
            calls: 0,
            before: world.clone(),
            after: world.clone(),
            transition_on: 1,
        };
        let observed = observe_after_action_with(
            &mut surface,
            &world,
            &item,
            Verb::Activate,
            &serde_json::json!({"hwnd": 1, "pid": 2, "title": "first"}),
            || serde_json::json!({"hwnd": 3, "pid": 4, "title": "second"}),
        );

        assert!(observed.foreground_changed);
        assert!(!observed.title_changed);
        assert!(!observed.context_changed);
    }
}
