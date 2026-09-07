use super::protocol::{SceneCard, SceneRect};
use std::collections::HashMap;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MOUSE_EVENT_KIND, COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CombineRgn, CreateRectRgn, DeleteObject, EqualRgn, GetWindowRgn, HBRUSH, HRGN, RGN_OR,
    ScreenToClient, SetWindowRgn,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

static REGISTER_INPUT_CLASS: Once = Once::new();
static REFINE_TEXT_ACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
pub(super) static INJECT_REGION_FAILURE: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
pub(super) static REGION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct AppliedInputState {
    pub visible_cards: usize,
    pub input_region_count: usize,
    pub is_hidden: bool,
}

impl AppliedInputState {
    pub(super) fn hidden() -> Self {
        Self {
            visible_cards: 0,
            input_region_count: 0,
            is_hidden: true,
        }
    }
}

pub(super) fn set_refine_text_active(active: bool) {
    REFINE_TEXT_ACTIVE.store(active, Ordering::SeqCst);
}

pub(super) fn is_refine_text_active() -> bool {
    REFINE_TEXT_ACTIVE.load(Ordering::SeqCst)
}

pub(super) fn create_input_surface(
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> anyhow::Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class_name = w!("SGTResultInputSurface");
        REGISTER_INPUT_CLASS.call_once(|| {
            let class = WNDCLASSW {
                lpfnWndProc: Some(input_surface_wnd_proc),
                hInstance: instance.into(),
                lpszClassName: class_name,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(std::ptr::null_mut()),
                ..Default::default()
            };
            let _ = RegisterClassW(&class);
        });

        let hwnd = CreateWindowExW(
            WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name,
            w!("Result Input Surface"),
            WS_POPUP,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            None,
        )?;

        super::visual_region::hide(hwnd);
        Ok(hwnd)
    }
}

unsafe extern "system" fn input_surface_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_MOUSEACTIVATE => {
                if is_refine_text_active() {
                    LRESULT(MA_ACTIVATE as isize)
                } else {
                    LRESULT(MA_NOACTIVATE as isize)
                }
            }
            WM_SETCURSOR => {
                let mut cur = HCURSOR::default();
                if super::child::get_dcomp_cursor(&mut cur).is_ok() && !cur.is_invalid() {
                    let _ = SetCursor(Some(cur));
                    return LRESULT(1);
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            WM_CAPTURECHANGED | WM_CANCELMODE => {
                super::pointer_input::capture_lost();
                LRESULT(0)
            }
            WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP
            | WM_MBUTTONDOWN | WM_MBUTTONUP => {
                forward_mouse_message(hwnd, message, wparam, lparam);
                LRESULT(0)
            }
            WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
                forward_wheel_message(hwnd, message, wparam, lparam);
                LRESULT(0)
            }
            super::gesture::WM_APP_GESTURE_DISPATCH => {
                super::gesture::dispatch_pending_gesture(wparam.0);
                LRESULT(0)
            }
            super::pointer_input::WM_APP_POINTER_DISPATCH => {
                super::pointer_input::dispatch();
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_DESTROY => {
                super::pointer_input::cancel();
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

fn forward_mouse_message(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) {
    let x = (lparam.0 & 0xffff) as i16 as i32;
    let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
    let vkeys = (wparam.0 & 0xffff) as i32;
    if super::pointer_input::route(hwnd, message, POINT { x, y }) {
        return;
    }
    super::child::send_mouse_to_webview(
        COREWEBVIEW2_MOUSE_EVENT_KIND(message as i32),
        COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(vkeys),
        0,
        POINT { x, y },
    );
}

pub(super) fn forward_observed_mouse(hwnd: HWND, mut point: POINT, button: u32, message: u32) {
    if unsafe { ScreenToClient(hwnd, &mut point) }.as_bool() {
        let mut keys = if message != WM_MOUSEMOVE {
            0
        } else {
            match button {
                2 => 2,
                4 => 16,
                _ => 1,
            }
        };
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_SHIFT};
        for (key, flag) in [(VK_SHIFT, 4), (VK_CONTROL, 8)] {
            if unsafe { GetKeyState(key.0 as i32) } < 0 {
                keys |= flag;
            }
        }
        super::child::send_mouse_to_webview(
            COREWEBVIEW2_MOUSE_EVENT_KIND(message as i32),
            COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(keys),
            0,
            point,
        );
    }
}

fn forward_wheel_message(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) {
    let screen_x = (lparam.0 & 0xffff) as i16 as i32;
    let screen_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
    let mut pt = POINT {
        x: screen_x,
        y: screen_y,
    };
    unsafe {
        let _ = ScreenToClient(hwnd, &mut pt);
    }
    let wheel_delta = ((wparam.0 >> 16) & 0xffff) as i16 as i32;
    let vkeys = (wparam.0 & 0xffff) as i32;
    super::child::send_mouse_to_webview(
        COREWEBVIEW2_MOUSE_EVENT_KIND(message as i32),
        COREWEBVIEW2_MOUSE_EVENT_VIRTUAL_KEYS(vkeys),
        wheel_delta as u32,
        pt,
    );
}

pub(super) fn update_input_regions(
    hwnd: HWND,
    cards: &HashMap<isize, SceneCard>,
    interactive_regions: &[SceneRect],
    display_x: i32,
    display_y: i32,
    display_w: i32,
    display_h: i32,
) -> AppliedInputState {
    #[cfg(test)]
    if INJECT_REGION_FAILURE.load(Ordering::SeqCst) {
        unsafe {
            let empty = CreateRectRgn(0, 0, 0, 0);
            let _ = SetWindowRgn(hwnd, Some(empty), false);
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        return AppliedInputState::hidden();
    }

    let mut visible_cards = 0usize;
    for card in cards.values() {
        if card.visible {
            visible_cards += 1;
        }
    }

    // If no cards are visible, input surface must disappear unconditionally.
    // Stale or delayed interactive button regions cannot resurrect dead input islands.
    if visible_cards == 0 {
        super::visual_region::hide(hwnd);
        return AppliedInputState::hidden();
    }

    unsafe {
        let combined = CreateRectRgn(0, 0, 0, 0);
        if combined.is_invalid() {
            let _ = ShowWindow(hwnd, SW_HIDE);
            return AppliedInputState::hidden();
        }

        let mut input_region_count = 0usize;
        let mut region_valid = true;

        for card in cards.values().filter(|card| card.visible) {
            if !card.external_navigation {
                let left = card.rect.x.clamp(0, display_w);
                let top = card.rect.y.clamp(0, display_h);
                let right = card
                    .rect
                    .x
                    .saturating_add(card.rect.width.max(0))
                    .clamp(0, display_w);
                let bottom = card
                    .rect
                    .y
                    .saturating_add(card.rect.height.max(0))
                    .clamp(0, display_h);
                if right > left && bottom > top {
                    region_valid &= union_rect(combined, left, top, right - left, bottom - top);
                    input_region_count += 1;
                }
            } else {
                let edges = super::region::external_resize_rects(
                    card.rect.x,
                    card.rect.y,
                    card.rect.width,
                    card.rect.height,
                    super::region::resize_edge_width(96),
                );
                for edge in edges {
                    let left = edge.left.clamp(0, display_w);
                    let top = edge.top.clamp(0, display_h);
                    let right = edge.right.clamp(0, display_w);
                    let bottom = edge.bottom.clamp(0, display_h);
                    if right > left && bottom > top {
                        region_valid &= union_rect(combined, left, top, right - left, bottom - top);
                        input_region_count += 1;
                    }
                }
            }
        }

        for region in interactive_regions {
            let left = region.x.clamp(0, display_w);
            let top = region.y.clamp(0, display_h);
            let right = region
                .x
                .saturating_add(region.width.max(0))
                .clamp(0, display_w);
            let bottom = region
                .y
                .saturating_add(region.height.max(0))
                .clamp(0, display_h);
            if right > left && bottom > top {
                region_valid &= union_rect(combined, left, top, right - left, bottom - top);
                input_region_count += 1;
            }
        }

        if !region_valid || input_region_count == 0 {
            let _ = DeleteObject(combined.into());
            let empty = CreateRectRgn(0, 0, 0, 0);
            let _ = SetWindowRgn(hwnd, Some(empty), false);
            let _ = ShowWindow(hwnd, SW_HIDE);
            AppliedInputState::hidden()
        } else {
            // Compare the actual native region so recreation and external changes cannot
            // leave a cached hit-test shape out of sync with the window.
            let current = CreateRectRgn(0, 0, 0, 0);
            let unchanged = !current.is_invalid()
                && GetWindowRgn(hwnd, current).0 != 0
                && EqualRgn(current, combined).as_bool();
            if !current.is_invalid() {
                let _ = DeleteObject(current.into());
            }
            let set_res = if unchanged {
                let _ = DeleteObject(combined.into());
                1
            } else {
                SetWindowRgn(hwnd, Some(combined), true)
            };
            if set_res == 0 {
                let _ = DeleteObject(combined.into());
                let _ = ShowWindow(hwnd, SW_HIDE);
                return AppliedInputState::hidden();
            }
            if !IsWindowVisible(hwnd).as_bool() {
                if SetWindowPos(
                    hwnd,
                    None,
                    display_x,
                    display_y,
                    display_w,
                    display_h,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                )
                .is_err()
                {
                    super::visual_region::hide(hwnd);
                    return AppliedInputState::hidden();
                }
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
            AppliedInputState {
                visible_cards,
                input_region_count,
                is_hidden: false,
            }
        }
    }
}

pub(super) fn activate_for_refine(hwnd: HWND) {
    set_refine_text_active(true);
    super::activation::focus_renderer(hwnd);
}

pub(super) fn restore_nonactivating(hwnd: HWND) {
    set_refine_text_active(false);
    super::activation::restore_nonactivating_style(hwnd);
}

pub(super) fn sync_input_surface_bounds(input_hwnd: HWND, x: i32, y: i32, width: i32, height: i32) {
    unsafe {
        let _ = SetWindowPos(
            input_hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE,
        );
    }
}

unsafe fn union_rect(region: HRGN, x: i32, y: i32, width: i32, height: i32) -> bool {
    unsafe {
        let rect = CreateRectRgn(x, y, x.saturating_add(width), y.saturating_add(height));
        if rect.is_invalid() {
            return false;
        }
        let result = CombineRgn(Some(region), Some(region), Some(rect), RGN_OR);
        let _ = DeleteObject(rect.into());
        result.0 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cards_with_stale_regions_hides_window_and_returns_hidden() {
        let _lock = REGION_LOCK.lock().unwrap();
        let hwnd = create_input_surface(0, 0, 800, 600).expect("create test surface");
        let cards: HashMap<isize, SceneCard> = HashMap::new();
        let stale_regions = vec![SceneRect {
            x: 10,
            y: 10,
            width: 100,
            height: 100,
        }];

        let state = update_input_regions(hwnd, &cards, &stale_regions, 0, 0, 800, 600);
        assert_eq!(state.visible_cards, 0);
        assert_eq!(state.input_region_count, 0);
        assert!(state.is_hidden);

        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }

    fn test_card(id: isize, rect: SceneRect) -> SceneCard {
        SceneCard {
            id,
            rect: rect.clone(),
            control_rect: rect,
            body: "result".to_string(),
            document: Some("test".to_string()),
            external_navigation: false,
            navigation_loading: false,
            refining: false,
            background: "#ffffff".to_string(),
            opacity: 100,
            visible: true,
            streaming: false,
            streaming_enabled: false,
            stack_order: 1,
            controls: Default::default(),
            presentation: crate::overlay::result::ResultPresentation::Standard,
            backdrop_data_url: None,
            foreground_color: None,
            preferred_font_size: None,
            source_vertical: false,
            source_regions: Vec::new(),
            source_segments: Vec::new(),
            source_replacement: false,
        }
    }

    #[test]
    fn input_surface_region_aligns_on_negative_origin_desktop() {
        let _lock = REGION_LOCK.lock().unwrap();
        let hwnd = create_input_surface(-1920, 0, 3840, 1080).expect("create test surface");
        let mut cards = HashMap::new();
        cards.insert(
            1,
            test_card(
                1,
                SceneRect {
                    x: 500,
                    y: 100,
                    width: 400,
                    height: 300,
                },
            ),
        );

        let state = update_input_regions(hwnd, &cards, &[], -1920, 0, 3840, 1080);
        assert_eq!(state.visible_cards, 1);
        assert_eq!(state.input_region_count, 1);
        assert!(!state.is_hidden);

        unsafe {
            assert_eq!(
                update_input_regions(hwnd, &cards, &[], -1920, 0, 3840, 1080),
                state
            );
            let _ = ShowWindow(hwnd, SW_HIDE);
            assert_eq!(
                update_input_regions(hwnd, &cards, &[], -1920, 0, 3840, 1080),
                state
            );
            assert!(IsWindowVisible(hwnd).as_bool());
            cards.get_mut(&1).unwrap().rect.x = 900;
            let _ = update_input_regions(hwnd, &cards, &[], -1920, 0, 3840, 1080);
            let actual = CreateRectRgn(0, 0, 0, 0);
            let expected = CreateRectRgn(900, 100, 1300, 400);
            assert_ne!(GetWindowRgn(hwnd, actual).0, 0);
            assert!(EqualRgn(actual, expected).as_bool());
            let _ = DeleteObject(actual.into());
            let _ = DeleteObject(expected.into());
            let _ = DestroyWindow(hwnd);
        }
    }

    #[test]
    fn fault_injected_region_failure_immediately_hides_surface() {
        let _lock = REGION_LOCK.lock().unwrap();
        let hwnd = create_input_surface(0, 0, 800, 600).expect("create test surface");
        let mut cards = HashMap::new();
        cards.insert(
            1,
            test_card(
                1,
                SceneRect {
                    x: 50,
                    y: 50,
                    width: 200,
                    height: 200,
                },
            ),
        );

        INJECT_REGION_FAILURE.store(true, Ordering::SeqCst);
        let state = update_input_regions(hwnd, &cards, &[], 0, 0, 800, 600);
        INJECT_REGION_FAILURE.store(false, Ordering::SeqCst);

        assert!(state.is_hidden);
        assert_eq!(state.visible_cards, 0);

        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}
