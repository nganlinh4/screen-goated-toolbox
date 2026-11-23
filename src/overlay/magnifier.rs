//! Process-wide Windows Magnification API shared by every overlay that zooms
//! the screen. Users hold reference-counted leases: the runtime initializes
//! for the first lease and uninitializes after the last one, so one overlay
//! ending its session can never silently disable another overlay's zoom.

use std::sync::{Mutex, OnceLock};

use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_core::BOOL;

type InitializeFn = unsafe extern "system" fn() -> BOOL;
type UninitializeFn = unsafe extern "system" fn() -> BOOL;
type SetFullscreenTransformFn = unsafe extern "system" fn(f32, i32, i32) -> BOOL;

struct Api {
    initialize: InitializeFn,
    uninitialize: UninitializeFn,
    set_fullscreen_transform: SetFullscreenTransformFn,
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(|| unsafe {
        let library = LoadLibraryW(windows::core::w!("Magnification.dll")).ok()?;
        let initialize = GetProcAddress(library, windows::core::s!("MagInitialize"))?;
        let uninitialize = GetProcAddress(library, windows::core::s!("MagUninitialize"))?;
        let transform = GetProcAddress(library, windows::core::s!("MagSetFullscreenTransform"))?;
        Some(Api {
            initialize: std::mem::transmute::<unsafe extern "system" fn() -> isize, InitializeFn>(
                initialize,
            ),
            uninitialize: std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                UninitializeFn,
            >(uninitialize),
            set_fullscreen_transform: std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                SetFullscreenTransformFn,
            >(transform),
        })
    })
    .as_ref()
}

/// Lease bookkeeping, independent of the native API so it can be verified.
#[derive(Default)]
struct Leases {
    count: usize,
}

impl Leases {
    fn acquire(&mut self, initialize: impl FnOnce() -> bool) -> bool {
        if self.count == 0 && !initialize() {
            return false;
        }
        self.count += 1;
        true
    }

    fn release(&mut self, uninitialize: impl FnOnce()) {
        if self.count == 0 {
            return;
        }
        self.count -= 1;
        if self.count == 0 {
            uninitialize();
        }
    }
}

static LEASES: Mutex<Leases> = Mutex::new(Leases { count: 0 });

fn with_leases<T>(effect: impl FnOnce(&mut Leases) -> T) -> T {
    effect(&mut LEASES.lock().unwrap_or_else(|error| error.into_inner()))
}

/// Takes a lease, initializing the magnifier for the first one. Returns false
/// when the API is unavailable; the caller then holds no lease.
pub(crate) fn acquire() -> bool {
    let Some(api) = api() else {
        return false;
    };
    with_leases(|leases| leases.acquire(|| unsafe { (api.initialize)().as_bool() }))
}

/// Returns one lease; the last release resets the view and uninitializes.
pub(crate) fn release() {
    let Some(api) = api() else {
        return;
    };
    with_leases(|leases| {
        leases.release(|| unsafe {
            let _ = (api.set_fullscreen_transform)(1.0, 0, 0);
            let _ = (api.uninitialize)();
        })
    });
}

/// Applies a full-screen zoom while at least one lease is held.
pub(crate) fn set_fullscreen_transform(level: f32, x: i32, y: i32) {
    let Some(api) = api() else {
        return;
    };
    with_leases(|leases| {
        if leases.count > 0 {
            unsafe {
                let _ = (api.set_fullscreen_transform)(level, x, y);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::Leases;
    use std::cell::Cell;

    #[test]
    fn one_overlay_ending_cannot_uninitialize_another() {
        let initialized = Cell::new(0);
        let uninitialized = Cell::new(0);
        let mut leases = Leases::default();
        let init = || {
            initialized.set(initialized.get() + 1);
            true
        };
        assert!(leases.acquire(init));
        assert!(leases.acquire(|| unreachable!("already initialized")));
        leases.release(|| uninitialized.set(uninitialized.get() + 1));
        assert_eq!(uninitialized.get(), 0);
        leases.release(|| uninitialized.set(uninitialized.get() + 1));
        assert_eq!(uninitialized.get(), 1);

        // A later session initializes again instead of trusting a stale flag.
        assert!(leases.acquire(|| {
            initialized.set(initialized.get() + 1);
            true
        }));
        assert_eq!(initialized.get(), 2);
    }

    #[test]
    fn failed_initialization_holds_no_lease() {
        let mut leases = Leases::default();
        assert!(!leases.acquire(|| false));
        leases.release(|| unreachable!("no lease is held"));
        assert_eq!(leases.count, 0);
    }
}
