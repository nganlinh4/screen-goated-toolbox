use super::child::CARDS;
use super::protocol::HostCommand;

pub(super) fn name(command: &HostCommand) -> &'static str {
    match command {
        HostCommand::Snapshot { .. } => "snapshot",
        HostCommand::Upsert { .. } => "upsert",
        HostCommand::UpsertBatch { .. } => "upsert_batch",
        HostCommand::Stream { .. } => "stream",
        HostCommand::Finalize { .. } => "finalize",
        HostCommand::Geometry { .. } => "geometry",
        HostCommand::DragSettled { .. } => "drag_settled",
        HostCommand::Controls { .. } => "controls",
        HostCommand::Opacity { .. } => "opacity",
        HostCommand::RefineText { .. } => "refine_text",
        HostCommand::ExternalDrag { .. } => "external_drag",
        HostCommand::Theme { .. } => "theme",
        HostCommand::Raise { .. } => "raise",
        HostCommand::Remove { .. } => "remove",
        HostCommand::NavigateBack { .. } => "navigate_back",
        HostCommand::NavigateForward { .. } => "navigate_forward",
        HostCommand::Shutdown => "shutdown",
    }
}

pub(super) fn id(command: &HostCommand) -> Option<isize> {
    match command {
        HostCommand::Upsert { card } => Some(card.id),
        HostCommand::Stream { card } => Some(card.id),
        HostCommand::Finalize { card } => Some(card.id),
        HostCommand::Remove { id }
        | HostCommand::NavigateBack { id }
        | HostCommand::NavigateForward { id } => Some(*id),
        HostCommand::Raise { id, .. } | HostCommand::Opacity { id, .. } => Some(*id),
        HostCommand::RefineText { id, .. } => Some(*id),
        HostCommand::Snapshot { .. }
        | HostCommand::UpsertBatch { .. }
        | HostCommand::Geometry { .. }
        | HostCommand::DragSettled { .. }
        | HostCommand::Controls { .. }
        | HostCommand::ExternalDrag { .. }
        | HostCommand::Theme { .. }
        | HostCommand::Shutdown => None,
    }
}

pub(super) fn apply(command: &HostCommand) {
    let mut cards = CARDS.lock().unwrap();
    match command {
        HostCommand::Snapshot { cards: snapshot } => {
            cards.clear();
            cards.extend(snapshot.iter().cloned().map(|card| (card.id, card)));
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
        HostCommand::Geometry { cards: updates } | HostCommand::DragSettled { cards: updates } => {
            for update in updates {
                if let Some(card) = cards.get_mut(&update.id) {
                    card.rect = update.rect.clone();
                    card.control_rect = update.control_rect.clone();
                    card.visible = update.visible;
                }
            }
            if matches!(command, HostCommand::DragSettled { .. }) {
                super::button_input::settle_drag();
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
            cards.remove(id);
        }
        HostCommand::NavigateBack { .. }
        | HostCommand::NavigateForward { .. }
        | HostCommand::Opacity { .. }
        | HostCommand::RefineText { .. }
        | HostCommand::Shutdown => {}
    }
}
