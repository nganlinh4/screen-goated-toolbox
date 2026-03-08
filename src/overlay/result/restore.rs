use super::button_canvas;
use super::state::{WINDOW_STATES, link_windows};
use super::{RefineContext, ResultWindowParams, WindowType, create_result_window};
use crate::win_types::SendHwnd;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, GetWindowRect, IsWindow, LWA_ALPHA, MSG, SW_SHOWNA,
    SetLayeredWindowAttributes, ShowWindow, TranslateMessage,
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
    is_markdown_mode: bool,
    is_markdown_streaming: bool,
    is_editing: bool,
    input_text: String,
    linked_restore_ids: Vec<u64>,
}

#[derive(Clone)]
struct RestoreBatchSnapshot {
    windows: Vec<RestorableWindowSnapshot>,
}

lazy_static::lazy_static! {
    static ref RECENT_CLOSED_SNAPSHOTS: Mutex<VecDeque<RestoreBatchSnapshot>> =
        Mutex::new(VecDeque::new());
}

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

        cumulative += batch.windows.len();
        counts.push(cumulative);
    }

    counts
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

        let render_mode = if window.is_markdown_mode {
            "markdown"
        } else {
            "text"
        };

        let hwnd = create_result_window(ResultWindowParams {
            target_rect: window.rect,
            win_type: WindowType::Primary,
            context: window.context.clone(),
            model_id: window.model_id.clone(),
            provider: window.provider.clone(),
            streaming_enabled: false,
            start_editing: window.is_editing,
            preset_prompt: window.preset_prompt.clone(),
            custom_bg_color: window.bg_color,
            render_mode,
            initial_text: window.full_text.clone(),
            preset_id: window.preset_id.clone(),
            is_chain_root: window.is_chain_root,
        });

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
                state.pending_text = Some(window.full_text.clone());
                state.text_history = window.text_history.clone();
                state.redo_history = window.redo_history.clone();
                state.input_text = window.input_text.clone();
                state.is_editing = window.is_editing;
                state.is_refining = false;
                state.is_streaming_active = false;
                state.was_streaming_active = false;
                state.bg_color = window.bg_color;
                state.linked_windows.clear();
                state.is_markdown_mode = window.is_markdown_mode;
                state.is_markdown_streaming =
                    window.is_markdown_mode && window.is_markdown_streaming;
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

        let alpha = ((window.opacity_percent as f32 / 100.0) * 255.0).round() as u8;
        unsafe {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
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
            is_markdown_mode: state.is_markdown_mode,
            is_markdown_streaming: state.is_markdown_streaming,
            is_editing: state.is_editing,
            input_text: state.input_text.clone(),
            linked_restore_ids: state
                .linked_windows
                .iter()
                .map(|linked| linked.0 as isize)
                .filter(|linked| target_set.contains(linked))
                .filter_map(|linked| restore_ids.get(&linked).copied())
                .collect(),
        });
    }

    if windows.is_empty() {
        None
    } else {
        Some(RestoreBatchSnapshot { windows })
    }
}
