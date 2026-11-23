//! A stop is acknowledged only after the native capture object has been dropped.
use std::sync::{Mutex, mpsc};
use std::time::Duration;

#[derive(Default)]
pub(super) struct CaptureLifetime(Mutex<Option<mpsc::Receiver<()>>>);

pub(super) struct Completion(mpsc::Sender<()>);

impl CaptureLifetime {
    pub(super) fn begin(&self) -> Completion {
        let (sender, receiver) = mpsc::channel();
        *self.0.lock().unwrap() = Some(receiver);
        Completion(sender)
    }

    pub(super) fn wait(&self, timeout: Duration) -> bool {
        let Some(receiver) = self.0.lock().unwrap().take() else {
            return true;
        };
        match receiver.recv_timeout(timeout) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => true,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                *self.0.lock().unwrap() = Some(receiver);
                false
            }
        }
    }
}

impl Drop for Completion {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitle_stop_waits_for_native_capture_close() {
        let lifetime = CaptureLifetime::default();
        let native_capture = lifetime.begin();
        assert!(!lifetime.wait(Duration::ZERO));
        drop(native_capture);
        assert!(lifetime.wait(Duration::from_secs(1)));
        assert!(lifetime.wait(Duration::ZERO));
    }
}
