//! Pins an IPC WebView to the one local page it was built for.
//!
//! The WebView may load that exact page once. Every later navigation (a link,
//! a script redirect, or a file dropped onto the window) is refused, and IPC is
//! accepted only from that page, so its handlers and secrets never reach
//! another document.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct TrustedPage {
    url: String,
    ipc_source: String,
    opened: AtomicBool,
}

impl TrustedPage {
    /// None when `url` is not a valid URI.
    pub(crate) fn new(url: &str) -> Option<Arc<Self>> {
        let ipc_source = url.parse::<wry::http::Uri>().ok()?.to_string();
        Some(Arc::new(Self {
            url: url.to_string(),
            ipc_source,
            opened: AtomicBool::new(false),
        }))
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    /// Admits only the first load of the exact page URL.
    pub(crate) fn allow_navigation(&self, requested: &str) -> bool {
        requested == self.url
            && self
                .opened
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
    }

    pub(crate) fn allow_ipc(&self, source: &str) -> bool {
        self.opened.load(Ordering::SeqCst) && source == self.ipc_source
    }
}

#[cfg(test)]
mod tests {
    use super::TrustedPage;

    #[test]
    fn only_initial_document_and_its_exact_source_are_accepted() {
        let page = TrustedPage::new("http://127.0.0.1:1234/token/page").unwrap();
        assert!(!page.allow_ipc(page.url()));
        assert!(!page.allow_navigation("http://127.0.0.1:1234/other"));
        assert!(page.allow_navigation(page.url()));
        assert!(page.allow_ipc(page.url()));
        assert!(!page.allow_ipc("http://127.0.0.1:1234/token/page#other"));
        assert!(!page.allow_ipc("file:///C:/Users/me/Downloads/dropped.html"));
        assert!(!page.allow_navigation(page.url()));
        assert!(!page.allow_navigation("file:///C:/Users/me/Downloads/dropped.html"));
    }

    #[test]
    fn an_invalid_page_url_is_not_trusted() {
        assert!(TrustedPage::new("not a url with spaces").is_none());
    }
}
