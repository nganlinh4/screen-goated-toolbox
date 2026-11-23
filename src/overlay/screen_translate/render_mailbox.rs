use std::sync::{Arc, mpsc};

use anyhow::Result;

use crate::overlay::wake_event::WakeEvent;

pub(super) struct Sender<T> {
    sender: Option<mpsc::Sender<T>>,
    wake: Arc<WakeEvent>,
}

pub(super) struct Receiver<T> {
    receiver: mpsc::Receiver<T>,
    pub(super) wake: Arc<WakeEvent>,
    disconnected: bool,
}

pub(super) enum Read<T> {
    Item(T),
    Pending,
    Disconnected,
}

pub(super) fn channel<T>() -> Result<(Sender<T>, Receiver<T>)> {
    let wake = Arc::new(WakeEvent::new()?);
    let (sender, receiver) = mpsc::channel();
    Ok((
        Sender {
            sender: Some(sender),
            wake: Arc::clone(&wake),
        },
        Receiver {
            receiver,
            wake,
            disconnected: false,
        },
    ))
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            wake: Arc::clone(&self.wake),
        }
    }
}

impl<T> Sender<T> {
    pub(super) fn send(&self, value: T) -> Result<(), mpsc::SendError<T>> {
        let result = self.sender.as_ref().expect("live sender").send(value);
        self.wake.signal();
        result
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        // Publish disconnection before waking: the receiver must not observe an
        // empty queue, sleep, then miss the last sender's destruction.
        self.sender.take();
        self.wake.signal();
    }
}

impl<T> Receiver<T> {
    pub(super) fn read(&mut self) -> Result<Read<T>> {
        if !self.disconnected {
            match self.receiver.try_recv() {
                Ok(value) => return Ok(Read::Item(value)),
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.disconnected = true;
                    return Ok(Read::Disconnected);
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        // Return to the owner after every wake so messages, cancellation and
        // retirement are checked before dispatching another queued command.
        self.wake.wait()?;
        Ok(Read::Pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queued_commands_are_not_lost_when_notifications_coalesce() {
        let (sender, mut receiver) = channel().unwrap();
        sender.send(1).unwrap();
        sender.send(2).unwrap();
        assert!(matches!(receiver.read().unwrap(), Read::Item(1)));
        assert!(matches!(receiver.read().unwrap(), Read::Item(2)));
    }

    #[test]
    fn a_disconnected_completed_owner_blocks_until_an_external_retirement_wake() {
        let (sender, mut receiver) = channel::<u32>().unwrap();
        drop(sender);
        assert!(matches!(receiver.read().unwrap(), Read::Disconnected));
        assert!(receiver.wake.signalled_within(0));
        let wake = Arc::clone(&receiver.wake);
        let (started, entered) = mpsc::channel();
        let (done, finished) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            started.send(()).unwrap();
            done.send(matches!(receiver.read().unwrap(), Read::Pending))
                .unwrap();
        });
        entered.recv().unwrap();
        let early = finished.recv_timeout(std::time::Duration::from_millis(40));
        wake.signal();
        thread.join().unwrap();
        assert!(early.is_err());
        assert!(
            finished
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap()
        );
    }

    #[test]
    fn only_the_last_sender_disconnects_and_the_notification_follows_publication() {
        let (sender, mut receiver) = channel::<u32>().unwrap();
        let other = sender.clone();
        drop(sender);
        assert!(receiver.wake.signalled_within(0));
        assert!(matches!(
            receiver.receiver.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        drop(other);
        assert!(receiver.wake.signalled_within(0));
        assert!(matches!(receiver.read().unwrap(), Read::Disconnected));
        assert!(!receiver.wake.signalled_within(0));
    }
}
