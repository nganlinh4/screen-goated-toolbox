use super::card_document::compositor_document;
use super::protocol::{ChildEvent, HostCommand, SceneCard};
use crate::win_types::HwndWrapper;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, RGN_OR, SetWindowRgn,
};
use windows::Win32::System::Com::CoUninitialize;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;
use wry::{Rect, WebContext, WebView, WebViewBuilder};

const WM_DRAIN_COMMANDS: u32 = WM_APP + 91;
const INPUT_TIMER_ID: usize = 1;
static HOST_HWND: AtomicIsize = AtomicIsize::new(0);
static RENDERER_READY: AtomicBool = AtomicBool::new(false);
static COMMANDS: LazyLock<Mutex<VecDeque<HostCommand>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));
pub(super) static CARDS: LazyLock<Mutex<HashMap<isize, SceneCard>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static STDOUT: LazyLock<Mutex<std::io::Stdout>> = LazyLock::new(|| Mutex::new(std::io::stdout()));

thread_local! {
    static WEBVIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
    static WEB_CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
}

pub fn run() -> anyhow::Result<()> {
    crate::initialization::init_com_and_dpi();

    let hwnd = create_host_window()?;
    HOST_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
    unsafe {
        let _ = SetTimer(Some(hwnd), INPUT_TIMER_ID, 100, None);
    }
    start_input_thread();

    let webview = create_webview(hwnd)?;
    WEBVIEW.with(|slot| *slot.borrow_mut() = Some(webview));

    unsafe {
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        WEBVIEW.with(|slot| *slot.borrow_mut() = None);
        WEB_CONTEXT.with(|slot| *slot.borrow_mut() = None);
        HOST_HWND.store(0, Ordering::SeqCst);
        CoUninitialize();
    }
    Ok(())
}

fn create_host_window() -> anyhow::Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("SGTResultSceneCompositor");
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: class_name,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        let _ = RegisterClassW(&window_class);

        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("Result compositor"),
            WS_POPUP | WS_CLIPCHILDREN,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            None,
        )?;
        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        DwmExtendFrameIntoClientArea(hwnd, &margins)?;
        let empty = CreateRectRgn(0, 0, 0, 0);
        let _ = SetWindowRgn(hwnd, Some(empty), false);
        Ok(hwnd)
    }
}

fn create_webview(hwnd: HWND) -> anyhow::Result<WebView> {
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) } as u32;
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) } as u32;
    let data_dir = crate::paths::app_sgt_dir()
        .join("webview_data")
        .join("result-compositor");
    let wrapper = HwndWrapper(hwnd);
    let isolated_origin = super::isolated_server::start()?;
    let compositor_html = compositor_document(&isolated_origin);
    WEB_CONTEXT.with(|slot| {
        *slot.borrow_mut() = Some(WebContext::new(Some(data_dir)));
        let mut context = slot.borrow_mut();
        WebViewBuilder::new_with_web_context(context.as_mut().unwrap())
            .with_bounds(Rect {
                position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(width, height)),
            })
            .with_transparent(true)
            .with_focused(false)
            .with_custom_protocol("sgtresult".to_string(), move |_id, request| {
                let path = request.uri().path();
                if path == "/" || path == "/index.html" {
                    return super::web_response::compositor_response(
                        200,
                        "text/html; charset=utf-8",
                        Cow::Owned(compositor_html.as_bytes().to_vec()),
                        "no-store",
                    );
                }
                if path == "/font.ttf" {
                    return super::web_response::compositor_response(
                        200,
                        "font/ttf",
                        Cow::Borrowed(super::font::bytes()),
                        "public, max-age=31536000, immutable",
                    );
                }
                super::web_response::compositor_response(
                    404,
                    "text/plain",
                    Cow::Borrowed(b"Not Found"),
                    "no-store",
                )
            })
            .with_url("sgtresult://localhost/index.html")
            .with_ipc_handler(|request: wry::http::Request<String>| {
                handle_renderer_event(request.body());
            })
            .build_as_child(&wrapper)
            .map_err(Into::into)
    })
}

fn start_input_thread() {
    std::thread::spawn(|| {
        for line in BufReader::new(std::io::stdin())
            .lines()
            .map_while(Result::ok)
        {
            let Ok(command) = serde_json::from_str::<HostCommand>(&line) else {
                continue;
            };
            COMMANDS.lock().unwrap().push_back(command);
            let hwnd_value = HOST_HWND.load(Ordering::SeqCst);
            if hwnd_value != 0 {
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd_value as *mut std::ffi::c_void)),
                        WM_DRAIN_COMMANDS,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            }
        }
        let hwnd_value = HOST_HWND.load(Ordering::SeqCst);
        if hwnd_value != 0 {
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd_value as *mut std::ffi::c_void)),
                    WM_CLOSE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        }
    });
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_DRAIN_COMMANDS => {
                drain_commands(hwnd);
                LRESULT(0)
            }
            WM_DISPLAYCHANGE => {
                resize_host(hwnd);
                LRESULT(0)
            }
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            WM_MOUSEMOVE if super::button_input::handle_mouse_move() => LRESULT(0),
            WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP
                if super::button_input::has_active_drag() =>
            {
                finish_button_drag(hwnd);
                LRESULT(0)
            }
            WM_CAPTURECHANGED | WM_CANCELMODE if super::button_input::has_active_drag() => {
                recover_button_drag(hwnd);
                LRESULT(0)
            }
            WM_TIMER if wparam.0 == INPUT_TIMER_ID => {
                poll_compositor_cursor(hwnd);
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

fn drain_commands(hwnd: HWND) {
    if !RENDERER_READY.load(Ordering::SeqCst) {
        return;
    }
    let mut redraw_region = false;
    let mut handled_command = false;
    loop {
        let command = COMMANDS.lock().unwrap().pop_front();
        let Some(command) = command else {
            break;
        };
        if command == HostCommand::Shutdown {
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
        handled_command = true;
        redraw_region |= command_requires_region_redraw(&command);
        apply_scene_state(&command);
        if let Ok(command_json) = serde_json::to_string(&command) {
            let script = format!("window.applyHostCommand({command_json});");
            WEBVIEW.with(|slot| {
                if let Some(webview) = slot.borrow().as_ref()
                    && let Err(error) = webview.evaluate_script(&script)
                {
                    emit_event(ChildEvent::CommandError {
                        command: command_name(&command).to_string(),
                        id: command_id(&command),
                        error: error.to_string(),
                    });
                }
            });
        }
    }
    if handled_command {
        update_window_region(hwnd, redraw_region);
    }
}

fn command_requires_region_redraw(command: &HostCommand) -> bool {
    !matches!(
        command,
        HostCommand::Geometry { .. } | HostCommand::Theme { .. } | HostCommand::Raise { .. }
    )
}

fn handle_renderer_event(body: &str) {
    let host_value = HOST_HWND.load(Ordering::SeqCst);
    if host_value != 0 {
        let host = HWND(host_value as *mut std::ffi::c_void);
        let outcome = {
            let cards = CARDS.lock().unwrap();
            super::button_input::handle_renderer_message(body, host, &cards)
        };
        match outcome {
            super::button_input::RendererInput::Unhandled => {}
            super::button_input::RendererInput::RefreshRegion => {
                update_window_region(host, true);
                return;
            }
            super::button_input::RendererInput::Event(event) => {
                emit_event(event);
                return;
            }
            super::button_input::RendererInput::EventAndRefresh(event) => {
                if event == ChildEvent::DragStarted {
                    evaluate_script("window.__SGT_BUTTON_SCENE__?.setDragActive(true);");
                }
                emit_event(event);
                update_window_region(host, true);
                return;
            }
        }
    }
    match body {
        "renderer_ready" => {
            RENDERER_READY.store(true, Ordering::SeqCst);
            emit_event(ChildEvent::Ready);
            let hwnd_value = HOST_HWND.load(Ordering::SeqCst);
            if hwnd_value != 0 {
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd_value as *mut std::ffi::c_void)),
                        WM_DRAIN_COMMANDS,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            }
        }
        "renderer_heartbeat" => emit_event(ChildEvent::Heartbeat),
        _ => {
            if let Ok(event) = serde_json::from_str::<ChildEvent>(body) {
                match event {
                    ChildEvent::Navigation { .. }
                    | ChildEvent::Interaction { .. }
                    | ChildEvent::ButtonAction { .. }
                    | ChildEvent::DragStarted
                    | ChildEvent::DragFinished { .. }
                    | ChildEvent::FitDiagnostic { .. }
                    | ChildEvent::CardDiagnostic { .. }
                    | ChildEvent::FontReady { .. }
                    | ChildEvent::CommandError { .. } => {
                        emit_event(event);
                    }
                    ChildEvent::Ready | ChildEvent::Heartbeat => {}
                }
            }
        }
    }
}

fn command_name(command: &HostCommand) -> &'static str {
    match command {
        HostCommand::Snapshot { .. } => "snapshot",
        HostCommand::Upsert { .. } => "upsert",
        HostCommand::Stream { .. } => "stream",
        HostCommand::Finalize { .. } => "finalize",
        HostCommand::Geometry { .. } => "geometry",
        HostCommand::Controls { .. } => "controls",
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

fn command_id(command: &HostCommand) -> Option<isize> {
    match command {
        HostCommand::Upsert { card } => Some(card.id),
        HostCommand::Stream { card } => Some(card.id),
        HostCommand::Finalize { card } => Some(card.id),
        HostCommand::Remove { id }
        | HostCommand::NavigateBack { id }
        | HostCommand::NavigateForward { id } => Some(*id),
        HostCommand::Raise { id, .. } => Some(*id),
        HostCommand::RefineText { id, .. } => Some(*id),
        HostCommand::Snapshot { .. }
        | HostCommand::Geometry { .. }
        | HostCommand::Controls { .. }
        | HostCommand::ExternalDrag { .. }
        | HostCommand::Theme { .. }
        | HostCommand::Shutdown => None,
    }
}

fn apply_scene_state(command: &HostCommand) {
    let mut cards = CARDS.lock().unwrap();
    match command {
        HostCommand::Snapshot { cards: snapshot } => {
            cards.clear();
            cards.extend(snapshot.iter().cloned().map(|card| (card.id, card)));
        }
        HostCommand::Upsert { card } => {
            cards.insert(card.id, card.clone());
        }
        HostCommand::Stream { card: update } => {
            if let Some(card) = cards.get_mut(&update.id) {
                card.body.clone_from(&update.body);
                card.document.clone_from(&update.document);
                card.refining = update.refining;
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
        | HostCommand::RefineText { .. }
        | HostCommand::Shutdown => {}
    }
}

fn update_window_region(hwnd: HWND, redraw: bool) {
    unsafe {
        let combined = CreateRectRgn(0, 0, 0, 0);
        let cards = CARDS.lock().unwrap();
        let mut visible_count = 0usize;
        for card in cards.values().filter(|card| card.visible) {
            visible_count += 1;
            let rect = CreateRectRgn(
                card.rect.x,
                card.rect.y,
                card.rect.x + card.rect.width,
                card.rect.y + card.rect.height,
            );
            let _ = CombineRgn(Some(combined), Some(combined), Some(rect), RGN_OR);
            let _ = DeleteObject(rect.into());
        }
        for region in super::button_input::interactive_regions() {
            let rect = CreateRectRgn(
                region.x,
                region.y,
                region.x + region.width,
                region.y + region.height,
            );
            let _ = CombineRgn(Some(combined), Some(combined), Some(rect), RGN_OR);
            let _ = DeleteObject(rect.into());
        }
        drop(cards);
        let _ = SetWindowRgn(hwnd, Some(combined), redraw);
        if visible_count == 0 {
            let _ = ShowWindow(hwnd, SW_HIDE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}

fn finish_button_drag(hwnd: HWND) {
    if let Some(event) = unsafe { super::button_input::finish_drag() } {
        emit_event(event);
        reset_button_cursor();
        update_window_region(hwnd, true);
    }
}

fn recover_button_drag(hwnd: HWND) {
    if let Some(event) = unsafe { super::button_input::recover_stale_drag(hwnd) } {
        emit_event(event);
        reset_button_cursor();
        update_window_region(hwnd, true);
    }
}

fn poll_compositor_cursor(hwnd: HWND) {
    if super::button_input::is_dragging() {
        recover_button_drag(hwnd);
        return;
    }
    let mut cursor = windows::Win32::Foundation::POINT::default();
    if unsafe { GetCursorPos(&mut cursor) }.is_err() {
        return;
    }
    let virtual_x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let virtual_y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let script = format!(
        "window.updateCursorPosition?.({}/(window.devicePixelRatio||1),{}/(window.devicePixelRatio||1));",
        cursor.x - virtual_x,
        cursor.y - virtual_y
    );
    evaluate_script(&script);
}

fn reset_button_cursor() {
    evaluate_script(
        "window.setResultDraggingCursor?.(false);window.__SGT_BUTTON_SCENE__?.setDragActive(false);",
    );
}

fn evaluate_script(script: &str) {
    WEBVIEW.with(|slot| {
        if let Some(webview) = slot.borrow().as_ref() {
            let _ = webview.evaluate_script(script);
        }
    });
}

fn resize_host(hwnd: HWND) {
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE,
        );
        WEBVIEW.with(|slot| {
            if let Some(webview) = slot.borrow().as_ref() {
                let _ = webview.set_bounds(Rect {
                    position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(
                        width as u32,
                        height as u32,
                    )),
                });
            }
        });
    }
}

fn emit_event(event: ChildEvent) {
    if let Ok(line) = serde_json::to_string(&event) {
        let mut stdout = STDOUT.lock().unwrap();
        let _ = writeln!(stdout, "{line}");
        let _ = stdout.flush();
    }
}

#[cfg(test)]
#[path = "child_tests.rs"]
mod tests;
