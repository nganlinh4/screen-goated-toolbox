use super::delivery::send_command;
use super::parent::{DRAGGING, SCENES};
use super::protocol::{HostCommand, SceneControlUpdate, SceneControls};
use crate::overlay::result::state::{WINDOW_STATES, WindowState};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::HWND;

pub fn sync(hwnd: HWND) {
    let id = hwnd.0 as isize;
    let Some(controls) = snapshot(id) else {
        return;
    };
    let updated = SCENES.lock().unwrap().get_mut(&id).is_some_and(|card| {
        if card.controls == controls {
            return false;
        }
        card.controls.clone_from(&controls);
        true
    });
    if updated {
        send_command(HostCommand::Controls {
            cards: vec![SceneControlUpdate { id, controls }],
        });
    }
}

pub fn sync_all() {
    let ids: Vec<isize> = SCENES.lock().unwrap().keys().copied().collect();
    let snapshots = snapshots(ids);
    let mut scenes = SCENES.lock().unwrap();
    let cards = snapshots
        .into_iter()
        .filter_map(|(id, controls)| {
            let card = scenes.get_mut(&id)?;
            if card.controls == controls {
                return None;
            }
            card.controls.clone_from(&controls);
            Some(SceneControlUpdate { id, controls })
        })
        .collect::<Vec<_>>();
    drop(scenes);
    if !cards.is_empty() {
        send_command(HostCommand::Controls { cards });
    }
}

pub fn set_opacity(hwnd: HWND, value: u8) {
    let id = hwnd.0 as isize;
    let opacity = value.clamp(10, 100);
    {
        let mut states = WINDOW_STATES.lock().unwrap();
        let Some(state) = states.get_mut(&id) else {
            return;
        };
        state.opacity_percent = opacity;
    }
    let updated = SCENES.lock().unwrap().get_mut(&id).is_some_and(|card| {
        if card.opacity == opacity && card.controls.opacity_percent == opacity {
            return false;
        }
        card.opacity = opacity;
        card.controls.opacity_percent = opacity;
        true
    });
    if updated {
        send_command(HostCommand::Opacity { id, opacity });
    }
}

pub fn set_refine_text(hwnd: HWND, text: &str, is_insert: bool) {
    send_command(HostCommand::RefineText {
        id: hwnd.0 as isize,
        text: text.to_string(),
        is_insert,
    });
}

pub fn set_external_drag(active: bool) {
    DRAGGING.store(active, Ordering::SeqCst);
    send_command(HostCommand::ExternalDrag { active });
}

pub fn is_dragging() -> bool {
    DRAGGING.load(Ordering::SeqCst)
}

pub fn is_point_over_result_window(x: i32, y: i32) -> bool {
    let virtual_x = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_XVIRTUALSCREEN,
        )
    };
    let virtual_y = unsafe {
        windows::Win32::UI::WindowsAndMessaging::GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_YVIRTUALSCREEN,
        )
    };
    SCENES.lock().unwrap().values().any(|card| {
        let padding = 60;
        let left = virtual_x + card.control_rect.x;
        let top = virtual_y + card.control_rect.y;
        card.visible
            && x >= left - padding
            && x <= left + card.control_rect.width + padding
            && y >= top - padding
            && y <= top + card.control_rect.height + padding
    })
}

pub(super) fn snapshot(id: isize) -> Option<SceneControls> {
    let states = WINDOW_STATES.lock().unwrap();
    let state = states.get(&id)?;
    Some(from_state(id, state, &states))
}

pub(super) fn snapshots(ids: impl IntoIterator<Item = isize>) -> Vec<(isize, SceneControls)> {
    let states = WINDOW_STATES.lock().unwrap();
    ids.into_iter()
        .filter_map(|id| {
            states
                .get(&id)
                .map(|state| (id, from_state(id, state, &states)))
        })
        .collect()
}

fn from_state(
    id: isize,
    state: &WindowState,
    states: &HashMap<isize, WindowState>,
) -> SceneControls {
    SceneControls {
        copy_success: state.copy_success,
        has_undo: !state.text_history.is_empty(),
        has_redo: !state.redo_history.is_empty(),
        nav_depth: state.navigation_depth,
        max_nav_depth: state.max_navigation_depth,
        tts_loading: state.tts_loading,
        tts_speaking: state.tts_request_id != 0 && !state.tts_loading,
        is_browsing: state.is_browsing,
        is_editing: state.is_editing,
        input_text: state.input_text.clone(),
        opacity_percent: state.opacity_percent,
        group_ids: connected_ids(id, states),
    }
}

fn connected_ids(root: isize, states: &HashMap<isize, WindowState>) -> Vec<isize> {
    let mut result = Vec::new();
    let mut visited = HashSet::from([root]);
    let mut queue = VecDeque::from([root]);
    while let Some(id) = queue.pop_front() {
        if !states.contains_key(&id) {
            continue;
        }
        result.push(id);
        if let Some(state) = states.get(&id) {
            for linked in &state.linked_windows {
                let linked_id = linked.0 as isize;
                if states.contains_key(&linked_id) && visited.insert(linked_id) {
                    queue.push_back(linked_id);
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::connected_ids;
    use crate::overlay::result::state::{RefineContext, WindowState};
    use std::collections::HashMap;
    use windows::Win32::Foundation::HWND;

    fn state(linked_windows: Vec<HWND>) -> WindowState {
        WindowState {
            copy_success: false,
            is_editing: false,
            context_data: RefineContext::None,
            full_text: String::new(),
            text_history: Vec::new(),
            redo_history: Vec::new(),
            is_refining: false,
            is_streaming_active: false,
            model_id: String::new(),
            provider: String::new(),
            streaming_enabled: false,
            preset_prompt: String::new(),
            input_text: String::new(),
            bg_color: 0,
            linked_windows,
            cancellation_token: None,
            chain_id: None,
            latency_trace_id: None,
            is_browsing: false,
            navigation_depth: 0,
            max_navigation_depth: 0,
            tts_request_id: 0,
            tts_loading: false,
            opacity_percent: 100,
            preset_id: None,
            is_chain_root: false,
        }
    }

    #[test]
    fn control_groups_follow_the_complete_link_graph() {
        let hwnd = |id: isize| HWND(id as *mut std::ffi::c_void);
        let states = HashMap::from([
            (1, state(vec![hwnd(2)])),
            (2, state(vec![hwnd(1), hwnd(3)])),
            (3, state(vec![hwnd(2)])),
            (4, state(Vec::new())),
        ]);

        assert_eq!(connected_ids(1, &states), vec![1, 2, 3]);
        assert_eq!(connected_ids(4, &states), vec![4]);
    }
}
