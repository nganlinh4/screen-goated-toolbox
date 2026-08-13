use super::{
    RefineContext, ResultWindowParams, WINDOW_STATES, WindowType, create_result_window,
    update_window_text,
};
use crate::win_types::SendHwnd;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, IsWindow, MSG, PostMessageW, SW_SHOWNA, ShowWindow,
    TranslateMessage, WM_CLOSE,
};

const CARD_COUNT: usize = 6;
const NON_STREAMING_CARD_INDEX: usize = 4;

pub(crate) fn run() -> i32 {
    if !super::scene_compositor::wait_until_ready(Duration::from_secs(5)) {
        crate::debug_log::log_debug("[OverlaySmoke] status=failed reason=renderer_not_ready");
        std::thread::sleep(Duration::from_millis(150));
        return 1;
    }
    let mut cards = Vec::with_capacity(CARD_COUNT);
    for index in 0..CARD_COUNT {
        let trace_id = format!("smoke-{}-{index}", std::process::id());
        super::latency::begin(&trace_id);
        let Some((hwnd, thread)) = spawn_card(index, trace_id.clone()) else {
            close_cards(&cards);
            return 1;
        };
        cards.push((hwnd, trace_id, thread));
    }

    let chunks = [
        "Google Sans Flex",
        "Google Sans Flex renders every result",
        "Google Sans Flex renders every result smoothly while the response grows.",
        "Google Sans Flex renders every result smoothly while the response grows.\n\nThe compositor coalesces updates without blocking provider input.",
    ];
    for chunk in chunks {
        for (index, (hwnd, trace_id, _)) in cards.iter().enumerate() {
            if index == NON_STREAMING_CARD_INDEX {
                continue;
            }
            super::latency::mark(trace_id, "provider_first_output");
            update_window_text(*hwnd, chunk);
        }
        std::thread::sleep(Duration::from_millis(18));
    }

    for (index, (hwnd, trace_id, _)) in cards.iter().enumerate() {
        if index == NON_STREAMING_CARD_INDEX {
            super::latency::mark(trace_id, "provider_first_output");
        }
        {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                state.is_streaming_active = false;
            }
        }
        let final_text = if index + 1 == CARD_COUNT {
            "<html><head></head><body><h2>Isolated HTML</h2><p>Compatibility path is visible.</p></body></html>"
        } else {
            "# Unified compositor\n\nGoogle Sans Flex remains stable while every overlay finishes independently."
        };
        update_window_text(*hwnd, final_text);
        super::latency::mark(trace_id, "provider_complete");
        super::raise_window(*hwnd);
    }

    let rendered = cards.iter().all(|(_, trace_id, _)| {
        super::latency::wait_for_phase(trace_id, "final_fit_completed", Duration::from_secs(8))
    });
    let restarted = super::scene_compositor::restart_and_wait(Duration::from_secs(10));
    let passed = rendered && restarted;
    std::thread::sleep(Duration::from_millis(500));
    close_cards(&cards);
    for (_, _, thread) in cards {
        let _ = thread.join();
    }
    crate::debug_log::log_debug(&format!(
        "[OverlaySmoke] status={} cards={CARD_COUNT} restart_restored={restarted}",
        if passed { "passed" } else { "failed" }
    ));
    std::thread::sleep(Duration::from_millis(150));
    if passed { 0 } else { 1 }
}

fn spawn_card(index: usize, trace_id: String) -> Option<(HWND, std::thread::JoinHandle<()>)> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let com = unsafe { CoInitialize(None) };
        let left = 80 + (index as i32 * 54);
        let top = 80 + (index as i32 * 46);
        let hwnd = create_result_window(ResultWindowParams {
            target_rect: RECT {
                left,
                top,
                right: left + 620,
                bottom: top + 240,
            },
            win_type: WindowType::Primary,
            context: RefineContext::None,
            model_id: "smoke".to_string(),
            provider: "local".to_string(),
            streaming_enabled: index != NON_STREAMING_CARD_INDEX,
            start_editing: false,
            preset_prompt: String::new(),
            custom_bg_color: super::get_chain_color(index),
            initial_text: String::new(),
            preset_id: None,
            is_chain_root: index == 0,
            latency_trace_id: Some(trace_id),
        });
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNA);
        }
        let _ = sender.send(SendHwnd(hwnd));
        unsafe {
            let mut message = MSG::default();
            while GetMessageW(&mut message, None, 0, 0).into() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
                if !IsWindow(Some(hwnd)).as_bool() {
                    break;
                }
            }
            if com.is_ok() {
                CoUninitialize();
            }
        }
    });
    receiver
        .recv_timeout(Duration::from_secs(4))
        .ok()
        .map(|hwnd| (hwnd.0, thread))
}

fn close_cards(cards: &[(HWND, String, std::thread::JoinHandle<()>)]) {
    for (hwnd, _, _) in cards {
        unsafe {
            let _ = PostMessageW(Some(*hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}
