use super::super::WindowError;
use super::{Placement, PlacementTarget, Win32Window, WindowBounds, minimize, place};
use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, MSG, PM_REMOVE, PeekMessageW,
    RegisterClassW, TranslateMessage, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_OVERLAPPEDWINDOW,
};
use windows::core::w;

const SHORT_BUDGET: Duration = Duration::from_millis(60);
const START: WindowBounds = WindowBounds {
    x: 10,
    y: 20,
    width: 200,
    height: 150,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Never,
    Restore,
    Placement,
}

struct FakeWindow<'a> {
    minimized: Cell<bool>,
    zoomed: Cell<bool>,
    bounds: Cell<WindowBounds>,
    accepts: bool,
    applies: bool,
    cancel: &'a AtomicBool,
    cancel_after: Stage,
    restores: Cell<u32>,
    placements: RefCell<Vec<Placement>>,
}

impl<'a> FakeWindow<'a> {
    fn new(cancel: &'a AtomicBool) -> Self {
        Self {
            minimized: Cell::new(false),
            zoomed: Cell::new(false),
            bounds: Cell::new(START),
            accepts: true,
            applies: true,
            cancel,
            cancel_after: Stage::Never,
            restores: Cell::new(0),
            placements: RefCell::new(Vec::new()),
        }
    }

    fn submitted(&self, stage: Stage) {
        if self.cancel_after == stage {
            self.cancel.store(true, Ordering::SeqCst);
        }
    }
}

impl PlacementTarget for FakeWindow<'_> {
    fn minimized(&self) -> bool {
        self.minimized.get()
    }

    fn restored(&self) -> bool {
        !self.minimized.get() && !self.zoomed.get()
    }

    fn bounds(&self) -> Option<WindowBounds> {
        Some(self.bounds.get())
    }

    fn submit_minimize(&self) -> bool {
        if self.accepts && self.applies {
            self.minimized.set(true);
        }
        self.accepts
    }

    fn submit_restore(&self) -> bool {
        self.restores.set(self.restores.get() + 1);
        if self.accepts && self.applies {
            // Like Windows: a minimized window first returns to its prior
            // maximized state; a maximized window then returns to normal.
            if self.minimized.get() {
                self.minimized.set(false);
            } else {
                self.zoomed.set(false);
            }
        }
        self.submitted(Stage::Restore);
        self.accepts
    }

    fn submit_placement(&self, placement: Placement) -> bool {
        self.placements.borrow_mut().push(placement);
        if self.accepts && self.applies {
            let mut bounds = self.bounds.get();
            match placement {
                Placement::Move { x, y } => (bounds.x, bounds.y) = (x, y),
                Placement::Resize { width, height } => {
                    (bounds.width, bounds.height) = (width, height)
                }
            }
            self.bounds.set(bounds);
        }
        self.submitted(Stage::Placement);
        self.accepts
    }
}

const MOVE: Placement = Placement::Move { x: 40, y: 50 };

#[test]
fn cancellation_before_submission_proves_no_window_effect() {
    let cancel = AtomicBool::new(true);
    let window = FakeWindow::new(&cancel);
    window.zoomed.set(true);
    let expected = Err(WindowError::Cancelled {
        effect_may_have_occurred: false,
    });
    assert_eq!(place(&window, MOVE, &cancel, SHORT_BUDGET), expected);
    assert_eq!(minimize(&window, &cancel, SHORT_BUDGET), expected);
    assert_eq!(window.restores.get(), 0);
    assert!(window.placements.borrow().is_empty());
    assert!(!window.minimized.get());
}

#[test]
fn normal_windows_are_positioned_without_a_restore_request() {
    let cancel = AtomicBool::new(false);
    let window = FakeWindow::new(&cancel);
    assert_eq!(place(&window, MOVE, &cancel, SHORT_BUDGET), Ok(true));
    assert_eq!(window.restores.get(), 0);
    assert_eq!(window.bounds.get().x, 40);
    assert_eq!(window.bounds.get().width, START.width);

    window.zoomed.set(true);
    let resize = Placement::Resize {
        width: 300,
        height: 250,
    };
    assert_eq!(place(&window, resize, &cancel, SHORT_BUDGET), Ok(true));
    assert_eq!(window.restores.get(), 1);
    assert_eq!(*window.placements.borrow(), vec![MOVE, resize]);
}

#[test]
fn a_window_minimized_from_maximized_is_restored_twice_before_placement() {
    let cancel = AtomicBool::new(false);
    let window = FakeWindow::new(&cancel);
    window.minimized.set(true);
    window.zoomed.set(true);
    assert_eq!(place(&window, MOVE, &cancel, SHORT_BUDGET), Ok(true));
    assert_eq!(window.restores.get(), 2);
    assert_eq!(window.bounds.get().x, 40);
}

#[test]
fn cancellation_during_restore_suppresses_the_later_placement() {
    let cancel = AtomicBool::new(false);
    let mut window = FakeWindow::new(&cancel);
    window.zoomed.set(true);
    window.cancel_after = Stage::Restore;
    assert_eq!(
        place(&window, MOVE, &cancel, SHORT_BUDGET),
        Err(WindowError::Cancelled {
            effect_may_have_occurred: true
        })
    );
    assert!(window.placements.borrow().is_empty());
}

#[test]
fn cancellation_after_placement_admission_cannot_revoke_that_effect() {
    let cancel = AtomicBool::new(false);
    let mut window = FakeWindow::new(&cancel);
    window.cancel_after = Stage::Placement;
    assert_eq!(
        place(&window, MOVE, &cancel, SHORT_BUDGET),
        Err(WindowError::Cancelled {
            effect_may_have_occurred: true
        })
    );
}

#[test]
fn unobserved_requests_report_an_unconfirmed_effect_with_current_bounds() {
    let cancel = AtomicBool::new(false);
    let mut window = FakeWindow::new(&cancel);
    window.applies = false;
    let started = Instant::now();
    assert_eq!(
        place(&window, MOVE, &cancel, SHORT_BUDGET),
        Err(WindowError::Unconfirmed {
            operation: "move",
            observed: Some(START),
        })
    );
    assert!(started.elapsed() >= SHORT_BUDGET);
    assert!(matches!(
        minimize(&window, &cancel, SHORT_BUDGET),
        Err(WindowError::Unconfirmed {
            operation: "minimize",
            ..
        })
    ));
    window.zoomed.set(true);
    assert!(matches!(
        place(&window, MOVE, &cancel, SHORT_BUDGET),
        Err(WindowError::Unconfirmed {
            operation: "restore",
            ..
        })
    ));
    assert_eq!(window.placements.borrow().len(), 1);
}

#[test]
fn rejected_submissions_report_no_success() {
    let cancel = AtomicBool::new(false);
    let mut window = FakeWindow::new(&cancel);
    window.accepts = false;
    assert_eq!(place(&window, MOVE, &cancel, SHORT_BUDGET), Ok(false));
    assert_eq!(minimize(&window, &cancel, SHORT_BUDGET), Ok(false));
}

unsafe extern "system" fn test_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

/// A hidden, non-activating tool window owned by its own thread. When
/// `pumping` is false that thread never processes messages until released,
/// exactly like a hung application window.
struct TestWindow {
    hwnd: usize,
    release: mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl TestWindow {
    fn spawn(pumping: bool) -> Self {
        let (ready_tx, ready_rx) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();
        let thread = std::thread::spawn(move || unsafe {
            let instance = GetModuleHandleW(None).unwrap();
            let class = w!("SGTComputerControlPlacementTest");
            // A repeated registration from a parallel test fails harmlessly.
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(test_window_proc),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            });
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                class,
                w!(""),
                WS_OVERLAPPEDWINDOW,
                START.x,
                START.y,
                START.width,
                START.height,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap();
            ready_tx.send(hwnd.0 as usize).unwrap();
            if pumping {
                while released.try_recv().is_err() {
                    let mut msg = MSG::default();
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            } else {
                let _ = released.recv();
            }
            // Destroying before pumping discards any request still queued.
            let _ = DestroyWindow(hwnd);
        });
        let hwnd = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        Self {
            hwnd,
            release,
            thread: Some(thread),
        }
    }

    fn target(&self) -> Win32Window {
        Win32Window(HWND(self.hwnd as *mut _))
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        let _ = self.release.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Run a window operation on its own thread so a blocking regression fails
/// the test instead of hanging the suite.
fn bounded<T: Send + 'static>(operation: impl FnOnce() -> T + Send + 'static) -> (T, Duration) {
    let (tx, rx) = mpsc::channel();
    let started = Instant::now();
    std::thread::spawn(move || {
        let _ = tx.send(operation());
    });
    let value = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("window operation blocked on a window that is not pumping messages");
    (value, started.elapsed())
}

#[test]
fn hung_window_placement_returns_an_unconfirmed_effect_instead_of_blocking() {
    let window = TestWindow::spawn(false);
    let hwnd = window.hwnd;
    let budget = Duration::from_millis(400);
    let (moved, elapsed) = bounded(move || {
        let target = Win32Window(HWND(hwnd as *mut _));
        place(&target, MOVE, &AtomicBool::new(false), budget)
    });
    assert_eq!(
        moved,
        Err(WindowError::Unconfirmed {
            operation: "move",
            observed: Some(START),
        })
    );
    assert!(elapsed < Duration::from_secs(5));

    let (minimized, _) = bounded(move || {
        let target = Win32Window(HWND(hwnd as *mut _));
        minimize(&target, &AtomicBool::new(false), budget)
    });
    assert!(matches!(
        minimized,
        Err(WindowError::Unconfirmed {
            operation: "minimize",
            ..
        })
    ));
    assert!(!window.target().minimized());
}

#[test]
fn cancellation_interrupts_waiting_on_a_hung_window() {
    let window = TestWindow::spawn(false);
    let hwnd = window.hwnd;
    let cancel = Arc::new(AtomicBool::new(false));
    let canceller = Arc::clone(&cancel);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        canceller.store(true, Ordering::SeqCst);
    });
    let (result, elapsed) = bounded(move || {
        let target = Win32Window(HWND(hwnd as *mut _));
        let resize = Placement::Resize {
            width: 320,
            height: 240,
        };
        place(&target, resize, &cancel, Duration::from_secs(30))
    });
    assert_eq!(
        result,
        Err(WindowError::Cancelled {
            effect_may_have_occurred: true
        })
    );
    assert!(elapsed < Duration::from_secs(5));
}

#[test]
fn responsive_window_placement_is_verified_from_observed_geometry() {
    let window = TestWindow::spawn(true);
    let hwnd = window.hwnd;
    let (results, _) = bounded(move || {
        let target = Win32Window(HWND(hwnd as *mut _));
        let cancel = AtomicBool::new(false);
        let budget = Duration::from_secs(3);
        let moved = place(&target, MOVE, &cancel, budget);
        let resize = Placement::Resize {
            width: 260,
            height: 180,
        };
        let resized = place(&target, resize, &cancel, budget);
        (moved, resized, target.bounds())
    });
    assert_eq!(results.0, Ok(true));
    assert_eq!(results.1, Ok(true));
    assert_eq!(
        results.2,
        Some(WindowBounds {
            x: 40,
            y: 50,
            width: 260,
            height: 180,
        })
    );
}
