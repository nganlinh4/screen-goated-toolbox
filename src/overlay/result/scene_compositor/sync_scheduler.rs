use super::parent::sync_window;
use std::collections::HashSet;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, IsWindowVisible};

const MIN_SYNC_INTERVAL: Duration = Duration::from_millis(8);

static PENDING: LazyLock<Mutex<HashSet<isize>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
static SIGNAL: LazyLock<SyncSender<()>> = LazyLock::new(|| {
    let (sender, receiver) = sync_channel(1);
    std::thread::Builder::new()
        .name("sgt-result-sync".to_string())
        .spawn(move || {
            let mut last_sync = Instant::now() - MIN_SYNC_INTERVAL;
            while receiver.recv().is_ok() {
                let remaining = MIN_SYNC_INTERVAL.saturating_sub(last_sync.elapsed());
                if !remaining.is_zero() {
                    std::thread::sleep(remaining);
                }
                while receiver.try_recv().is_ok() {}
                let windows: Vec<_> = PENDING.lock().unwrap().drain().collect();
                for id in windows {
                    let hwnd = HWND(id as *mut std::ffi::c_void);
                    if unsafe { IsWindow(Some(hwnd)).as_bool() } {
                        let visible = unsafe { IsWindowVisible(hwnd).as_bool() };
                        sync_window(hwnd, visible);
                    }
                }
                last_sync = Instant::now();
            }
        })
        .expect("failed to start result sync thread");
    sender
});

pub(crate) fn queue_window_sync(hwnd: HWND) {
    PENDING.lock().unwrap().insert(hwnd.0 as isize);
    let _ = SIGNAL.try_send(());
}

#[cfg(test)]
mod tests {
    use super::MIN_SYNC_INTERVAL;

    #[test]
    fn coalescing_stays_within_one_high_refresh_frame() {
        assert_eq!(MIN_SYNC_INTERVAL.as_millis(), 8);
    }
}
