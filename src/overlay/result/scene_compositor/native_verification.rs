// Native region/visibility tests only: these do not create the composition
// renderer or verify physical pointer-event delivery.
#[cfg(test)]
mod tests {
    use super::super::input_surface::{
        INJECT_REGION_FAILURE, create_input_surface, update_input_regions,
    };
    use super::super::protocol::{SceneCard, SceneRect};
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;
    use std::sync::{Mutex, Once};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::Graphics::Gdi::HBRUSH;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::w;

    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static REGISTER_RECEIVER_CLASS: Once = Once::new();
    unsafe extern "system" fn receiver_wnd_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    fn pump_messages() {
        unsafe {
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn create_receiver_window(x: i32, y: i32, width: i32, height: i32) -> anyhow::Result<HWND> {
        unsafe {
            let instance = GetModuleHandleW(None)?;
            let class_name = w!("SGTNativeReceiverHarness");
            REGISTER_RECEIVER_CLASS.call_once(|| {
                let class = WNDCLASSW {
                    lpfnWndProc: Some(receiver_wnd_proc),
                    hInstance: instance.into(),
                    lpszClassName: class_name,
                    hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                    hbrBackground: HBRUSH(std::ptr::null_mut()),
                    ..Default::default()
                };
                let _ = RegisterClassW(&class);
            });

            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class_name,
                w!("Native Receiver Harness Window"),
                WS_POPUP | WS_VISIBLE,
                x,
                y,
                width,
                height,
                None,
                None,
                Some(instance.into()),
                None,
            )?;
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                width,
                height,
                SWP_SHOWWINDOW,
            );
            pump_messages();
            Ok(hwnd)
        }
    }

    fn harness_test_card(id: isize, rect: SceneRect) -> SceneCard {
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
    fn outside_point_hits_receiver_beneath_clipped_input_surface() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _region_lock = super::super::input_surface::REGION_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // 1. Create native receiver window beneath overlay covering (100, 100) to (700, 500)
        let receiver = create_receiver_window(100, 100, 600, 400).expect("create receiver window");

        // 2. Create input surface window covering (0, 0) to (1920, 1080)
        let input_surface = create_input_surface(0, 0, 1920, 1080).expect("create input surface");
        unsafe {
            let _ = SetWindowPos(
                input_surface,
                Some(HWND_TOPMOST),
                0,
                0,
                1920,
                1080,
                SWP_SHOWWINDOW,
            );
        }

        // 3. Place card at (300, 300) size 200x200
        let mut cards = HashMap::new();
        cards.insert(
            -10,
            harness_test_card(
                -10,
                SceneRect {
                    x: 300,
                    y: 300,
                    width: 200,
                    height: 200,
                },
            ),
        );

        let state = update_input_regions(input_surface, &cards, &[], 0, 0, 1920, 1080);
        assert_eq!(state.visible_cards, 1);
        assert_eq!(state.input_region_count, 1);
        assert!(!state.is_hidden);
        pump_messages();

        // Point (150, 150) is OUTSIDE the card (which is at 300, 300) but INSIDE the receiver window
        let pt_outside = POINT { x: 150, y: 150 };
        let hit_hwnd = unsafe { WindowFromPoint(pt_outside) };

        // Input surface must never intercept clicks outside its card regions
        assert_ne!(
            hit_hwnd, input_surface,
            "Outside point must never hit input surface"
        );
        assert_eq!(
            hit_hwnd, receiver,
            "Outside point must hit receiver window beneath overlay"
        );

        unsafe {
            let _ = DestroyWindow(input_surface);
            let _ = DestroyWindow(receiver);
        }
        pump_messages();
    }

    #[test]
    fn inside_point_hits_input_surface() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _region_lock = super::super::input_surface::REGION_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let receiver = create_receiver_window(100, 100, 600, 400).expect("create receiver window");
        let input_surface = create_input_surface(0, 0, 1920, 1080).expect("create input surface");
        unsafe {
            let _ = SetWindowPos(
                input_surface,
                Some(HWND_TOPMOST),
                0,
                0,
                1920,
                1080,
                SWP_SHOWWINDOW,
            );
        }

        let mut cards = HashMap::new();
        cards.insert(
            -11,
            harness_test_card(
                -11,
                SceneRect {
                    x: 200,
                    y: 200,
                    width: 200,
                    height: 200,
                },
            ),
        );

        let _ = update_input_regions(input_surface, &cards, &[], 0, 0, 1920, 1080);
        pump_messages();

        // Point (250, 250) is INSIDE the card (200, 200, 200, 200)
        let pt_inside = POINT { x: 250, y: 250 };
        let hit_hwnd = unsafe { WindowFromPoint(pt_inside) };
        assert_eq!(
            hit_hwnd, input_surface,
            "Inside point must hit input surface window"
        );

        unsafe {
            let _ = DestroyWindow(input_surface);
            let _ = DestroyWindow(receiver);
        }
        pump_messages();
    }

    #[test]
    fn final_card_closure_immediately_hides_surface_and_clears_region() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _region_lock = super::super::input_surface::REGION_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let receiver = create_receiver_window(100, 100, 600, 400).expect("create receiver window");
        let input_surface = create_input_surface(0, 0, 1920, 1080).expect("create input surface");
        unsafe {
            let _ = SetWindowPos(
                input_surface,
                Some(HWND_TOPMOST),
                0,
                0,
                1920,
                1080,
                SWP_SHOWWINDOW,
            );
        }

        let mut cards = HashMap::new();
        cards.insert(
            -30,
            harness_test_card(
                -30,
                SceneRect {
                    x: 200,
                    y: 200,
                    width: 200,
                    height: 200,
                },
            ),
        );

        let state_active = update_input_regions(input_surface, &cards, &[], 0, 0, 1920, 1080);
        assert_eq!(state_active.visible_cards, 1);
        assert!(!state_active.is_hidden);
        assert!(unsafe { IsWindowVisible(input_surface).as_bool() });
        pump_messages();

        // Close final card (empty card map)
        cards.clear();
        let state_closed = update_input_regions(input_surface, &cards, &[], 0, 0, 1920, 1080);
        assert_eq!(state_closed.visible_cards, 0);
        assert_eq!(state_closed.input_region_count, 0);
        assert!(state_closed.is_hidden);
        assert!(!unsafe { IsWindowVisible(input_surface).as_bool() });
        pump_messages();

        // The formerly inside point (250, 250) now hits the receiver window because surface is hidden
        let pt = POINT { x: 250, y: 250 };
        let hit = unsafe { WindowFromPoint(pt) };
        assert_ne!(
            hit, input_surface,
            "Input surface must not be hit after final card closure"
        );
        assert_eq!(
            hit, receiver,
            "Point must hit receiver after final card closure"
        );

        cards.insert(
            -30,
            harness_test_card(
                -30,
                SceneRect {
                    x: 200,
                    y: 200,
                    width: 200,
                    height: 200,
                },
            ),
        );
        let restored = update_input_regions(input_surface, &cards, &[], 0, 0, 1920, 1080);
        assert!(!restored.is_hidden);
        pump_messages();
        assert_eq!(unsafe { WindowFromPoint(pt) }, input_surface);

        unsafe {
            let _ = DestroyWindow(input_surface);
            let _ = DestroyWindow(receiver);
        }
        pump_messages();
    }

    #[test]
    fn injected_region_failure_hides_input_surface() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _region_lock = super::super::input_surface::REGION_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // Fault injection: simulate SetWindowRgn failure, verify surface is immediately hidden
        let input_surface = create_input_surface(0, 0, 800, 600).expect("create input surface");
        let mut cards = HashMap::new();
        cards.insert(
            -50,
            harness_test_card(
                -50,
                SceneRect {
                    x: 50,
                    y: 50,
                    width: 100,
                    height: 100,
                },
            ),
        );

        INJECT_REGION_FAILURE.store(true, Ordering::SeqCst);
        let state = update_input_regions(input_surface, &cards, &[], 0, 0, 800, 600);
        INJECT_REGION_FAILURE.store(false, Ordering::SeqCst);

        assert!(state.is_hidden);
        assert_eq!(state.visible_cards, 0);
        assert!(!unsafe { IsWindowVisible(input_surface).as_bool() });

        unsafe {
            let _ = DestroyWindow(input_surface);
        }
        pump_messages();
    }
}
