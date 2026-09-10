use super::*;
use std::time::Duration;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::w;

#[test]
fn busy_controller_receives_one_request_without_blocking_the_sender() {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    let owner = std::thread::spawn(move || unsafe {
        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE,
            w!("Static"),
            w!(""),
            WS_POPUP,
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
        register(hwnd);
        ready_tx.send(()).unwrap();
        resume_rx.recv().unwrap();
        let mut count = 0;
        let mut message = MSG::default();
        while PeekMessageW(
            &mut message,
            Some(hwnd),
            WM_RECONCILE_STACK,
            WM_RECONCILE_STACK,
            PM_REMOVE,
        )
        .as_bool()
        {
            count += 1;
        }
        assert!(take(hwnd));
        assert!(!take(hwnd));
        unregister(hwnd);
        assert!(!take(hwnd));
        DestroyWindow(hwnd).unwrap();
        count
    });
    ready_rx.recv().unwrap();
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let sender = std::thread::spawn(move || {
        for _ in 0..64 {
            request();
        }
        done_tx.send(()).unwrap();
    });
    let completed_while_busy = done_rx.recv_timeout(Duration::from_secs(1)).is_ok();
    resume_tx.send(()).unwrap();
    sender.join().unwrap();
    assert_eq!(owner.join().unwrap(), 1);
    assert!(
        completed_while_busy,
        "stacking must not wait for the controller's UI thread"
    );
}
