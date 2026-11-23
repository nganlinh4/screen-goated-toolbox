use windows::Win32::Foundation::HWND;

pub(super) const MAX_REQUEST_BYTES: usize = 64 * 1024;

pub(super) fn register(hwnd: HWND) {
    crate::overlay::creation_commands::register(
        hwnd,
        super::WM_APP_IPC_REPLY,
        MAX_REQUEST_BYTES,
        super::ipc::dispatch,
    );
}

pub(super) fn unregister(hwnd: HWND) {
    crate::overlay::creation_commands::unregister(hwnd);
}

pub(super) fn request(
    hwnd: HWND,
    envelope: super::ipc::IpcEnvelope,
    bytes: usize,
) -> Result<(), crate::overlay::creation_error::CreationError> {
    crate::overlay::creation_commands::request(hwnd, envelope, bytes)
}

pub(super) fn take_replies(hwnd: HWND) -> Vec<String> {
    crate::overlay::creation_commands::take_replies(hwnd)
}

#[cfg(test)]
mod tests;
