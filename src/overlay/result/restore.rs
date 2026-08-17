use super::button_canvas;
use super::state::{ResultControlOptions, ResultPresentation, WINDOW_STATES, link_windows};
use super::{
    RefineContext, ResultWindowParams, WindowType, create_result_window,
    create_text_only_result_window,
};
use crate::win_types::SendHwnd;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{
    LazyLock, Mutex,
    atomic::{AtomicU64, Ordering},
};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, GetWindowRect, IsWindow, MSG, SW_SHOWNA, ShowWindow,
    TranslateMessage,
};

const MAX_RESTORE_HISTORY: usize = 5;
static NEXT_RESTORE_WINDOW_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
struct RestorableWindowSnapshot {
    restore_id: u64,
    rect: RECT,
    context: RefineContext,
    full_text: String,
    text_history: Vec<String>,
    redo_history: Vec<String>,
    model_id: String,
    provider: String,
    preset_prompt: String,
    bg_color: u32,
    opacity_percent: u8,
    preset_id: Option<String>,
    is_chain_root: bool,
    is_editing: bool,
    input_text: String,
    linked_restore_ids: Vec<u64>,
    presentation: ResultPresentation,
    control_options: Option<ResultControlOptions>,
    backdrop_data_url: Option<String>,
    foreground_color: Option<String>,
    preferred_font_size: Option<f32>,
    source_vertical: bool,
    source_regions: Vec<super::state::SourceReplacementRegion>,
    source_segments: Vec<String>,
}

#[derive(Clone)]
struct RestoreBatchSnapshot {
    windows: Vec<RestorableWindowSnapshot>,
}

static RECENT_CLOSED_SNAPSHOTS: LazyLock<Mutex<VecDeque<RestoreBatchSnapshot>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));

pub fn can_restore_last_closed() -> bool {
    !RECENT_CLOSED_SNAPSHOTS.lock().unwrap().is_empty()
}

pub fn recent_restore_option_counts() -> Vec<usize> {
    let history = RECENT_CLOSED_SNAPSHOTS.lock().unwrap();
    let mut cumulative = 0usize;
    let mut counts = Vec::with_capacity(history.len().min(MAX_RESTORE_HISTORY));

    for batch in history.iter().take(MAX_RESTORE_HISTORY) {
        if batch.windows.is_empty() {
            continue;
        }

        cumulative += logical_overlay_count(&batch.windows);
        counts.push(cumulative);
    }

    counts
}

fn logical_overlay_count(windows: &[RestorableWindowSnapshot]) -> usize {
    let restore_ids: Vec<u64> = windows.iter().map(|window| window.restore_id).collect();
    let links: Vec<(u64, u64)> = windows
        .iter()
        .flat_map(|window| {
            window
                .linked_restore_ids
                .iter()
                .map(move |linked| (window.restore_id, *linked))
        })
        .collect();
    connected_component_count(&restore_ids, &links)
}

fn connected_component_count(restore_ids: &[u64], links: &[(u64, u64)]) -> usize {
    let captured: HashSet<u64> = restore_ids.iter().copied().collect();
    let mut adjacency: HashMap<u64, Vec<u64>> = restore_ids
        .iter()
        .copied()
        .map(|restore_id| (restore_id, Vec::new()))
        .collect();

    for &(left, right) in links {
        if !captured.contains(&left) || !captured.contains(&right) {
            continue;
        }
        adjacency.entry(left).or_default().push(right);
        adjacency.entry(right).or_default().push(left);
    }

    let mut visited = HashSet::new();
    let mut count = 0usize;
    for &restore_id in restore_ids {
        if !visited.insert(restore_id) {
            continue;
        }

        count += 1;
        let mut pending = vec![restore_id];
        while let Some(current) = pending.pop() {
            if let Some(neighbors) = adjacency.get(&current) {
                for &neighbor in neighbors {
                    if visited.insert(neighbor) {
                        pending.push(neighbor);
                    }
                }
            }
        }
    }
    count
}

pub fn remember_last_closed(targets: &[HWND]) {
    let Some(snapshot) = capture_snapshot(targets) else {
        return;
    };

    let mut history = RECENT_CLOSED_SNAPSHOTS.lock().unwrap();
    history.push_front(snapshot);
    while history.len() > MAX_RESTORE_HISTORY {
        history.pop_back();
    }
}

pub fn restore_last_closed() -> bool {
    restore_recent(1)
}

pub fn restore_recent(batch_count: usize) -> bool {
    if batch_count == 0 {
        return false;
    }

    let selected_batches = {
        let mut history = RECENT_CLOSED_SNAPSHOTS.lock().unwrap();
        let take_count = batch_count.min(history.len());
        if take_count == 0 {
            return false;
        }

        let mut selected = Vec::with_capacity(take_count);
        for _ in 0..take_count {
            if let Some(batch) = history.pop_front() {
                selected.push(batch);
            }
        }
        selected
    };

    if restore_batches(&selected_batches) {
        return true;
    }

    let mut history = RECENT_CLOSED_SNAPSHOTS.lock().unwrap();
    for batch in selected_batches.into_iter().rev() {
        history.push_front(batch);
    }
    while history.len() > MAX_RESTORE_HISTORY {
        history.pop_back();
    }
    false
}

fn restore_batches(batches: &[RestoreBatchSnapshot]) -> bool {
    let mut restored = HashMap::new();

    for batch in batches.iter().rev() {
        for window in &batch.windows {
            if let Some(hwnd) = spawn_restored_window(window.clone()) {
                restored.insert(window.restore_id, hwnd);
            }
        }
    }

    for batch in batches.iter().rev() {
        for window in &batch.windows {
            let Some(&hwnd) = restored.get(&window.restore_id) else {
                continue;
            };

            for linked_restore_id in &window.linked_restore_ids {
                if let Some(&linked_hwnd) = restored.get(linked_restore_id) {
                    link_windows(hwnd, linked_hwnd);
                }
            }
        }
    }

    !restored.is_empty()
}

fn spawn_restored_window(window: RestorableWindowSnapshot) -> Option<HWND> {
    let (tx, rx) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        let coinit = unsafe { CoInitialize(None) };

        let params = ResultWindowParams {
            target_rect: window.rect,
            win_type: WindowType::Primary,
            context: window.context.clone(),
            model_id: window.model_id.clone(),
            provider: window.provider.clone(),
            streaming_enabled: false,
            start_editing: window.is_editing,
            preset_prompt: window.preset_prompt.clone(),
            custom_bg_color: window.bg_color,
            initial_text: window.full_text.clone(),
            preset_id: window.preset_id.clone(),
            is_chain_root: window.is_chain_root,
            latency_trace_id: None,
        };
        let hwnd = match window.presentation {
            ResultPresentation::Standard => create_result_window(params),
            ResultPresentation::TextOnly => create_text_only_result_window(
                params,
                window.backdrop_data_url.clone().unwrap_or_default(),
                window.foreground_color.clone().unwrap_or_default(),
                format!("restored-overlay-{}", window.restore_id),
                window.control_options.clone(),
                window.preferred_font_size,
                window.source_vertical,
                window.source_regions.clone(),
                window.source_segments.clone(),
            ),
        };

        if hwnd.is_invalid() {
            let _ = tx.send(None);
            if coinit.is_ok() {
                unsafe {
                    CoUninitialize();
                }
            }
            return;
        }

        {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                state.full_text = window.full_text.clone();
                state.text_history = window.text_history.clone();
                state.redo_history = window.redo_history.clone();
                state.input_text = window.input_text.clone();
                state.is_editing = window.is_editing;
                state.is_refining = false;
                state.is_streaming_active = false;
                state.bg_color = window.bg_color;
                state.linked_windows.clear();
                state.is_browsing = false;
                state.navigation_depth = 0;
                state.max_navigation_depth = 0;
                state.tts_request_id = 0;
                state.tts_loading = false;
                state.opacity_percent = window.opacity_percent;
                state.cancellation_token = None;
                state.chain_id = None;
            }
        }

        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNA);
        }
        button_canvas::update_window_position(hwnd);

        let _ = tx.send(Some(SendHwnd(hwnd)));

        unsafe {
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).into() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
                if !IsWindow(Some(hwnd)).as_bool() {
                    break;
                }
            }

            if coinit.is_ok() {
                CoUninitialize();
            }
        }
    });

    rx.recv().ok().flatten().map(|hwnd| hwnd.0)
}

fn capture_snapshot(targets: &[HWND]) -> Option<RestoreBatchSnapshot> {
    if targets.is_empty() {
        return None;
    }

    let mut seen = HashSet::new();
    let target_hwnds: Vec<HWND> = targets
        .iter()
        .copied()
        .filter(|hwnd| seen.insert(hwnd.0 as isize))
        .collect();
    let target_set: HashSet<isize> = target_hwnds.iter().map(|hwnd| hwnd.0 as isize).collect();
    let restore_ids: HashMap<isize, u64> = target_hwnds
        .iter()
        .map(|hwnd| {
            (
                hwnd.0 as isize,
                NEXT_RESTORE_WINDOW_ID.fetch_add(1, Ordering::Relaxed),
            )
        })
        .collect();

    let states = WINDOW_STATES.lock().unwrap();
    let mut windows = Vec::new();

    for hwnd in target_hwnds {
        let hwnd_key = hwnd.0 as isize;
        let Some(state) = states.get(&hwnd_key) else {
            continue;
        };

        let mut rect = RECT::default();
        unsafe {
            let _ = GetWindowRect(hwnd, &mut rect);
        }

        windows.push(RestorableWindowSnapshot {
            restore_id: *restore_ids
                .get(&hwnd_key)
                .expect("restore ID must exist for captured hwnd"),
            rect,
            context: state.context_data.clone(),
            full_text: state.full_text.clone(),
            text_history: state.text_history.clone(),
            redo_history: state.redo_history.clone(),
            model_id: state.model_id.clone(),
            provider: state.provider.clone(),
            preset_prompt: state.preset_prompt.clone(),
            bg_color: state.bg_color,
            opacity_percent: state.opacity_percent,
            preset_id: state.preset_id.clone(),
            is_chain_root: state.is_chain_root,
            is_editing: state.is_editing,
            input_text: state.input_text.clone(),
            linked_restore_ids: state
                .linked_windows
                .iter()
                .map(|linked| linked.0 as isize)
                .filter(|linked| target_set.contains(linked))
                .filter_map(|linked| restore_ids.get(&linked).copied())
                .collect(),
            presentation: state.presentation,
            control_options: state.control_options.clone(),
            backdrop_data_url: state.backdrop_data_url.clone(),
            foreground_color: state.foreground_color.clone(),
            preferred_font_size: state.preferred_font_size,
            source_vertical: state.source_vertical,
            source_regions: state.source_regions.clone(),
            source_segments: state.source_segments.clone(),
        });
    }

    if windows.is_empty() {
        None
    } else {
        Some(RestoreBatchSnapshot { windows })
    }
}

#[cfg(test)]
mod tests {
    use super::connected_component_count;

    #[test]
    fn linked_result_windows_count_as_one_logical_overlay() {
        let restore_ids = [1, 2, 3, 4];
        let links = [(1, 2), (2, 3)];

        assert_eq!(connected_component_count(&restore_ids, &links), 2);
        assert_eq!(connected_component_count(&restore_ids[..3], &links), 1);
        assert_eq!(connected_component_count(&[], &[]), 0);
    }
}
