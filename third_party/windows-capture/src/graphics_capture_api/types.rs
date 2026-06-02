use std::sync::atomic::{self, AtomicBool};
use std::sync::Arc;

use crate::d3d11;

#[derive(thiserror::Error, Eq, PartialEq, Clone, Debug)]
pub enum Error {
    #[error("The Graphics Capture API is not supported on this platform.")]
    Unsupported,
    #[error(
        "Toggling cursor capture is not supported by the Graphics Capture API on this platform."
    )]
    CursorConfigUnsupported,
    #[error(
        "Toggling the capture border is not supported by the Graphics Capture API on this platform."
    )]
    BorderConfigUnsupported,
    #[error(
        "Capturing secondary windows is not supported by the Graphics Capture API on this platform."
    )]
    SecondaryWindowsUnsupported,
    #[error(
        "Setting a minimum update interval is not supported by the Graphics Capture API on this platform."
    )]
    MinimumUpdateIntervalUnsupported,
    #[error(
        "Dirty region tracking is not supported by the Graphics Capture API on this platform."
    )]
    DirtyRegionUnsupported,
    #[error("The capture has already been started.")]
    AlreadyStarted,
    #[error("DirectX error: {0}")]
    DirectXError(#[from] d3d11::Error),
    #[error("Window error: {0}")]
    WindowError(#[from] crate::window::Error),
    #[error("Windows API error: {0}")]
    WindowsError(#[from] windows::core::Error),
}

#[derive(Clone)]
pub struct InternalCaptureControl {
    stop: Arc<AtomicBool>,
}

impl InternalCaptureControl {
    #[must_use]
    #[inline]
    pub const fn new(stop: Arc<AtomicBool>) -> Self {
        Self { stop }
    }

    #[inline]
    pub fn stop(self) {
        self.stop.store(true, atomic::Ordering::Relaxed);
    }
}
