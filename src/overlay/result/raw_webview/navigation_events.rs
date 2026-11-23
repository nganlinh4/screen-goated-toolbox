use std::cell::RefCell;
use std::rc::Rc;
use webview2_com::{
    NavigationCompletedEventHandler, NavigationStartingEventHandler, SourceChangedEventHandler,
};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;
use windows::core::{BOOL, PWSTR};
use wry::WebViewExtWindows;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum NavigationEvent {
    Started,
    Finished { success: bool, url: Option<String> },
}

#[derive(Default)]
pub(super) struct NavigationEvents {
    current: Option<u64>,
    pending: Option<NavigationEvent>,
    same_document: Option<String>,
    expected_url: Option<String>,
    expected_start: Option<String>,
    committed_url: Option<String>,
    loading: bool,
    succeeded: bool,
    posted: bool,
    retired: bool,
}

impl NavigationEvents {
    pub(super) fn invalidate(&mut self) {
        self.current = None;
        self.pending = None;
        self.same_document = None;
        self.expected_url = None;
        self.expected_start = None;
        self.loading = false;
        self.succeeded = false;
        self.posted = false;
    }
    fn started(&mut self, id: u64, redirected: bool, url: &str) -> bool {
        if self.retired {
            return false;
        }
        if redirected && self.current != Some(id) {
            return false;
        }
        if !redirected
            && self
                .expected_start
                .as_deref()
                .is_some_and(|expected| expected != url)
        {
            return false;
        }
        self.current = Some(id);
        self.expected_url = None;
        self.expected_start = None;
        self.pending = Some(NavigationEvent::Started);
        self.same_document = None;
        self.loading = true;
        self.succeeded = false;
        true
    }

    fn completed(&mut self, id: u64, success: bool, url: Option<String>) -> bool {
        if self.current != Some(id) {
            return false;
        }
        self.loading = false;
        self.succeeded = success;
        if success {
            self.committed_url = url.clone();
        }
        self.pending = Some(NavigationEvent::Finished { success, url });
        true
    }

    fn source_changed(&mut self, url: String) -> bool {
        if self.retired {
            return false;
        }
        if self.expected_url.as_deref() == Some(url.as_str()) {
            self.expected_url = None;
            self.expected_start = None;
            self.loading = false;
            self.succeeded = true;
            self.committed_url = Some(url.clone());
            self.pending = Some(NavigationEvent::Finished {
                success: true,
                url: Some(url),
            });
            return true;
        }
        if self.loading || !self.succeeded {
            return false;
        }
        self.committed_url = Some(url.clone());
        self.same_document = Some(url);
        true
    }

    pub(super) fn expect_url(&mut self, url: &str) {
        self.retired = false;
        fn without_fragment(value: &str) -> &str {
            value.split('#').next().unwrap_or_default()
        }
        self.expected_start = Some(url.to_owned());
        self.expected_url = self
            .committed_url
            .as_deref()
            .filter(|previous| without_fragment(previous) == without_fragment(url))
            .map(|_| url.to_owned());
    }

    pub(super) fn retire(&mut self) {
        self.invalidate();
        self.retired = true;
    }

    pub(super) fn take(&mut self) -> (Option<NavigationEvent>, Option<String>) {
        self.posted = false;
        (self.pending.take(), self.same_document.take())
    }

    fn request_post(&mut self) -> bool {
        if self.posted {
            false
        } else {
            self.posted = true;
            true
        }
    }
}

fn post_event(hwnd: HWND, events: &Rc<RefCell<NavigationEvents>>) {
    if unsafe {
        PostMessageW(
            Some(hwnd),
            super::WM_NAVIGATION_EVENT,
            Default::default(),
            Default::default(),
        )
    }
    .is_err()
    {
        events.borrow_mut().posted = false;
    }
}

pub(super) fn attach(
    webview: &wry::WebView,
    hwnd: HWND,
) -> windows::core::Result<Rc<RefCell<NavigationEvents>>> {
    let events = Rc::new(RefCell::new(NavigationEvents::default()));
    let core = webview.webview();
    unsafe {
        let start_events = events.clone();
        let starting = NavigationStartingEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut uri = PWSTR::null();
            let mut id = 0;
            let mut redirected = BOOL::default();
            let uri = if args.Uri(&mut uri).is_ok() && !uri.is_null() {
                super::public_web_url(&webview2_com::CoTaskMemPWSTR::from(uri).to_string())
            } else {
                None
            };
            let allowed = uri.as_ref().is_some()
                && args.NavigationId(&mut id).is_ok()
                && args.IsRedirected(&mut redirected).is_ok()
                && start_events.borrow_mut().started(
                    id,
                    redirected.as_bool(),
                    uri.as_deref().unwrap_or_default(),
                );
            args.SetCancel(!allowed)?;
            let should_post = allowed && start_events.borrow_mut().request_post();
            if should_post {
                post_event(hwnd, &start_events);
            }
            Ok(())
        }));
        let mut token = Default::default();
        core.add_NavigationStarting(&starting, &mut token)?;

        let completed_events = events.clone();
        let completed = NavigationCompletedEventHandler::create(Box::new(move |webview, args| {
            let (Some(webview), Some(args)) = (webview, args) else {
                return Ok(());
            };
            let mut id = 0;
            let mut success = BOOL::default();
            if args.NavigationId(&mut id).is_err() {
                return Ok(());
            }
            let success_known = args.IsSuccess(&mut success).is_ok();
            let url = if success.as_bool() {
                let mut source = PWSTR::null();
                if webview.Source(&mut source).is_ok() && !source.is_null() {
                    super::public_web_url(&webview2_com::CoTaskMemPWSTR::from(source).to_string())
                } else {
                    None
                }
            } else {
                None
            };
            let should_post = {
                let mut state = completed_events.borrow_mut();
                state.completed(id, success_known && success.as_bool() && url.is_some(), url)
                    && state.request_post()
            };
            if should_post {
                post_event(hwnd, &completed_events);
            }
            Ok(())
        }));
        core.add_NavigationCompleted(&completed, &mut token)?;

        let source_events = events.clone();
        let source_changed = SourceChangedEventHandler::create(Box::new(move |webview, args| {
            let (Some(webview), Some(args)) = (webview, args) else {
                return Ok(());
            };
            let mut new_document = BOOL::default();
            if args.IsNewDocument(&mut new_document).is_err() || new_document.as_bool() {
                return Ok(());
            }
            let mut source = PWSTR::null();
            if webview.Source(&mut source).is_err() || source.is_null() {
                return Ok(());
            }
            let Some(url) =
                super::public_web_url(&webview2_com::CoTaskMemPWSTR::from(source).to_string())
            else {
                return Ok(());
            };
            let should_post = {
                let mut state = source_events.borrow_mut();
                state.source_changed(url) && state.request_post()
            };
            if should_post {
                post_event(hwnd, &source_events);
            }
            Ok(())
        }));
        core.add_SourceChanged(&source_changed, &mut token)?;
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn superseded_completions_cannot_reveal_or_commit_the_new_navigation() {
        let mut events = NavigationEvents::default();
        events.started(1, false, "https://old.test/");
        events.started(2, false, "https://new.test/");
        assert!(!events.started(1, true, "https://old.test/redirect"));
        assert!(!events.completed(1, true, Some("https://old.test/".into())));
        assert_eq!(events.take(), (Some(NavigationEvent::Started), None));
        assert!(events.completed(2, true, Some("https://new.test/".into())));
        assert_eq!(
            events.take(),
            (
                Some(NavigationEvent::Finished {
                    success: true,
                    url: Some("https://new.test/".into())
                }),
                None
            )
        );
    }

    #[test]
    fn a_failed_latest_navigation_is_not_reported_as_success() {
        let mut events = NavigationEvents::default();
        events.started(4, false, "https://example.test/");
        assert!(events.completed(4, false, None));
        assert_eq!(
            events.take(),
            (
                Some(NavigationEvent::Finished {
                    success: false,
                    url: None
                }),
                None
            )
        );
    }

    #[test]
    fn retiring_a_view_rejects_its_late_completion() {
        let mut events = NavigationEvents::default();
        events.started(4, false, "https://example.test/");
        events.retire();
        assert!(!events.completed(4, true, Some("https://old.test/".into())));
        assert!(!events.started(5, false, "https://example.test/new"));
        assert!(!events.source_changed("https://example.test/#part".into()));
        assert_eq!(events.take(), (None, None));
        events.expect_url("https://example.test/new");
        assert!(events.started(6, false, "https://example.test/new"));
    }

    #[test]
    fn same_document_changes_preserve_pending_success_and_coalesce() {
        let mut events = NavigationEvents::default();
        events.started(1, false, "https://example.test/");
        assert!(events.request_post());
        assert!(!events.request_post());
        assert!(!events.source_changed("https://example.test/#loading".into()));
        events.completed(1, true, Some("https://example.test/".into()));
        assert!(events.source_changed("https://example.test/#one".into()));
        assert!(events.source_changed("https://example.test/#two".into()));
        assert_eq!(
            events.take(),
            (
                Some(NavigationEvent::Finished {
                    success: true,
                    url: Some("https://example.test/".into())
                }),
                Some("https://example.test/#two".into())
            )
        );
        assert!(events.request_post());
    }

    #[test]
    fn host_requested_fragment_navigation_completes_without_starting_event() {
        let mut events = NavigationEvents::default();
        events.started(1, false, "https://example.test/");
        events.completed(1, true, Some("https://example.test/".into()));
        events.take();
        events.invalidate();
        events.expect_url("https://example.test/#part");
        assert!(!events.source_changed("https://example.test/#other".into()));
        assert!(events.source_changed("https://example.test/#part".into()));
        assert_eq!(
            events.take(),
            (
                Some(NavigationEvent::Finished {
                    success: true,
                    url: Some("https://example.test/#part".into())
                }),
                None
            )
        );
    }

    #[test]
    fn pending_host_navigation_rejects_stale_start_and_path_spoof() {
        let mut events = NavigationEvents::default();
        events.started(1, false, "https://example.test/old");
        events.completed(1, true, Some("https://example.test/old".into()));
        events.take();
        events.invalidate();
        events.expect_url("https://example.test/new");
        assert!(!events.source_changed("https://example.test/new".into()));
        assert!(!events.started(2, false, "https://example.test/stale"));
        assert!(events.started(3, false, "https://example.test/new"));
        assert!(!events.completed(2, true, Some("https://example.test/stale".into())));
    }
}
