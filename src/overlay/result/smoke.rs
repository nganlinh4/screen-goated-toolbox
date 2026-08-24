use super::{
    RefineContext, ResultWindowParams, WINDOW_STATES, WindowType, create_result_window,
    update_window_text,
};
use crate::win_types::SendHwnd;
use std::time::Duration;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::System::Com::{CoInitialize, CoUninitialize};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, GetSystemMetrics, IsWindow, MSG, PostMessageW,
    SM_CXVIRTUALSCREEN, SM_XVIRTUALSCREEN, SW_SHOWNA, ShowWindow, TranslateMessage, WM_CLOSE,
};

const CARD_COUNT: usize = 6;
const NON_STREAMING_CARD_INDEX: usize = 4;

pub(crate) fn run() -> i32 {
    let interactive_hold_ms = interactive_acceptance_hold_ms();
    let processing_hold_ms = processing_acceptance_hold_ms();
    let card_count = if interactive_hold_ms > 0 {
        1
    } else if processing_hold_ms > 0 {
        4
    } else {
        CARD_COUNT
    };
    if !super::scene_compositor::wait_until_ready(Duration::from_secs(5)) {
        crate::debug_log::log_debug("[OverlaySmoke] status=failed reason=renderer_not_ready");
        std::thread::sleep(Duration::from_millis(150));
        return 1;
    }
    let Some(browser_url) =
        crate::overlay::html_components::font_manager::store_html_page(
            r#"<!doctype html><html><head><style>html,body{width:100%;height:100%;margin:0}body{background:#0db15b;color:#fff}input{margin:24px;padding:8px}output{display:block;margin:0 24px;font:700 24px sans-serif}</style></head><body><input id="keyboard-target" value="browser loaded" oninput="typed.textContent=this.value"><strong>Native browser page</strong><output id="typed">browser loaded</output><output id="keylog">waiting for key</output><script>addEventListener('keydown',event=>keylog.textContent='key: '+event.key)</script></body></html>"#.to_string(),
        )
    else {
        crate::debug_log::log_debug("[OverlaySmoke] status=failed reason=page_store_unavailable");
        return 1;
    };
    let mut cards = Vec::with_capacity(card_count);
    for index in 0..card_count {
        let trace_id = format!("smoke-{}-{index}", std::process::id());
        super::latency::begin(&trace_id);
        let Some((hwnd, thread)) = spawn_card(index, trace_id.clone(), processing_hold_ms > 0)
        else {
            close_cards(&cards);
            return 1;
        };
        cards.push((hwnd, trace_id, thread));
    }

    hold_for_processing_acceptance(&cards, processing_hold_ms);

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
        let final_text = if index == 0 {
            format!("[Open native browser page]({browser_url})")
        } else if index + 1 == card_count {
            r#"<!doctype html><html><head><style>html,body{width:100%;height:100%;margin:0}body{background:#172033;color:#f7f8ff}</style></head>
<body><button id="probe">Run</button><output id="result">waiting</output><script>
(function(){
  const probe = document.getElementById('probe');
  const result = document.getElementById('result');
  let count = 0;
  probe.addEventListener('click', function(){ count += 1; result.textContent = String(count); });
  probe.click();
  if (count === 1) document.body.style.background = '#0db15b';
  setTimeout(function(){
    const bridge = document.querySelector('script[data-sgt-card-identity]');
    const identity = bridge ? String(bridge.dataset.sgtCardIdentity || '').split(':') : [];
    const revision = Number(identity[1] || 0);
    if (!identity[0] || !revision || count !== 1 || result.textContent !== '1'
        || document.getElementById('probe') !== probe || !probe.isConnected) return;
    window.parent.postMessage({
      type: 'card_diagnostic', phase: 'interactive_document_alive',
      card_id: identity[0], document_revision: revision,
      payload_len: document.body.innerHTML.length,
      text_len: (document.body.innerText || '').trim().length,
      opacity: getComputedStyle(document.body).opacity,
      content_revision: 0, error: null
    }, '*');
  }, 350);
})();
</script></body></html>"#
                .to_string()
        } else {
            "# Unified compositor\n\nGoogle Sans Flex remains stable while every overlay finishes independently."
                .to_string()
        };
        update_window_text(*hwnd, &final_text);
        super::latency::mark(trace_id, "provider_complete");
        super::raise_window(*hwnd);
    }

    hold_for_interactive_acceptance(&cards, interactive_hold_ms);

    let rendered = cards
        .iter()
        .take(card_count.saturating_sub(1))
        .all(|(_, trace_id, _)| {
            super::latency::wait_for_phase(trace_id, "final_fit_completed", Duration::from_secs(8))
        });
    let raw_html_alive = cards.last().is_some_and(|(_, trace_id, _)| {
        super::latency::wait_for_phase(
            trace_id,
            "interactive_document_alive",
            Duration::from_secs(4),
        )
    });
    let raw_html_visible = cards.last().is_some_and(|(_, trace_id, _)| {
        super::latency::wait_for_phase(
            trace_id,
            "interactive_surface_visible",
            Duration::from_secs(4),
        )
    });
    let raw_html_pixels = cards.last().is_some_and(|(_, trace_id, _)| {
        super::latency::wait_for_phase(
            trace_id,
            "interactive_pixels_visible",
            Duration::from_secs(4),
        )
    });
    let markdown_link_pixels = cards.first().is_some_and(|(_, trace_id, _)| {
        super::latency::wait_for_phase(
            trace_id,
            "interactive_pixels_visible",
            Duration::from_secs(5),
        )
    });
    let browser_hwnd = cards.first().map(|(hwnd, _, _)| *hwnd);
    let back_restored = browser_hwnd.is_some_and(|hwnd| {
        unsafe {
            let _ = PostMessageW(
                Some(hwnd),
                super::event_handler::misc::WM_BACK_CLICK,
                WPARAM(0),
                LPARAM(0),
            );
        }
        wait_for_navigation(hwnd, 0, false)
    });
    let forward_restored = browser_hwnd.is_some_and(|hwnd| {
        unsafe {
            let _ = PostMessageW(
                Some(hwnd),
                super::event_handler::misc::WM_FORWARD_CLICK,
                WPARAM(0),
                LPARAM(0),
            );
        }
        wait_for_navigation(hwnd, 1, true)
    });
    let restarted = super::scene_compositor::restart_and_wait(Duration::from_secs(10));
    let passed = rendered
        && raw_html_alive
        && raw_html_visible
        && raw_html_pixels
        && markdown_link_pixels
        && back_restored
        && forward_restored
        && restarted;
    std::thread::sleep(Duration::from_millis(500));
    close_cards(&cards);
    for (_, _, thread) in cards {
        let _ = thread.join();
    }
    crate::debug_log::log_debug(&format!(
        "[OverlaySmoke] status={} cards={card_count} fitted_cards={rendered} raw_html_alive={raw_html_alive} raw_html_visible={raw_html_visible} raw_html_pixels={raw_html_pixels} markdown_link_pixels={markdown_link_pixels} back_restored={back_restored} forward_restored={forward_restored} restart_restored={restarted}",
        if passed { "passed" } else { "failed" }
    ));
    std::thread::sleep(Duration::from_millis(150));
    if passed { 0 } else { 1 }
}

fn interactive_acceptance_hold_ms() -> u64 {
    std::env::var("SGT_RESULT_COMPOSITOR_ACCEPTANCE_INTERACTIVE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|value| value.clamp(1_000, 120_000))
        .unwrap_or(0)
}

fn processing_acceptance_hold_ms() -> u64 {
    std::env::var("SGT_RESULT_COMPOSITOR_PROCESSING_ACCEPTANCE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|value| value.clamp(1_000, 120_000))
        .unwrap_or(0)
}

fn hold_for_processing_acceptance(
    cards: &[(HWND, String, std::thread::JoinHandle<()>)],
    hold_ms: u64,
) {
    if hold_ms == 0 {
        return;
    }
    if cards.is_empty() {
        return;
    }
    {
        let mut states = WINDOW_STATES.lock().unwrap();
        for (hwnd, _, _) in cards {
            if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                state.is_refining = true;
                state.is_streaming_active = true;
            }
        }
    }
    for (hwnd, _, _) in cards {
        super::scene_compositor::queue_window_sync(*hwnd);
        super::raise_window(*hwnd);
    }
    crate::debug_log::log_debug(&format!(
        "[OverlaySmoke] phase=processing_hold duration_ms={hold_ms}"
    ));
    std::thread::sleep(Duration::from_millis(hold_ms));
    {
        let mut states = WINDOW_STATES.lock().unwrap();
        for (hwnd, _, _) in cards {
            if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                state.is_refining = false;
            }
        }
    }
    for (hwnd, _, _) in cards {
        super::scene_compositor::queue_window_sync(*hwnd);
    }
}

fn hold_for_interactive_acceptance(
    cards: &[(HWND, String, std::thread::JoinHandle<()>)],
    hold_ms: u64,
) {
    if hold_ms == 0 {
        return;
    }
    if let Some((hwnd, _, _)) = cards.first() {
        super::raise_window(*hwnd);
    }
    crate::debug_log::log_debug(&format!(
        "[OverlaySmoke] phase=interactive_hold duration_ms={hold_ms}"
    ));
    std::thread::sleep(Duration::from_millis(hold_ms));
}

fn wait_for_navigation(hwnd: HWND, depth: usize, active: bool) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        let state_matches = WINDOW_STATES
            .lock()
            .unwrap()
            .get(&(hwnd.0 as isize))
            .is_some_and(|state| state.navigation_depth == depth && state.is_browsing == active);
        if state_matches && super::raw_webview::is_active(hwnd) == active {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    false
}

fn spawn_card(
    index: usize,
    trace_id: String,
    processing_acceptance: bool,
) -> Option<(HWND, std::thread::JoinHandle<()>)> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        let com = unsafe { CoInitialize(None) };
        let host_x = if super::scene_compositor::acceptance_offscreen() {
            super::scene_compositor::compositor_host_x(
                unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) },
                unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) }.max(1),
            )
        } else {
            0
        };
        let target_rect = if processing_acceptance {
            processing_acceptance_rect(index, host_x)
        } else {
            let left = host_x + 80 + (index as i32 * 54);
            let top = 80 + (index as i32 * 46);
            RECT {
                left,
                top,
                right: left + 620,
                bottom: top + 240,
            }
        };
        let hwnd = create_result_window(ResultWindowParams {
            target_rect,
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

fn processing_acceptance_rect(index: usize, host_x: i32) -> RECT {
    let (left, top, width, height) = match index {
        0 => (40, 40, 820, 72),
        1 => (900, 40, 380, 240),
        2 => (40, 150, 96, 620),
        _ => (180, 300, 220, 100),
    };
    RECT {
        left: host_x + left,
        top,
        right: host_x + left + width,
        bottom: top + height,
    }
}

fn close_cards(cards: &[(HWND, String, std::thread::JoinHandle<()>)]) {
    for (hwnd, _, _) in cards {
        unsafe {
            let _ = PostMessageW(Some(*hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
    }
}
