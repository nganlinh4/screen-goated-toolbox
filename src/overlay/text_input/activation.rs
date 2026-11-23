use super::state::*;
use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, PostMessageW, SetTimer};

const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(8);

struct Activation {
    session: u64,
    deadline: Instant,
    in_flight: bool,
    page_ready: bool,
    prior_quiet: bool,
    prior_observation: Option<super::delivery::PriorObservation>,
}

thread_local! {
    static PENDING: RefCell<Option<Activation>> = const { RefCell::new(None) };
}

pub(super) fn start(hwnd: HWND, session: u64) {
    PENDING.with(|pending| {
        *pending.borrow_mut() = Some(Activation {
            session,
            deadline: Instant::now() + ACTIVATION_TIMEOUT,
            in_flight: false,
            page_ready: false,
            prior_quiet: false,
            prior_observation: None,
        })
    });
    {
        let hwnd_value = hwnd.0 as isize;
        let observation = super::delivery::observe_prior_quiescence(
            session,
            std::sync::Arc::new(move || unsafe {
                let _ = PostMessageW(
                    Some(HWND(hwnd_value as *mut _)),
                    WM_APP_PRIOR_QUIET,
                    WPARAM(session as usize),
                    LPARAM(0),
                );
            }),
        );
        PENDING.with(|pending| {
            if let Some(current) = pending.borrow_mut().as_mut()
                && current.session == session
            {
                current.prior_observation = Some(observation);
            }
        });
    }
    if !arm_timer(
        || unsafe { SetTimer(Some(hwnd), 7, 100, None) },
        || fail(hwnd, session),
    ) {
        return;
    }
    tick(hwnd);
}

fn arm_timer(arm: impl FnOnce() -> usize, on_failure: impl FnOnce()) -> bool {
    if arm() == 0 {
        on_failure();
        false
    } else {
        true
    }
}

pub(super) fn stop(hwnd: HWND) {
    PENDING.with(|pending| *pending.borrow_mut() = None);
    unsafe {
        let _ = KillTimer(Some(hwnd), 7);
    }
}

pub(super) fn retire(hwnd: HWND) {
    stop(hwnd);
    super::focus::cancel(hwnd);
    let previous = EDITOR_SESSIONS.current();
    EDITOR_READY.store(0, Ordering::SeqCst);
    EDITOR_SESSIONS.end();
    CALLBACK_OWNER.store(0, Ordering::SeqCst);
    if let Some(session) = previous {
        invalidate(session);
    }
}

pub(super) fn invalidate(session: u64) {
    TEXT_INPUT_WEBVIEW.with(|webview| {
        if let Some(wv) = webview.borrow().as_ref() {
            let script = format!(
                "if(window.__sgtTextSession === '{session}') window.__sgtTextSession = null;"
            );
            let _ = wv.evaluate_script(&script);
        }
    });
}

pub(super) fn tick(hwnd: HWND) {
    let next = PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        let current = pending.as_mut()?;
        if Instant::now() >= current.deadline {
            let session = current.session;
            return Some((session, true));
        }
        if current.in_flight || current.page_ready {
            return None;
        }
        current.in_flight = true;
        Some((current.session, false))
    });
    let Some((session, timed_out)) = next else {
        return;
    };
    if timed_out {
        fail(hwnd, session);
        return;
    }
    let nonce = DOCUMENT_NONCE.lock().unwrap().clone();
    let Some(nonce) = nonce else {
        retry(session);
        return;
    };
    let payload = serde_json::json!({ "nonce": nonce, "session": session.to_string() });
    let script = format!(
        r#"(() => {{
        const p = {payload};
        const reply = status => ({{nonce:window.__sgtDocumentNonce, session:p.session, status}});
        const editor = document.getElementById('editor');
        if (window.__sgtDocumentNonce !== p.nonce || !editor || !window.__sgtCancelledThrough) return reply('retry');
        const current = window.__sgtTextEpoch || '0';
        if (p.session === current && window.__sgtTextSession === p.session) return reply('ready');
        if (p.session.length < current.length || (p.session.length === current.length && p.session <= current)) return reply('stale');
        window.__sgtTextEpoch = p.session;
        window.__sgtTextSession = p.session;
        window.__sgtCancelledThrough = '0';
        window.__sgtTextRevision = '0';
        window.__sgtPassiveRevision = '0';
        editor.value = '';
        editor.selectionStart = editor.selectionEnd = 0;
        return reply('ready');
    }})()"#
    );
    let hwnd_value = hwnd.0 as isize;
    let submitted = TEXT_INPUT_WEBVIEW.with(|webview| {
        webview.borrow().as_ref().is_some_and(|wv| {
            wv.evaluate_script_with_callback(&script, move |result| {
                let ready = valid_ack(&result, &nonce, session);
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(hwnd_value as *mut _)),
                        WM_APP_ACTIVATION_ACK,
                        WPARAM(session as usize),
                        LPARAM(isize::from(ready)),
                    );
                }
            })
            .is_ok()
        })
    });
    if !submitted {
        retry(session);
    }
}

fn fail(hwnd: HWND, session: u64) {
    let matched = PENDING.with(|pending| {
        pending
            .borrow()
            .as_ref()
            .is_some_and(|item| item.session == session)
    });
    if !matched {
        return;
    }
    stop(hwnd);
    if EDITOR_SESSIONS.is_current(session) {
        EDITOR_READY.store(0, Ordering::SeqCst);
        EDITOR_SESSIONS.end();
        CALLBACK_OWNER.store(0, Ordering::SeqCst);
        PASSIVE_CAPTURE_ENABLED.store(false, Ordering::SeqCst);
        let language = CFG_LANG.lock().unwrap().clone();
        crate::overlay::auto_copy_badge::show_error_notification(
            crate::gui::locale::LocaleText::get(&language)
                .shell
                .text_input_unavailable,
        );
    }
}

fn valid_ack(result: &str, nonce: &str, session: u64) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(result) else {
        return false;
    };
    value.get("nonce").and_then(|v| v.as_str()) == Some(nonce)
        && value.get("session").and_then(|v| v.as_str()) == Some(session.to_string().as_str())
        && value.get("status").and_then(|v| v.as_str()) == Some("ready")
}

fn retry(session: u64) {
    PENDING.with(|pending| {
        if let Some(current) = pending.borrow_mut().as_mut()
            && current.session == session
        {
            current.in_flight = false;
        }
    });
}

pub(super) fn ack(hwnd: HWND, session: u64, ready: bool) -> bool {
    let matched = PENDING.with(|pending| {
        pending
            .borrow()
            .as_ref()
            .is_some_and(|item| item.session == session)
    });
    if !matched {
        return false;
    }
    if !ready {
        retry(session);
        return false;
    }
    PENDING.with(|pending| {
        if let Some(current) = pending.borrow_mut().as_mut()
            && current.session == session
        {
            current.page_ready = true;
            current.in_flight = false;
        }
    });
    finish_if_ready(hwnd, session)
}

pub(super) fn prior_quiet(hwnd: HWND, session: u64) -> bool {
    PENDING.with(|pending| {
        if let Some(current) = pending.borrow_mut().as_mut()
            && current.session == session
        {
            current.prior_quiet = true;
        }
    });
    finish_if_ready(hwnd, session)
}

fn finish_if_ready(hwnd: HWND, session: u64) -> bool {
    let ready = PENDING.with(|pending| {
        pending
            .borrow()
            .as_ref()
            .is_some_and(|item| item.session == session && item.page_ready && item.prior_quiet)
    });
    if !ready {
        return false;
    }
    stop(hwnd);
    if !EDITOR_SESSIONS.is_current(session) {
        return false;
    }
    EDITOR_READY.store(session, Ordering::SeqCst);
    true
}

#[cfg(test)]
mod tests {
    use super::{arm_timer, valid_ack};

    #[test]
    fn timer_failure_runs_cleanup_instead_of_waiting_forever() {
        let failed = std::cell::Cell::new(0);
        assert!(!arm_timer(|| 0, || failed.set(failed.get() + 1)));
        assert_eq!(failed.get(), 1);
        assert!(arm_timer(
            || 7,
            || panic!("successful timer must not fail activation")
        ));
    }

    #[test]
    fn ready_ack_must_match_document_session_and_status() {
        assert!(valid_ack(
            r#"{"nonce":"n","session":"9","status":"ready"}"#,
            "n",
            9
        ));
        assert!(!valid_ack(
            r#"{"nonce":"other","session":"9","status":"ready"}"#,
            "n",
            9
        ));
        assert!(!valid_ack(
            r#"{"nonce":"n","session":"10","status":"ready"}"#,
            "n",
            9
        ));
        assert!(!valid_ack(
            r#"{"nonce":"n","session":"9","status":"retry"}"#,
            "n",
            9
        ));
        assert!(!valid_ack("null", "n", 9));
    }
}
