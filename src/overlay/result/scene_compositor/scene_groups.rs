use super::delivery::send_command;
use super::diagnostics::log_host_command;
use super::parent::{SCENES, next_stack_order};
use super::protocol::{HostCommand, SceneCard, SceneControls, SceneGeometry, SceneRect};
use crate::overlay::result::markdown_view::conversion::render_for_compositor;
use crate::overlay::result::state::WINDOW_STATES;
use crate::overlay::result::{ResultPresentation, SourceReplacementRegion};
use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, IsWindow, SM_CXVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

static NEXT_SCENE_ID: AtomicIsize = AtomicIsize::new(-1);
static GROUPS: LazyLock<Mutex<HashMap<isize, SourceGroupState>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static CARD_GROUPS: LazyLock<Mutex<HashMap<isize, isize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
pub struct SourceCardSpec {
    pub target_rect: RECT,
    pub backdrop_data_url: String,
    pub foreground_color: String,
    pub preferred_font_size: f32,
    pub source_vertical: bool,
    pub source_regions: Vec<SourceReplacementRegion>,
}

#[derive(Clone, Copy)]
pub struct SourceCardHandle {
    root_id: isize,
    id: isize,
}

#[derive(Clone, Copy)]
pub struct SourceGroupHandle {
    root_id: isize,
}

struct SourceCardState {
    spec: SourceCardSpec,
    text: String,
    segments: Vec<String>,
    visible: bool,
    stack_order: u64,
}

struct SourceGroupState {
    controller_id: isize,
    controller_stack_order: u64,
    card_order: Vec<isize>,
    cards: HashMap<isize, SourceCardState>,
    offset: (i32, i32),
    opacity: u8,
    controls_visible: bool,
}

pub fn prewarm_source_group(
    controller: HWND,
    specs: Vec<SourceCardSpec>,
    opacity: u8,
    trace_id: &str,
) -> (SourceGroupHandle, Vec<SourceCardHandle>) {
    let root_id = controller.0 as isize;
    let mut cards = HashMap::with_capacity(specs.len());
    let mut card_order = Vec::with_capacity(specs.len());
    let mut handles = Vec::with_capacity(specs.len());
    for spec in specs {
        let id = NEXT_SCENE_ID.fetch_sub(1, Ordering::SeqCst);
        card_order.push(id);
        handles.push(SourceCardHandle { root_id, id });
        cards.insert(
            id,
            SourceCardState {
                spec,
                text: String::new(),
                segments: Vec::new(),
                visible: false,
                stack_order: next_stack_order(),
            },
        );
    }
    {
        let mut roots = CARD_GROUPS.lock().unwrap();
        roots.insert(root_id, root_id);
        for id in &card_order {
            roots.insert(*id, root_id);
        }
    }
    crate::overlay::result::latency::bind_logical_scene_ids(card_order.iter().copied(), trace_id);
    GROUPS.lock().unwrap().insert(
        root_id,
        SourceGroupState {
            controller_id: root_id,
            controller_stack_order: next_stack_order(),
            card_order,
            cards,
            offset: (0, 0),
            opacity,
            controls_visible: false,
        },
    );
    let mut scene_cards = source_group_cards(root_id);
    if let Some(controller_card) = controller_card(root_id, false) {
        scene_cards.push(controller_card);
    }
    SCENES
        .lock()
        .unwrap()
        .extend(scene_cards.iter().cloned().map(|card| (card.id, card)));
    let command = HostCommand::UpsertBatch { cards: scene_cards };
    log_host_command(&command, 0);
    send_command(command);
    (SourceGroupHandle { root_id }, handles)
}

pub fn reveal_source_card(handle: SourceCardHandle, segments: Vec<String>, trace_id: &str) -> bool {
    let text = segments.join("\n");
    let (card, first_visible, combined_text) = {
        let mut groups = GROUPS.lock().unwrap();
        let Some(group) = groups.get_mut(&handle.root_id) else {
            return false;
        };
        let first_visible = !group.controls_visible;
        group.controls_visible = true;
        let Some(state) = group.cards.get_mut(&handle.id) else {
            return false;
        };
        state.text.clone_from(&text);
        state.segments = segments;
        state.visible = true;
        let card = source_card(handle.id, state, group.offset, group.opacity);
        let combined_text = group
            .card_order
            .iter()
            .filter_map(|id| group.cards.get(id))
            .map(|card| card.text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\r\n");
        (card, first_visible, combined_text)
    };
    if let Some(state) = WINDOW_STATES.lock().unwrap().get_mut(&handle.root_id) {
        state.full_text = combined_text;
    }
    SCENES.lock().unwrap().insert(card.id, card.clone());
    let command = HostCommand::Upsert { card };
    log_host_command(&command, text.chars().count());
    send_command(command);
    crate::overlay::result::latency::mark(trace_id, "compositor_command_queued");
    if first_visible && let Some(card) = controller_card(handle.root_id, true) {
        SCENES.lock().unwrap().insert(card.id, card.clone());
        send_command(HostCommand::Upsert { card });
    }
    first_visible
}

pub fn is_group_alive(handle: SourceGroupHandle) -> bool {
    let controller_id = GROUPS
        .lock()
        .unwrap()
        .get(&handle.root_id)
        .map(|group| group.controller_id);
    controller_id
        .is_some_and(|id| unsafe { IsWindow(Some(HWND(id as *mut std::ffi::c_void))).as_bool() })
}

pub fn remove_source_group(handle: SourceGroupHandle) {
    remove_group(handle.root_id);
}

pub fn group_ids(id: isize) -> Option<Vec<isize>> {
    let root_id = CARD_GROUPS.lock().unwrap().get(&id).copied()?;
    let groups = GROUPS.lock().unwrap();
    let group = groups.get(&root_id)?;
    let mut ids = Vec::with_capacity(group.card_order.len() + 1);
    ids.push(root_id);
    ids.extend(group.card_order.iter().copied());
    Some(ids)
}

pub fn set_group_opacity(id: isize, opacity: u8) -> bool {
    let opacity = crate::config::types::normalize_result_overlay_opacity_percent(opacity);
    let Some(root_id) = CARD_GROUPS.lock().unwrap().get(&id).copied() else {
        return false;
    };
    let ids = {
        let mut groups = GROUPS.lock().unwrap();
        let Some(group) = groups.get_mut(&root_id) else {
            return false;
        };
        group.opacity = opacity;
        let mut ids = group.card_order.clone();
        ids.push(root_id);
        ids
    };
    if let Some(state) = WINDOW_STATES.lock().unwrap().get_mut(&root_id) {
        state.opacity_percent = opacity;
    }
    let mut scenes = SCENES.lock().unwrap();
    for id in &ids {
        if let Some(card) = scenes.get_mut(id) {
            card.opacity = opacity;
            card.controls.opacity_percent = opacity;
        }
    }
    drop(scenes);
    for id in ids {
        send_command(HostCommand::Opacity { id, opacity });
    }
    true
}

pub fn move_group(id: isize, dx: i32, dy: i32) -> Option<Vec<SceneGeometry>> {
    let root_id = CARD_GROUPS.lock().unwrap().get(&id).copied()?;
    let ids = {
        let mut groups = GROUPS.lock().unwrap();
        let group = groups.get_mut(&root_id)?;
        group.offset.0 = group.offset.0.saturating_add(dx);
        group.offset.1 = group.offset.1.saturating_add(dy);
        let mut ids = group.card_order.clone();
        ids.push(root_id);
        ids
    };
    if let Some(state) = WINDOW_STATES.lock().unwrap().get_mut(&root_id)
        && let Some(options) = state.control_options.as_mut()
    {
        options.shift_anchor(dx, dy);
    }
    let controls = super::controls::snapshot(root_id);
    let mut scenes = SCENES.lock().unwrap();
    let cards = ids
        .into_iter()
        .filter_map(|id| {
            let card = scenes.get_mut(&id)?;
            card.rect.x = card.rect.x.saturating_add(dx);
            card.rect.y = card.rect.y.saturating_add(dy);
            card.control_rect.x = card.control_rect.x.saturating_add(dx);
            card.control_rect.y = card.control_rect.y.saturating_add(dy);
            if id == root_id
                && let Some(controls) = &controls
            {
                card.controls.clone_from(controls);
            }
            Some(SceneGeometry {
                id,
                rect: card.rect.clone(),
                control_rect: card.control_rect.clone(),
                visible: card.visible,
            })
        })
        .collect();
    Some(cards)
}

pub fn raise_group(id: isize) -> bool {
    let Some(root_id) = CARD_GROUPS.lock().unwrap().get(&id).copied() else {
        return false;
    };
    let ids = {
        let groups = GROUPS.lock().unwrap();
        let Some(group) = groups.get(&root_id) else {
            return false;
        };
        let mut ids = group.card_order.clone();
        ids.push(root_id);
        ids
    };
    let updates = {
        let mut scenes = SCENES.lock().unwrap();
        ids.into_iter()
            .filter_map(|id| {
                let stack_order = next_stack_order();
                let card = scenes.get_mut(&id)?;
                card.stack_order = stack_order;
                Some((id, stack_order))
            })
            .collect::<Vec<_>>()
    };
    for (id, stack_order) in updates {
        send_command(HostCommand::Raise { id, stack_order });
    }
    true
}

pub fn resize_card(id: isize, rect: SceneRect) -> Option<SceneGeometry> {
    let root_id = CARD_GROUPS.lock().unwrap().get(&id).copied()?;
    let (scene_x, virtual_y) = scene_origin();
    {
        let mut groups = GROUPS.lock().unwrap();
        let group = groups.get_mut(&root_id)?;
        let card = group.cards.get_mut(&id)?;
        let left = scene_x
            .saturating_add(rect.x)
            .saturating_sub(group.offset.0);
        let top = virtual_y
            .saturating_add(rect.y)
            .saturating_sub(group.offset.1);
        card.spec.target_rect = RECT {
            left,
            top,
            right: left.saturating_add(rect.width.max(1)),
            bottom: top.saturating_add(rect.height.max(1)),
        };
    }
    let mut scenes = SCENES.lock().unwrap();
    let card = scenes.get_mut(&id)?;
    card.rect = rect.clone();
    card.control_rect = rect.clone();
    Some(SceneGeometry {
        id,
        rect: rect.clone(),
        control_rect: rect,
        visible: card.visible,
    })
}

fn remove_group(root_id: isize) {
    let Some(group) = GROUPS.lock().unwrap().remove(&root_id) else {
        return;
    };
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
        crate::log_info!("[ResultCard] id={id} host=remove");
        send_command(HostCommand::Remove { id });
    }
}

fn source_group_cards(root_id: isize) -> Vec<SceneCard> {
    let groups = GROUPS.lock().unwrap();
    let Some(group) = groups.get(&root_id) else {
        return Vec::new();
    };
    group
        .card_order
        .iter()
        .filter_map(|id| {
            group
                .cards
                .get(id)
                .map(|card| source_card(*id, card, group.offset, group.opacity))
        })
        .collect()
}

fn source_card(id: isize, state: &SourceCardState, offset: (i32, i32), opacity: u8) -> SceneCard {
    let text = state.segments.join("\n");
    let rendered = render_for_compositor(&text, false, "", "");
    let mut rect = scene_rect(state.spec.target_rect);
    rect.x = rect.x.saturating_add(offset.0);
    rect.y = rect.y.saturating_add(offset.1);
    SceneCard {
        id,
        rect: rect.clone(),
        control_rect: rect,
        body: rendered.body,
        document: rendered.isolated_document,
        external_navigation: false,
        navigation_loading: false,
        refining: false,
        background: "transparent".to_string(),
        opacity,
        visible: state.visible,
        streaming: false,
        streaming_enabled: false,
        stack_order: state.stack_order,
        controls: SceneControls {
            hidden: true,
            ..SceneControls::default()
        },
        presentation: ResultPresentation::TextOnly,
        backdrop_data_url: Some(state.spec.backdrop_data_url.clone()),
        foreground_color: Some(state.spec.foreground_color.clone()),
        preferred_font_size: Some(state.spec.preferred_font_size),
        source_replacement: true,
        source_vertical: state.spec.source_vertical,
        source_regions: state.spec.source_regions.clone(),
        source_segments: state.segments.clone(),
    }
}

fn controller_card(root_id: isize, visible: bool) -> Option<SceneCard> {
    let (target_rect, offset, opacity, stack_order) = {
        let groups = GROUPS.lock().unwrap();
        let group = groups.get(&root_id)?;
        let first = group.cards.get(group.card_order.first()?)?;
        (
            first.spec.target_rect,
            group.offset,
            group.opacity,
            group.controller_stack_order,
        )
    };
    let mut rect = scene_rect(target_rect);
    rect.x = rect.x.saturating_add(offset.0);
    rect.y = rect.y.saturating_add(offset.1);
    rect.width = 1;
    rect.height = 1;
    Some(SceneCard {
        id: root_id,
        rect: rect.clone(),
        control_rect: rect,
        body: String::new(),
        document: None,
        external_navigation: false,
        navigation_loading: false,
        refining: false,
        background: "transparent".to_string(),
        opacity,
        visible,
        streaming: false,
        streaming_enabled: false,
        stack_order,
        controls: super::controls::snapshot(root_id).unwrap_or_default(),
        presentation: ResultPresentation::TextOnly,
        backdrop_data_url: None,
        foreground_color: None,
        preferred_font_size: None,
        source_replacement: false,
        source_vertical: false,
        source_regions: Vec::new(),
        source_segments: Vec::new(),
    })
}

fn scene_rect(rect: RECT) -> SceneRect {
    let (scene_x, virtual_y) = scene_origin();
    SceneRect {
        x: rect.left.saturating_sub(scene_x),
        y: rect.top.saturating_sub(virtual_y),
        width: rect.right.saturating_sub(rect.left).max(1),
        height: rect.bottom.saturating_sub(rect.top).max(1),
    }
}

fn scene_origin() -> (i32, i32) {
    let virtual_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    (
        super::compositor_host_x(
            virtual_x,
            unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) }.max(1),
        ),
        unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) },
    )
}

#[cfg(test)]
mod tests {
    use super::{NEXT_SCENE_ID, SourceCardSpec, SourceCardState, source_card};
    use std::sync::atomic::Ordering;
    use windows::Win32::Foundation::RECT;

    #[test]
    fn logical_scene_ids_never_overlap_native_window_handles() {
        let id = NEXT_SCENE_ID.fetch_sub(1, Ordering::SeqCst);
        assert!(id < 0);
    }

    #[test]
    fn logical_translation_cells_never_own_result_controls() {
        let card = source_card(
            -42,
            &SourceCardState {
                spec: SourceCardSpec {
                    target_rect: RECT {
                        left: 10,
                        top: 20,
                        right: 110,
                        bottom: 70,
                    },
                    backdrop_data_url: String::new(),
                    foreground_color: "#ffffff".to_string(),
                    preferred_font_size: 16.0,
                    source_vertical: false,
                    source_regions: Vec::new(),
                },
                text: "translated".to_string(),
                segments: vec!["translated".to_string()],
                visible: true,
                stack_order: 1,
            },
            (0, 0),
            100,
        );

        assert!(card.controls.hidden);
        assert!(card.controls.group_ids.is_empty());
    }
}
