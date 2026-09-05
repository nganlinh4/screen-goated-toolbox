use super::child::CARDS;
use super::protocol::HostCommand;

pub(super) fn apply(command: &HostCommand) {
    let mut cards = CARDS.lock().unwrap();
    match command {
        HostCommand::Snapshot { cards: snapshot } => {
            cards.clear();
            cards.extend(snapshot.iter().cloned().map(|card| (card.id, card)));
            super::button_input::cancel_missing_cards(&cards);
        }
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
            if super::button_input::settle_drag(*gesture_id) {
                for update in updates {
                    if let Some(card) = cards.get_mut(&update.id) {
                        card.rect = update.rect.clone();
                        card.control_rect = update.control_rect.clone();
                        card.visible = update.visible;
                    }
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
        HostCommand::ExternalDrag { active } => super::button_input::set_external_drag(*active),
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
        HostCommand::Remove { id } => {
            super::button_input::cancel_removed_card(*id);
            cards.remove(id);
        }
        HostCommand::NavigateBack { .. }
        | HostCommand::NavigateForward { .. }
        | HostCommand::Opacity { .. }
        | HostCommand::RefineText { .. }
        | HostCommand::ApplyRevision { .. }
        | HostCommand::Shutdown => {}
    }
}
