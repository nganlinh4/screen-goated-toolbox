use super::*;

pub(super) fn start_watchdog() {
    WATCHDOG.call_once(|| {
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let live = LIVE_GENERATION.load(Ordering::SeqCst);
                if crate::overlay::compositor_process::watchdog_should_restart(
                    DESIRED.load(Ordering::SeqCst),
                    TRANSITIONING.load(Ordering::SeqCst),
                    live,
                ) {
                    request_restart();
                    continue;
                }
                if live == 0 {
                    continue;
                }
                let timeout = if READY_GENERATION.load(Ordering::SeqCst) == live {
                    HEARTBEAT_TIMEOUT_MS
                } else {
                    15_000
                };
                if now_ms().saturating_sub(LAST_HEARTBEAT_MS.load(Ordering::SeqCst)) > timeout {
                    fail_generation(live, "heartbeat timed out", true);
                }
            }
        });
    });
}

pub(super) fn restart_now() -> ProcessState {
    TRANSITIONING.store(true, Ordering::SeqCst);
    let mut old = PROCESS.lock().unwrap().take();
    if let Some(renderer) = old.as_mut() {
        LIVE_GENERATION
            .compare_exchange(renderer.generation, 0, Ordering::SeqCst, Ordering::SeqCst)
            .ok();
        LIVE_PID.store(0, Ordering::SeqCst);
        READY_GENERATION.store(0, Ordering::SeqCst);
        READY_SINCE_MS.store(0, Ordering::SeqCst);
        let _ = write_to(&mut renderer.stdin, &HostCommand::Shutdown);
        crate::overlay::compositor_process::wait_for_exit_or_kill(&mut renderer.child);
    }
    drop(old);
    let state = ensure_process();
    TRANSITIONING.store(false, Ordering::SeqCst);
    state
}

pub(in crate::overlay::status_compositor) fn shutdown_for_exit() {
    DESIRED.store(false, Ordering::SeqCst);
    TRANSITIONING.store(true, Ordering::SeqCst);
    let mut renderer = PROCESS.lock().unwrap().take();
    LIVE_GENERATION.store(0, Ordering::SeqCst);
    LIVE_PID.store(0, Ordering::SeqCst);
    READY_GENERATION.store(0, Ordering::SeqCst);
    if let Some(renderer) = renderer.as_mut() {
        let _ = write_to(&mut renderer.stdin, &HostCommand::Shutdown);
        crate::overlay::compositor_process::wait_for_exit_or_kill(&mut renderer.child);
    }
}
