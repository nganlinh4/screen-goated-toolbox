use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::HWND;

pub(crate) static REALTIME_STATE: LazyLock<
    std::sync::Arc<Mutex<crate::api::realtime_audio::RealtimeState>>,
> = LazyLock::new(|| {
    std::sync::Arc::new(Mutex::new(
        crate::api::realtime_audio::RealtimeState::default(),
    ))
});
pub(crate) static mut REALTIME_HWND: HWND = HWND(std::ptr::null_mut());
