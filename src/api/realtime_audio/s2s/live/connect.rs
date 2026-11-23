use std::sync::mpsc;
use std::time::Duration;

use anyhow::Result;

/// Let a retiring run release capture while an OS connection attempt finishes.
pub(super) fn wait_for_connect<T: Send + 'static>(
    connect: impl FnOnce() -> Result<T> + Send + 'static,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<Option<T>> {
    if cancelled() {
        return Ok(None);
    }
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("sgt-s2s-connect".into())
        .spawn(move || {
            let _ = tx.send(connect());
        })?;
    loop {
        if cancelled() {
            return Ok(None);
        }
        match rx.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => return result.map(Some),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                anyhow::bail!("S2S connection worker stopped unexpectedly")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn cancellation_releases_wait_and_drops_late_connection() {
        struct Connection(mpsc::Sender<()>);
        impl Drop for Connection {
            fn drop(&mut self) {
                let _ = self.0.send(());
            }
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (dropped_tx, dropped_rx) = mpsc::channel();
        let (capture_tx, capture_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let flag = cancelled.clone();
        let waiter = std::thread::spawn(move || {
            let capture = Connection(capture_tx);
            let result = wait_for_connect(
                move || {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(Connection(dropped_tx))
                },
                &mut || flag.load(Ordering::SeqCst),
            );
            drop(capture);
            done_tx.send(result.unwrap().is_none()).unwrap();
        });
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        cancelled.store(true, Ordering::SeqCst);
        assert!(done_rx.recv_timeout(Duration::from_secs(1)).unwrap());
        capture_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        release_tx.send(()).unwrap();
        dropped_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        waiter.join().unwrap();
    }
}
