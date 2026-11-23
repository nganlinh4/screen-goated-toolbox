use super::*;

pub(crate) fn register_retirement_wake(
    handle: SourceGroupHandle,
    wake: std::sync::Arc<crate::overlay::wake_event::WakeEvent>,
) {
    let mut groups = GROUPS.lock().unwrap();
    if let Some(group) = groups.get_mut(&handle.root_id) {
        group.retirement_wake = Some(wake);
    } else {
        // The group may retire between creation and listener registration.
        wake.signal();
    }
}

pub(super) fn remove_group(root_id: isize) {
    super::super::source_image::cancel(root_id);
    let Some(group) = GROUPS.lock().unwrap().remove(&root_id) else {
        return;
    };
    if let Some(wake) = group.retirement_wake {
        wake.signal();
    }
    let mut ids = group.card_order;
    ids.push(root_id);
    crate::overlay::result::latency::unbind_logical_scene_ids(ids.iter().copied());
    {
        let mut roots = CARD_GROUPS.lock().unwrap();
        for id in &ids {
            roots.remove(id);
        }
    }
    let removed = {
        let mut scenes = SCENES.lock().unwrap();
        ids.into_iter()
            .filter(|id| scenes.remove(id).is_some())
            .collect::<Vec<_>>()
    };
    for id in removed {
        crate::log_trace!("[ResultCard] id={id} host=remove");
        send_command(HostCommand::Remove { id });
    }
}
