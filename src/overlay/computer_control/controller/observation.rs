//! Compact delivery state for indexed observations.
//!
//! The controller keeps the last full element list the model received. A fresh
//! observation with identical identity and model text can then report an
//! unchanged observation id instead of resending thousands of duplicate bytes.

use serde_json::{Value, json};

use super::world::{SurfaceIdentity, WorldState};

#[derive(Default)]
pub(super) struct ObservationCache {
    next_id: u64,
    exposed: Option<ExposedObservation>,
}

struct ExposedObservation {
    id: u64,
    identity: SurfaceIdentity,
    text: String,
    count: usize,
}

pub(super) struct PublishedObservation {
    metadata: Value,
    elements: Option<String>,
}

impl ObservationCache {
    pub(super) fn publish(&mut self, world: &WorldState, force_full: bool) -> PublishedObservation {
        let text = world.to_model_text();
        let unchanged = self
            .exposed
            .as_ref()
            .is_some_and(|last| last.identity == world.identity && last.text == text);
        if unchanged {
            let last = self.exposed.as_ref().expect("unchanged observation exists");
            return PublishedObservation {
                metadata: json!({
                    "id": last.id,
                    "status": if force_full { "full" } else { "unchanged" },
                    "count": last.count,
                }),
                elements: force_full.then_some(text),
            };
        }

        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        let count = world.elements.len();
        self.exposed = Some(ExposedObservation {
            id,
            identity: world.identity.clone(),
            text: text.clone(),
            count,
        });
        PublishedObservation {
            metadata: json!({"id": id, "status": "full", "count": count}),
            elements: Some(text),
        }
    }
}

impl PublishedObservation {
    pub(super) fn attach(self, result: &mut Value) {
        result["observation"] = self.metadata;
        if let Some(elements) = self.elements {
            result["elements"] = Value::String(elements);
        } else {
            result["elements_unchanged"] = Value::Bool(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::computer_control::controller::world::{
        BrowserWindowIdentity, SurfaceIdentity,
    };

    fn world(document_id: &str, title: &str) -> WorldState {
        WorldState {
            elements: Vec::new(),
            url: Some("https://example.invalid/".to_string()),
            title: Some(title.to_string()),
            identity: SurfaceIdentity::Browser {
                tab_id: 7,
                document_id: document_id.to_string(),
                window: BrowserWindowIdentity {
                    browser_window_id: 1,
                    hwnd: 2,
                    pid: 3,
                    generation: 4,
                },
            },
        }
    }

    #[test]
    fn unchanged_observation_reuses_the_full_list_by_reference() {
        let mut cache = ObservationCache::default();
        let first = cache.publish(&world("doc", "page"), false);
        let mut first_value = json!({});
        first.attach(&mut first_value);
        let second = cache.publish(&world("doc", "page"), false);
        let mut second_value = json!({});
        second.attach(&mut second_value);

        assert_eq!(first_value["observation"]["status"], "full");
        assert!(first_value["elements"].is_string());
        assert_eq!(second_value["observation"]["status"], "unchanged");
        assert_eq!(
            second_value["observation"]["id"],
            first_value["observation"]["id"]
        );
        assert_eq!(second_value["elements_unchanged"], true);
        assert!(second_value.get("elements").is_none());
    }

    #[test]
    fn identity_or_content_change_publishes_a_new_full_observation() {
        let mut cache = ObservationCache::default();
        let mut first = json!({});
        cache
            .publish(&world("doc-a", "page"), false)
            .attach(&mut first);
        let mut changed = json!({});
        cache
            .publish(&world("doc-b", "other"), false)
            .attach(&mut changed);

        assert_eq!(changed["observation"]["status"], "full");
        assert_ne!(changed["observation"]["id"], first["observation"]["id"]);
        assert!(changed["elements"].is_string());
    }
}
