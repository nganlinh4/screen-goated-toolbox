//! Idle-time offers for optional control capabilities.

use std::collections::HashSet;

use super::*;

pub(super) struct Offers {
    browser_sent: bool,
    browser_checked: Instant,
    mcp_sent: HashSet<&'static str>,
    mcp_checked: Instant,
}

impl Offers {
    pub(super) fn new() -> Self {
        Self {
            browser_sent: false,
            browser_checked: Instant::now(),
            mcp_sent: HashSet::new(),
            mcp_checked: Instant::now(),
        }
    }

    pub(super) fn poll(&mut self, socket: &mut Sock, state: &mut Reader, last_event: Instant) {
        if !self.browser_sent
            && !state.awaiting
            && !state.control_revoked
            && state.has_command
            && last_event.elapsed() > Duration::from_secs(6)
            && self.browser_checked.elapsed() >= Duration::from_secs(4)
        {
            self.browser_checked = Instant::now();
            if !super::super::browser::is_connected()
                && !super::super::browser::recently_connected()
                && super::super::browser::offer_due()
                && super::session_control::foreground_is_browser()
            {
                self.browser_sent = true;
                let _ = send(
                    socket,
                    realtime_text(
                        "(Heads-up for you, not the user: they're working in a web browser and deep browser control \
isn't set up. If it fits the moment, briefly offer ONCE - in their language - to set it up via browser_setup for \
more precise page reading/acting. If they decline, call decline_browser_control.)",
                    ),
                );
                state.awaiting = true;
            }
        }

        if !state.awaiting
            && !state.control_revoked
            && state.has_command
            && last_event.elapsed() > Duration::from_secs(6)
            && self.mcp_checked.elapsed() >= Duration::from_secs(4)
        {
            self.mcp_checked = Instant::now();
            let title = super::super::uia::pointer_context().0;
            if let Some(id) = super::super::mcp::detect_uninstalled_match(&title)
                && self.mcp_sent.insert(id)
                && let Some(name) = super::super::mcp::display_name(id)
            {
                let _ = send(
                    socket,
                    realtime_text(&format!(
                        "(Heads-up for you, not the user: they're using {name}, which has a CURATED app-control \
integration giving you precise tools instead of clicking its UI. If it fits the moment, briefly offer ONCE - in \
their language - to set it up. They must say YES first (it installs + runs software), then call \
setup_app_integration with id:'{id}', confirmed:true. If they decline, call decline_app_integration with id:'{id}'.)"
                    )),
                );
                state.awaiting = true;
            }
        }
    }
}
