use super::child::CARDS;
use super::protocol::{HostCommand, SceneCard};
use std::collections::HashMap;
use std::sync::Mutex;

pub(super) fn apply(command: &HostCommand) -> bool {
    apply_to(
        command,
        &CARDS,
        super::button_input::cancel_removed_card,
        super::button_input::cancel_missing_cards,
    )
}

fn apply_to(
    command: &HostCommand,
    state: &Mutex<HashMap<isize, SceneCard>>,
    cancel_removed: impl FnOnce(isize),
    cancel_missing: impl FnOnce(&HashMap<isize, SceneCard>),
) -> bool {
    if let HostCommand::Remove { id } = command {
        // Cancellation can call native APIs that dispatch nested UI messages.
        // Preserve its original ordering before the card is removed.
        cancel_removed(*id);
        state.lock().unwrap().remove(id);
        return true;
    }
    if let HostCommand::Snapshot { cards } = command {
        let snapshot: HashMap<_, _> = cards.iter().cloned().map(|card| (card.id, card)).collect();
        *state.lock().unwrap() = snapshot.clone();
        cancel_missing(&snapshot);
        return true;
    }
    let mut cards = state.lock().unwrap();
    match command {
        HostCommand::CaptureExclusion { .. } => {}
        HostCommand::Processing { effects } => super::processing::apply(effects),
        HostCommand::Snapshot { .. } | HostCommand::Remove { .. } => unreachable!(),
        HostCommand::Upsert { card } => {
            cards.insert(card.id, card.clone());
        }
        HostCommand::UpsertBatch { cards: updates } => {
            cards.extend(updates.iter().cloned().map(|card| (card.id, card)));
        }
        HostCommand::Stream { card: update } => {
            if let Some(card) = cards.get_mut(&update.id) {
                card.body.clone_from(&update.body);
                card.document.clone_from(&update.document);
                card.refining = update.refining;
                card.navigation_loading = update.navigation_loading;
                card.background.clone_from(&update.background);
                card.opacity = update.opacity;
                card.visible = update.visible;
                card.streaming = true;
                card.controls.clone_from(&update.controls);
            }
        }
        HostCommand::Finalize { card: update } => {
            if let Some(card) = cards.get_mut(&update.id) {
                card.body.clone_from(&update.body);
                card.document.clone_from(&update.document);
                card.refining = update.refining;
                card.navigation_loading = update.navigation_loading;
                card.background.clone_from(&update.background);
                card.opacity = update.opacity;
                card.visible = update.visible;
                card.streaming = false;
                card.controls.clone_from(&update.controls);
            }
        }
        HostCommand::Geometry { cards: updates } => {
            for update in updates {
                if let Some(card) = cards.get_mut(&update.id) {
                    card.rect = update.rect.clone();
                    card.control_rect = update.control_rect.clone();
                    card.visible = update.visible;
                }
            }
        }
        HostCommand::DragSettled {
            gesture_id,
            cards: updates,
        } => {
            if !super::button_input::settle_drag(*gesture_id) {
                return false;
            }
            for update in updates {
                if let Some(card) = cards.get_mut(&update.id) {
                    card.rect = update.rect.clone();
                    card.control_rect = update.control_rect.clone();
                    card.visible = update.visible;
                }
            }
        }
        HostCommand::Controls { cards: updates } => {
            for update in updates {
                if let Some(card) = cards.get_mut(&update.id) {
                    card.controls.clone_from(&update.controls);
                }
            }
        }
        HostCommand::Theme { theme } => {
            for appearance in &theme.cards {
                if let Some(card) = cards.get_mut(&appearance.id) {
                    card.background.clone_from(&appearance.background);
                }
            }
        }
        HostCommand::Raise { id, stack_order } => {
            if let Some(card) = cards.get_mut(id) {
                card.stack_order = *stack_order;
            }
        }
        HostCommand::SourceImage { .. }
        | HostCommand::SourceImageStatus { .. }
        | HostCommand::ExternalDrag { .. }
        | HostCommand::Opacity { .. }
        | HostCommand::RefineText { .. }
        | HostCommand::Locale { .. }
        | HostCommand::ApplyRevision { .. }
        | HostCommand::Shutdown => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: isize) -> SceneCard {
        serde_json::from_value(serde_json::json!({
            "id": id, "rect": {"x": 0, "y": 0, "width": 10, "height": 10},
            "body": "result", "document": null, "refining": false,
            "background": "#fff", "opacity": 100, "visible": true
        }))
        .unwrap()
    }

    #[test]
    fn removed_card_cancellation_runs_unlocked_before_removal() {
        let state = Mutex::new(HashMap::from([(7, card(7))]));
        assert!(apply_to(
            &HostCommand::Remove { id: 7 },
            &state,
            |id| {
                let during = state
                    .try_lock()
                    .expect("cancellation must not hold the card lock");
                assert!(during.contains_key(&id));
            },
            |_| panic!("unexpected snapshot callback")
        ));
        assert!(!state.lock().unwrap().contains_key(&7));
    }

    #[test]
    fn snapshot_cancellation_runs_unlocked_after_snapshot_is_installed() {
        let state = Mutex::new(HashMap::from([(7, card(7))]));
        assert!(apply_to(
            &HostCommand::Snapshot {
                cards: vec![card(8)]
            },
            &state,
            |_| panic!("unexpected removal callback"),
            |snapshot| {
                let during = state
                    .try_lock()
                    .expect("cancellation must not hold the card lock");
                assert_eq!(
                    during.keys().collect::<Vec<_>>(),
                    snapshot.keys().collect::<Vec<_>>()
                );
                assert!(during.contains_key(&8));
                assert!(!during.contains_key(&7));
            }
        ));
    }
}
