use crate::overlay::creation_recovery::retry::{Attempt, Retry};

static RETRY: Retry = Retry::new("3d", super::ensure_recovery_started);

pub(super) fn begin() -> Option<Attempt<'static>> {
    RETRY.begin()
}
pub(super) fn pending() -> bool {
    RETRY.pending()
}
pub(super) fn schedule() {
    RETRY.schedule();
}
