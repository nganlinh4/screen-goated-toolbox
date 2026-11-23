use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::core::Interface;

const WAIT_TIMEOUT: i32 = 0x102;
const WAIT_ABANDONED: i32 = 0x80;
const WAIT_SLICE_MS: u32 = 100;
const MAX_WAIT: Duration = Duration::from_secs(30);

/// Owns a keyed shared surface until released. A GPU submission can quarantine
/// the acquisition on failure, preventing premature reuse of an in-flight slot.
pub struct KeyedMutexGuard<'a> {
    mutex: &'a IDXGIKeyedMutex,
    key: u64,
    release_on_drop: bool,
}

impl<'a> KeyedMutexGuard<'a> {
    pub fn acquire(
        mutex: &'a IDXGIKeyedMutex,
        key: u64,
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        wait_for_acquisition(cancel, MAX_WAIT, || unsafe {
            // The generated Result<()> method treats positive WAIT_TIMEOUT and
            // WAIT_ABANDONED as success. Inspect the raw HRESULT instead.
            (Interface::vtable(mutex).AcquireSync)(Interface::as_raw(mutex), key, WAIT_SLICE_MS).0
        })?;
        let guard = Self {
            mutex,
            key,
            release_on_drop: true,
        };
        if cancel.load(Ordering::Acquire) {
            return Err("GPU operation cancelled while acquiring a shared surface".to_string());
        }
        Ok(guard)
    }

    pub fn quarantine(&mut self) {
        self.release_on_drop = false;
    }

    pub fn release_after_completion(mut self) -> Result<(), String> {
        self.release_on_drop = false;
        unsafe { self.mutex.ReleaseSync(self.key) }
            .map_err(|error| format!("Could not release shared GPU surface: {error}"))
    }
}

impl Drop for KeyedMutexGuard<'_> {
    fn drop(&mut self) {
        if self.release_on_drop {
            unsafe {
                let _ = self.mutex.ReleaseSync(self.key);
            }
        }
    }
}

fn wait_for_acquisition(
    cancel: &AtomicBool,
    timeout: Duration,
    mut acquire: impl FnMut() -> i32,
) -> Result<(), String> {
    let start = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) {
            return Err("GPU operation cancelled while waiting for a shared surface".to_string());
        }
        match acquire() {
            0 => return Ok(()),
            WAIT_TIMEOUT if start.elapsed() < timeout => continue,
            WAIT_TIMEOUT => return Err("Timed out waiting for a shared GPU surface".to_string()),
            WAIT_ABANDONED => {
                return Err("Shared GPU surface was abandoned and cannot be reused".to_string());
            }
            code => {
                return Err(format!(
                    "Shared GPU surface acquisition failed (0x{:08X})",
                    code as u32
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_wait_codes_never_count_as_ownership() {
        let cancel = AtomicBool::new(false);
        assert!(wait_for_acquisition(&cancel, Duration::ZERO, || WAIT_TIMEOUT).is_err());
        assert!(wait_for_acquisition(&cancel, Duration::from_secs(1), || WAIT_ABANDONED).is_err());
        let mut responses = [WAIT_TIMEOUT, 0].into_iter();
        assert!(
            wait_for_acquisition(&cancel, Duration::from_secs(1), || responses
                .next()
                .unwrap())
            .is_ok()
        );
        cancel.store(true, Ordering::Release);
        assert!(
            wait_for_acquisition(&cancel, Duration::from_secs(1), || panic!(
                "cancelled wait dispatched"
            ))
            .is_err()
        );
    }

    #[test]
    #[ignore = "requires a D3D11 shared keyed texture"]
    fn real_mutex_wait_is_cancellable_and_quarantine_keeps_ownership() {
        use super::super::{SharedVramBuffer, create_d3d11_device};
        use windows::Win32::Graphics::Direct3D11::{ID3D11Device1, ID3D11Texture2D};
        let (device, _) = create_d3d11_device().expect("D3D11 device");
        let texture = SharedVramBuffer::new(&device, 4, 4, true).expect("shared texture");
        let mutex: IDXGIKeyedMutex = texture.texture.cast().expect("keyed mutex");
        let (other_device, _) = create_d3d11_device().expect("second D3D11 device");
        let other_device: ID3D11Device1 = other_device.cast().expect("D3D11 device1");
        let other_texture: ID3D11Texture2D =
            unsafe { other_device.OpenSharedResource1(texture.handle) }
                .expect("open shared texture");
        let other_mutex: IDXGIKeyedMutex = other_texture.cast().expect("second keyed mutex");
        let cancel = AtomicBool::new(false);
        let mut owner = KeyedMutexGuard::acquire(&mutex, 0, &cancel).unwrap();
        std::thread::scope(|scope| {
            let waiting =
                scope.spawn(|| KeyedMutexGuard::acquire(&other_mutex, 0, &cancel).map(|_| ()));
            std::thread::sleep(Duration::from_millis(250));
            assert!(!waiting.is_finished(), "other device must still be waiting");
            cancel.store(true, Ordering::Release);
            assert!(waiting.join().unwrap().unwrap_err().contains("cancel"));
        });
        owner.quarantine();
        drop(owner);
        let status = unsafe {
            (Interface::vtable(&other_mutex).AcquireSync)(Interface::as_raw(&other_mutex), 0, 0).0
        };
        assert_eq!(
            status, WAIT_TIMEOUT,
            "unproven GPU work must retain keyed ownership"
        );
        // No GPU work was submitted in this test, so releasing the key is safe.
        unsafe {
            mutex.ReleaseSync(0).unwrap();
        }
    }
}
