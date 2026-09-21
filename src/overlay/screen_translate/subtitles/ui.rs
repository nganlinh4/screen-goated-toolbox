//! Native layered region editor. It owns no OCR, translation or result rendering.
use super::{Session, editor_layout::Layout};
use anyhow::Result;
use std::{
    cell::RefCell,
    sync::{Arc, atomic::Ordering},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::GetDpiForWindow, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::w,
};

const REFRESH: u32 = WM_APP + 81;
struct Editor {
    session: Arc<Session>,
    scale: f32,
    drag: Option<(usize, POINT, RECT)>,
    pressed: Option<usize>,
    painted: Option<(RECT, String, u32, String)>,
}
thread_local! { static EDITOR: RefCell<Option<Editor>> = const { RefCell::new(None) }; }

pub(super) fn launch(session: Arc<Session>) {
    std::thread::spawn(move || {
        crate::initialization::init_com_and_dpi();
        let result = run(Arc::clone(&session));
        if let Err(error) = result {
            super::stop_expected(Some(&session));
            super::super::capture::notify_error(&error.to_string());
        }
        unsafe {
            windows::Win32::System::Com::CoUninitialize();
        }
    });
}

pub(super) fn refresh(session: &Session) {
    post(session, REFRESH);
}
pub(super) fn close(session: &Session) {
    post(session, WM_CLOSE);
}
fn post(session: &Session, message: u32) {
    let value = session.native_window.load(Ordering::Acquire);
    if value != 0 {
        unsafe {
            let _ = PostMessageW(Some(HWND(value as _)), message, WPARAM(0), LPARAM(0));
        }
    }
}

fn run(session: Arc<Session>) -> Result<()> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = w!("SGTSubtitleRegionEditor");
        let _ = RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        });
        let rect = session.view.lock().unwrap().rect;
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class,
            w!("SGT Subtitle Region"),
            WS_POPUP,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .and_then(crate::overlay::shell_policy::prepare)?;
        EDITOR.with_borrow_mut(|slot| {
            *slot = Some(Editor {
                session: Arc::clone(&session),
                scale: GetDpiForWindow(hwnd).max(96) as f32 / 96.0,
                drag: None,
                pressed: None,
                painted: None,
            })
        });
        session
            .native_window
            .store(hwnd.0 as isize, Ordering::Release);
        let setup = (|| -> Result<()> {
            if session.stop.load(Ordering::Acquire) {
                return Ok(());
            }
            SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE)?;
            update(hwnd)?;
            if !session.stop.load(Ordering::Acquire) {
                super::engine::start(Arc::clone(&session));
            }
            Ok(())
        })();
        if setup.is_err() || session.stop.load(Ordering::Acquire) {
            session.native_window.store(0, Ordering::Release);
            let _ = DestroyWindow(hwnd);
            EDITOR.with_borrow_mut(|slot| slot.take());
            return setup;
        }
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        session.native_window.store(0, Ordering::Release);
        if IsWindow(Some(hwnd)).as_bool() {
            let _ = DestroyWindow(hwnd);
        }
        super::stop_expected(Some(&session));
        EDITOR.with_borrow_mut(|slot| slot.take());
        Ok(())
    }
}

fn current() -> Option<(Arc<Session>, f32)> {
    EDITOR.with_borrow(|slot| slot.as_ref().map(|s| (Arc::clone(&s.session), s.scale)))
}

fn update(hwnd: HWND) -> Result<()> {
    let Some((session, scale)) = current() else {
        return Ok(());
    };
    let view = session.view.lock().unwrap().clone();
    unsafe {
        if session.stop.load(Ordering::Acquire) {
            let _ = ShowWindow(hwnd, SW_HIDE);
            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
        } else if !view.editing {
            EDITOR.with_borrow_mut(|slot| {
                if let Some(editor) = slot {
                    editor.drag = None;
                    editor.pressed = None;
                }
            });
            let _ = ReleaseCapture();
            let _ = ShowWindow(hwnd, SW_HIDE);
        } else {
            let key = (
                view.rect,
                view.error.clone(),
                scale.to_bits(),
                view.hotkey.clone(),
            );
            let changed = EDITOR.with_borrow(|slot| {
                slot.as_ref()
                    .is_none_or(|editor| editor.painted.as_ref() != Some(&key))
            });
            if changed {
                super::editor_paint::paint(hwnd, &view, scale)?;
                EDITOR.with_borrow_mut(|slot| {
                    if let Some(editor) = slot {
                        editor.painted = Some(key);
                    }
                });
            }
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    }
    Ok(())
}

fn paint_or_stop(hwnd: HWND) {
    if let Err(error) = update(hwnd) {
        if let Some((session, _)) = current() {
            super::stop_expected(Some(&session));
        }
        super::super::capture::notify_error(&error.to_string());
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            REFRESH => {
                paint_or_stop(hwnd);
                return LRESULT(0);
            }
            WM_ERASEBKGND => return LRESULT(1),
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut paint);
                let _ = EndPaint(hwnd, &paint);
                return LRESULT(0);
            }
            WM_CLOSE | WM_DISPLAYCHANGE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                if let Some((session, _)) = current() {
                    session.native_window.store(0, Ordering::Release);
                    super::stop_expected(Some(&session));
                }
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            WM_CAPTURECHANGED => {
                EDITOR.with_borrow_mut(|slot| {
                    if let Some(s) = slot {
                        s.drag = None;
                        s.pressed = None;
                    }
                });
                return LRESULT(0);
            }
            WM_DPICHANGED => {
                EDITOR.with_borrow_mut(|slot| {
                    if let Some(s) = slot {
                        s.scale = GetDpiForWindow(hwnd).max(96) as f32 / 96.0;
                    }
                });
                paint_or_stop(hwnd);
                return LRESULT(0);
            }
            WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MOUSEMOVE | WM_SETCURSOR => {
                let Some((session, scale)) = current() else {
                    return DefWindowProcW(hwnd, msg, w, l);
                };
                let view = session.view.lock().unwrap().clone();
                if !view.editing || session.stop.load(Ordering::Acquire) {
                    return LRESULT(0);
                }
                let mut point = POINT::default();
                let _ = GetCursorPos(&mut point);
                let hit = Layout::new(&view, scale).hit(point);
                match msg {
                    WM_LBUTTONDOWN => {
                        EDITOR.with_borrow_mut(|slot| {
                            if let Some(s) = slot {
                                s.pressed = hit;
                                s.drag = hit.filter(|i| *i <= 8).map(|i| (i, point, view.rect));
                            }
                        });
                        if hit.is_some() {
                            SetCapture(hwnd);
                        }
                    }
                    WM_LBUTTONUP => {
                        let quit = EDITOR.with_borrow_mut(|slot| {
                            let Some(s) = slot else {
                                return false;
                            };
                            let quit = s.pressed == Some(9) && hit == Some(9);
                            s.pressed = None;
                            s.drag = None;
                            quit
                        });
                        let _ = ReleaseCapture();
                        if quit {
                            // Hide synchronously. Destruction and cancellation belong to this HWND.
                            let _ = ShowWindow(hwnd, SW_HIDE);
                            let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                        }
                    }
                    WM_MOUSEMOVE => {
                        let drag = EDITOR.with_borrow(|slot| slot.as_ref().and_then(|s| s.drag));
                        if let Some((edge, origin, start)) = drag {
                            let rect = crate::overlay::selection::drag_region(
                                edge,
                                start,
                                POINT {
                                    x: point.x - origin.x,
                                    y: point.y - origin.y,
                                },
                                view.bounds,
                            );
                            session.move_region(rect);
                            paint_or_stop(hwnd);
                        }
                    }
                    WM_SETCURSOR => {
                        let drag = EDITOR
                            .with_borrow(|slot| slot.as_ref().and_then(|s| s.drag.map(|d| d.0)));
                        let cursor = match drag.or(hit) {
                            Some(0 | 4) => IDC_SIZENWSE,
                            Some(2 | 6) => IDC_SIZENESW,
                            Some(1 | 5) => IDC_SIZENS,
                            Some(3 | 7) => IDC_SIZEWE,
                            Some(8) => IDC_SIZEALL,
                            Some(9) => IDC_HAND,
                            _ => IDC_ARROW,
                        };
                        SetCursor(LoadCursorW(None, cursor).ok());
                        return LRESULT(1);
                    }
                    _ => {}
                }
                return LRESULT(0);
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, w, l)
    }
}
