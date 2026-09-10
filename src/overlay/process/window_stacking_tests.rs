use super::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[test]
fn owner_reconciles_its_visible_edges_without_activation() {
    unsafe {
        let controller = create_processing_window(RECT {
            left: -20000,
            top: -20000,
            right: -19800,
            bottom: -19800,
        });
        assert!(!controller.is_invalid());
        SendMessageW(controller, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
        let other = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            w!("Static"),
            w!(""),
            WS_POPUP | WS_VISIBLE,
            -20000,
            -20000,
            10,
            10,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let foreground = GetForegroundWindow();
        request_stack_reconciliation();
        let mut message = MSG::default();
        assert!(
            PeekMessageW(
                &mut message,
                Some(controller),
                super::super::stacking::WM_RECONCILE_STACK,
                super::super::stacking::WM_RECONCILE_STACK,
                PM_REMOVE
            )
            .as_bool()
        );
        DispatchMessageW(&message);
        let mut edge = GetWindow(other, GW_HWNDPREV).unwrap();
        let mut own_edges = 0;
        loop {
            if GetWindow(edge, GW_OWNER).ok() == Some(controller) {
                own_edges += 1;
            }
            let Ok(previous) = GetWindow(edge, GW_HWNDPREV) else {
                break;
            };
            edge = previous;
        }
        assert_eq!(own_edges, 4);
        assert_eq!(GetForegroundWindow(), foreground);
        DestroyWindow(other).unwrap();
        DestroyWindow(controller).unwrap();
    }
}
