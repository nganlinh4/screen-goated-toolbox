//! Scripts bound for a selector window. Posted messages only wake the window;
//! the script text stays owned here, so a failed post or a window destroyed
//! with messages still queued never leaks or reconstructs a raw allocation.

use std::sync::Mutex;

struct PendingScript {
    hwnd: isize,
    token: u64,
    script: String,
}

#[derive(Default)]
pub(super) struct ScriptQueue {
    pending: Vec<PendingScript>,
}

impl ScriptQueue {
    pub(super) const fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, hwnd: isize, token: u64, script: String) {
        self.pending.push(PendingScript {
            hwnd,
            token,
            script,
        });
    }

    /// Removes and returns the scripts queued for `hwnd`, in submission order.
    pub(super) fn take(&mut self, hwnd: isize) -> Vec<(u64, String)> {
        let mut taken = Vec::new();
        self.pending.retain_mut(|pending| {
            if pending.hwnd != hwnd {
                return true;
            }
            taken.push((pending.token, std::mem::take(&mut pending.script)));
            false
        });
        taken
    }

    /// Drops every script for `hwnd`; used when its post failed or it closed.
    pub(super) fn discard(&mut self, hwnd: isize) {
        self.pending.retain(|pending| pending.hwnd != hwnd);
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.pending.len()
    }
}

pub(super) static SCRIPTS: Mutex<ScriptQueue> = Mutex::new(ScriptQueue::new());

pub(super) fn with_scripts<T>(effect: impl FnOnce(&mut ScriptQueue) -> T) -> T {
    effect(&mut SCRIPTS.lock().unwrap_or_else(|error| error.into_inner()))
}

#[cfg(test)]
mod tests {
    use super::ScriptQueue;

    #[test]
    fn scripts_are_delivered_per_window_in_order() {
        let mut queue = ScriptQueue::new();
        queue.push(1, 0, "a".to_string());
        queue.push(2, 5, "other".to_string());
        queue.push(1, 7, "b".to_string());
        assert_eq!(
            queue.take(1),
            vec![(0, "a".to_string()), (7, "b".to_string())]
        );
        assert!(queue.take(1).is_empty());
        assert_eq!(queue.len(), 1);
    }

    #[test]
    fn destroyed_window_releases_its_undelivered_scripts() {
        let mut queue = ScriptQueue::new();
        queue.push(1, 3, "preview".to_string());
        queue.push(2, 4, "replacement".to_string());
        queue.discard(1);
        assert!(queue.take(1).is_empty());
        assert_eq!(queue.take(2), vec![(4, "replacement".to_string())]);
        assert_eq!(queue.len(), 0);
    }
}
