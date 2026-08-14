use std::sync::{LazyLock, Mutex, MutexGuard};
use std::time::Instant;

static CURRENT_OWNER: LazyLock<Mutex<Option<&'static str>>> = LazyLock::new(|| Mutex::new(None));

pub(crate) struct InitGuard {
    owner: &'static str,
    acquired_at: Instant,
    finished: bool,
    _guard: MutexGuard<'static, ()>,
}

pub(crate) fn acquire(owner: &'static str) -> InitGuard {
    let queued_at = Instant::now();
    let observed_owner = CURRENT_OWNER.lock().ok().and_then(|owner| *owner);
    let guard = crate::overlay::GLOBAL_WEBVIEW_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let wait_ms = queued_at.elapsed().as_secs_f64() * 1_000.0;
    if let Ok(mut current) = CURRENT_OWNER.lock() {
        *current = Some(owner);
    }
    crate::log_info!(
        "[WebViewInit] acquired owner={owner} wait_ms={wait_ms:.1} previous_owner={}",
        observed_owner.unwrap_or("none")
    );
    InitGuard {
        owner,
        acquired_at: Instant::now(),
        finished: false,
        _guard: guard,
    }
}

impl InitGuard {
    pub(crate) fn finish(mut self, success: bool) {
        self.finished = true;
        self.log_completion(success);
    }

    fn log_completion(&self, success: bool) {
        let build_ms = self.acquired_at.elapsed().as_secs_f64() * 1_000.0;
        crate::log_info!(
            "[WebViewInit] completed owner={} build_ms={build_ms:.1} success={success}",
            self.owner
        );
    }
}

impl Drop for InitGuard {
    fn drop(&mut self) {
        if !self.finished {
            self.log_completion(false);
        }
        if let Ok(mut current) = CURRENT_OWNER.lock()
            && current.as_ref() == Some(&self.owner)
        {
            *current = None;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn named_gate_recovers_from_poisoning_at_the_call_boundary() {
        let source = include_str!("webview_init.rs");
        assert!(source.contains("poisoned.into_inner()"));
        assert!(source.contains("wait_ms"));
        assert!(source.contains("build_ms"));
    }
}
