use crate::admission::Admission;
use sgt_screen_text_detector_protocol::stream::{self, Request};
use std::io::Read;
use std::sync::{Arc, atomic::AtomicBool, mpsc};

pub(crate) enum Incoming {
    Capture {
        id: u64,
        jpeg: Vec<u8>,
        cancelled: Arc<AtomicBool>,
    },
    Stop,
}

/// Read host requests until EOF, a protocol violation, or shutdown. A capture
/// that arrives while busy or with a stale id ends the session.
pub(crate) fn start(
    mut reader: impl Read + Send + 'static,
    admission: Admission,
) -> mpsc::Receiver<Incoming> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        while let Ok((id, request)) = stream::read_request(&mut reader) {
            match request {
                Request::Capture(jpeg) => {
                    let Some(cancelled) = admission.admit(id) else {
                        break;
                    };
                    let capture = Incoming::Capture {
                        id,
                        jpeg,
                        cancelled,
                    };
                    if sender.try_send(capture).is_err() {
                        break;
                    }
                }
                Request::Cancel => admission.cancel(id),
                Request::Shutdown | Request::Hello(_) => break,
            }
        }
        admission.cancel_active();
        let _ = sender.try_send(Incoming::Stop);
    });
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    /// In-memory stdin whose bytes arrive when the test sends them.
    struct ChannelReader {
        chunks: mpsc::Receiver<Vec<u8>>,
        pending: std::collections::VecDeque<u8>,
    }

    impl Read for ChannelReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.pending.is_empty() {
                match self.chunks.recv() {
                    Ok(chunk) => self.pending.extend(chunk),
                    Err(_) => return Ok(0),
                }
            }
            let count = buffer.len().min(self.pending.len());
            for (slot, byte) in buffer.iter_mut().zip(self.pending.drain(..count)) {
                *slot = byte;
            }
            Ok(count)
        }
    }

    struct Host {
        stdin: Option<mpsc::Sender<Vec<u8>>>,
        incoming: mpsc::Receiver<Incoming>,
        admission: Admission,
    }

    impl Host {
        fn ready() -> Self {
            let (admission, _) = Admission::starting(1);
            admission.finish(1);
            let (stdin, chunks) = mpsc::channel();
            let reader = ChannelReader {
                chunks,
                pending: Default::default(),
            };
            let incoming = start(reader, admission.clone());
            Self {
                stdin: Some(stdin),
                incoming,
                admission,
            }
        }

        fn send(&self, id: u64, request: Request) {
            let mut bytes = Vec::new();
            stream::write_request(&mut bytes, id, &request).unwrap();
            self.stdin.as_ref().unwrap().send(bytes).unwrap();
        }

        fn next(&self) -> Incoming {
            self.incoming
                .recv_timeout(Duration::from_secs(5))
                .expect("input thread must answer")
        }

        fn capture(&self) -> (u64, Arc<AtomicBool>) {
            match self.next() {
                Incoming::Capture { id, cancelled, .. } => (id, cancelled),
                Incoming::Stop => panic!("expected a capture"),
            }
        }

        fn expect_stop(&self) {
            assert!(matches!(self.next(), Incoming::Stop));
        }
    }

    #[test]
    fn sequential_captures_are_admitted_after_each_finish() {
        let host = Host::ready();
        for id in [2, 3, 7] {
            host.send(id, Request::Capture(vec![1]));
            let (admitted, cancelled) = host.capture();
            assert_eq!(admitted, id);
            assert!(!cancelled.load(Ordering::Acquire));
            host.admission.finish(id);
        }
    }

    #[test]
    fn cancel_reaches_only_the_active_capture() {
        let host = Host::ready();
        host.send(2, Request::Capture(vec![1]));
        let (_, first) = host.capture();
        host.send(9, Request::Cancel);
        host.admission.finish(2);
        host.send(2, Request::Cancel);
        host.send(3, Request::Capture(vec![1]));
        let (_, second) = host.capture();
        assert!(!first.load(Ordering::Acquire));
        assert!(!second.load(Ordering::Acquire));
        host.send(3, Request::Cancel);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !second.load(Ordering::Acquire) {
            assert!(
                std::time::Instant::now() < deadline,
                "cancel was not delivered"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        host.admission.finish(3);
        host.send(4, Request::Capture(vec![1]));
        let (id, third) = host.capture();
        assert_eq!(id, 4);
        assert!(!third.load(Ordering::Acquire));
    }

    #[test]
    fn busy_or_stale_captures_end_the_session() {
        let host = Host::ready();
        host.send(2, Request::Capture(vec![1]));
        let (_, running) = host.capture();
        host.send(3, Request::Capture(vec![1]));
        host.expect_stop();
        assert!(running.load(Ordering::Acquire));

        let host = Host::ready();
        host.send(1, Request::Capture(vec![1]));
        host.expect_stop();
    }

    #[test]
    fn stdin_eof_cancels_the_active_capture_and_stops() {
        let mut host = Host::ready();
        host.send(2, Request::Capture(vec![1]));
        let (_, running) = host.capture();
        host.stdin.take();
        host.expect_stop();
        assert!(running.load(Ordering::Acquire));
    }
}
