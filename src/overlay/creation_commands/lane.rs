use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};

/// One bounded, ordered lane. Submission never waits for storage or queue space.
pub(crate) struct Lane<T>(SyncSender<T>);

impl<T: Send + 'static> Lane<T> {
    pub(crate) fn new(
        name: &str,
        capacity: usize,
        process: impl Fn(T) + Send + 'static,
    ) -> anyhow::Result<Self> {
        let (sender, receiver) = sync_channel(capacity);
        std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    process(request);
                }
            })?;
        Ok(Self(sender))
    }

    pub(crate) fn submit(&self, request: T) -> Result<(), T> {
        self.0.try_send(request).map_err(|error| match error {
            TrySendError::Full(request) | TrySendError::Disconnected(request) => request,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    #[test]
    fn blocked_work_does_not_block_submission_and_queue_is_bounded_and_ordered() {
        let (started, wait_started) = channel();
        let (release, wait_release) = channel();
        let (completed, wait_completed) = channel();
        let caller = std::thread::current().id();
        let lane = Lane::new("creation-work-test", 2, move |value| {
            assert_ne!(std::thread::current().id(), caller);
            if value == 1 {
                started.send(()).unwrap();
                wait_release.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            completed.send(value).unwrap();
        })
        .unwrap();
        lane.submit(1).unwrap();
        wait_started.recv_timeout(Duration::from_secs(5)).unwrap();
        lane.submit(2).unwrap();
        lane.submit(3).unwrap();
        assert_eq!(lane.submit(4), Err(4));
        release.send(()).unwrap();
        for expected in [1, 2, 3] {
            assert_eq!(
                wait_completed.recv_timeout(Duration::from_secs(5)).unwrap(),
                expected
            );
        }
    }
}
