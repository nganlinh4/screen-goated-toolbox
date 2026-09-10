//! Coalesced geometry is resolved from the current scene at dispatch time.
use super::parent::{SCENE_DISPATCH, SCENES};
use super::protocol::{HostCommand, SceneCard, SceneGeometry};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{LazyLock, Mutex};

static PENDING: LazyLock<Mutex<HashSet<isize>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static SIGNAL: LazyLock<SyncSender<()>> = LazyLock::new(|| {
    let (sender, receiver) = sync_channel(1);
    std::thread::spawn(move || {
        while receiver.recv().is_ok() {
            while receiver.try_recv().is_ok() {}
            // Snapshot and enqueue under the same lock used by visibility updates
            // and removal. An older position must not restore a hidden input region.
            let _dispatch = SCENE_DISPATCH.lock().unwrap();
            let cards = drain_current(&mut PENDING.lock().unwrap(), &SCENES.lock().unwrap());
            if !cards.is_empty() {
                super::delivery::send_command(HostCommand::Geometry { cards });
            }
        }
    });
    sender
});

pub(super) fn queue(id: isize) {
    PENDING.lock().unwrap().insert(id);
    let _ = SIGNAL.try_send(());
}

pub(super) fn remove(id: isize) {
    PENDING.lock().unwrap().remove(&id);
}

fn drain_current(
    pending: &mut HashSet<isize>,
    scenes: &HashMap<isize, SceneCard>,
) -> Vec<SceneGeometry> {
    pending
        .drain()
        .filter_map(|id| {
            let card = scenes.get(&id)?;
            Some(SceneGeometry {
                id,
                rect: card.rect.clone(),
                control_rect: card.control_rect.clone(),
                visible: card.visible,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> SceneCard {
        serde_json::from_value(serde_json::json!({
            "id": 7,
            "rect": { "x": 100, "y": 200, "width": 500, "height": 250 },
            "control_rect": { "x": 96, "y": 198, "width": 508, "height": 254 },
            "body": "result", "document": null, "refining": false,
            "background": "#202020", "opacity": 100, "visible": true
        }))
        .unwrap()
    }

    #[test]
    fn queued_visible_geometry_cannot_reverse_a_later_hide() {
        let mut scenes = HashMap::from([(7, card())]);
        let mut pending = HashSet::from([7]);
        scenes.get_mut(&7).unwrap().visible = false;
        let batch = drain_current(&mut pending, &scenes);
        assert_eq!(batch.len(), 1);
        assert!(!batch[0].visible);
        assert!(pending.is_empty());
    }

    #[test]
    fn queued_geometry_uses_latest_bounds_and_visibility() {
        let mut current = card();
        current.visible = false;
        let mut scenes = HashMap::from([(7, current)]);
        let mut pending = HashSet::from([7]);
        let current = scenes.get_mut(&7).unwrap();
        current.rect.x = 2_200;
        current.control_rect.x = 2_196;
        current.visible = true;
        let batch = drain_current(&mut pending, &scenes);
        assert_eq!(batch[0].rect.x, 2_200);
        assert_eq!(batch[0].control_rect.x, 2_196);
        assert!(batch[0].visible);
    }

    #[test]
    fn removed_scenes_are_not_recreated_by_pending_geometry() {
        let mut scenes = HashMap::from([(7, card())]);
        let mut pending = HashSet::from([7]);
        scenes.remove(&7);
        assert!(drain_current(&mut pending, &scenes).is_empty());
        assert!(pending.is_empty());
    }
}
