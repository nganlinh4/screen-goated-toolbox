//! Show and hide requests for the favorite bubble window.
//!
//! Requests can arrive while the window thread is still creating the window or
//! while it is fading out. The last request always wins: a show cancels a
//! fade-out, a hide during creation fades the new window out, and a show that
//! arrives after the close was committed reopens the bubble once it is gone.

use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Hidden,
    Creating,
    Visible,
    FadingOut,
    Closing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    None,
    Spawn,
    FadeIn,
    FadeOut,
    Close,
}

#[derive(Debug)]
pub(super) struct Lifecycle {
    phase: Phase,
    wanted: bool,
}

impl Lifecycle {
    const fn new() -> Self {
        Self {
            phase: Phase::Hidden,
            wanted: false,
        }
    }

    pub(super) fn show(&mut self) -> Action {
        self.wanted = true;
        match self.phase {
            Phase::Hidden => {
                self.phase = Phase::Creating;
                Action::Spawn
            }
            Phase::FadingOut => {
                self.phase = Phase::Visible;
                Action::FadeIn
            }
            Phase::Creating | Phase::Visible | Phase::Closing => Action::None,
        }
    }

    pub(super) fn hide(&mut self) -> Action {
        self.wanted = false;
        if self.phase == Phase::Visible {
            self.phase = Phase::FadingOut;
            return Action::FadeOut;
        }
        Action::None
    }

    /// The window exists; fade it out at once if a hide arrived meanwhile.
    pub(super) fn created(&mut self) -> Action {
        self.phase = Phase::Visible;
        if self.wanted {
            Action::None
        } else {
            self.phase = Phase::FadingOut;
            Action::FadeOut
        }
    }

    pub(super) fn creation_failed(&mut self) {
        self.phase = Phase::Hidden;
        self.wanted = false;
    }

    /// The fade-out reached zero opacity.
    pub(super) fn faded_out(&mut self) -> Action {
        if self.phase == Phase::FadingOut {
            self.phase = Phase::Closing;
            return Action::Close;
        }
        Action::None
    }

    /// The window thread ended; reopen if a show arrived during the close.
    pub(super) fn closed(&mut self) -> Action {
        self.phase = Phase::Hidden;
        if self.wanted {
            self.phase = Phase::Creating;
            Action::Spawn
        } else {
            Action::None
        }
    }
}

static LIFECYCLE: Mutex<Lifecycle> = Mutex::new(Lifecycle::new());

pub(super) fn update<T>(transition: impl FnOnce(&mut Lifecycle) -> T) -> T {
    transition(&mut LIFECYCLE.lock().unwrap_or_else(|error| error.into_inner()))
}

#[cfg(test)]
mod tests {
    use super::{Action, Lifecycle};

    #[test]
    fn show_during_fade_out_keeps_the_bubble() {
        let mut bubble = Lifecycle::new();
        assert_eq!(bubble.show(), Action::Spawn);
        assert_eq!(bubble.created(), Action::None);
        assert_eq!(bubble.hide(), Action::FadeOut);
        assert_eq!(bubble.show(), Action::FadeIn);
        assert_eq!(bubble.faded_out(), Action::None);
    }

    #[test]
    fn hide_before_the_window_exists_fades_it_out_once_created() {
        let mut bubble = Lifecycle::new();
        assert_eq!(bubble.show(), Action::Spawn);
        assert_eq!(bubble.hide(), Action::None);
        assert_eq!(bubble.created(), Action::FadeOut);
        assert_eq!(bubble.faded_out(), Action::Close);
        assert_eq!(bubble.closed(), Action::None);
    }

    #[test]
    fn show_after_the_close_was_committed_reopens_the_bubble() {
        let mut bubble = Lifecycle::new();
        bubble.show();
        bubble.created();
        bubble.hide();
        assert_eq!(bubble.faded_out(), Action::Close);
        assert_eq!(bubble.show(), Action::None);
        assert_eq!(bubble.closed(), Action::Spawn);
        assert_eq!(bubble.created(), Action::None);
    }

    #[test]
    fn repeated_requests_are_idempotent() {
        let mut bubble = Lifecycle::new();
        assert_eq!(bubble.hide(), Action::None);
        assert_eq!(bubble.show(), Action::Spawn);
        assert_eq!(bubble.show(), Action::None);
        bubble.creation_failed();
        assert_eq!(bubble.show(), Action::Spawn);
    }
}
