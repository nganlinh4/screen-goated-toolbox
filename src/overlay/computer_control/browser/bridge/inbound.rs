//! Authenticated-connection reads on a dedicated thread. Inbound bytes arrive on
//! the same channel as outbound requests, so the pump wakes for whichever comes
//! first instead of polling the socket between timed reads.
use std::collections::VecDeque;
use std::io::{self, Read};
use std::net::TcpStream;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};

const CHUNK_BYTES: usize = 64 * 1024;
/// Chunks read ahead of the pump. Keeps TCP backpressure on a slow consumer.
const MAX_UNCONSUMED_CHUNKS: usize = 16;

pub(super) enum Inbound {
    Bytes(Vec<u8>),
    /// The peer closed (`None`) or the socket failed.
    End(Option<io::Error>),
}

#[derive(Default)]
struct Credits {
    /// (chunks read but not yet consumed, buffer dropped)
    state: Mutex<(usize, bool)>,
    changed: Condvar,
}

impl Credits {
    fn acquire(&self) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        while state.0 >= MAX_UNCONSUMED_CHUNKS && !state.1 {
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        state.0 += 1;
        !state.1
    }

    fn release(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.0 = state.0.saturating_sub(1);
        self.changed.notify_all();
    }

    fn close(&self) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).1 = true;
        self.changed.notify_all();
    }
}

/// Bytes delivered by the reader thread, served to the WebSocket without blocking.
pub(super) struct InboundBuffer {
    pending: VecDeque<u8>,
    end: Option<Option<io::Error>>,
    credits: Arc<Credits>,
}

impl InboundBuffer {
    pub(super) fn start<T>(stream: &TcpStream, wake: Sender<T>) -> io::Result<Self>
    where
        T: From<Inbound> + Send + 'static,
    {
        let mut reader = stream.try_clone()?;
        reader.set_read_timeout(None)?;
        let credits = Arc::new(Credits::default());
        let thread_credits = credits.clone();
        std::thread::Builder::new()
            .name("sgt-browser-read".into())
            .spawn(move || {
                while thread_credits.acquire() {
                    let mut chunk = vec![0; CHUNK_BYTES];
                    let inbound = match reader.read(&mut chunk) {
                        Ok(0) => Inbound::End(None),
                        Ok(read) => {
                            chunk.truncate(read);
                            Inbound::Bytes(chunk)
                        }
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                            thread_credits.release();
                            continue;
                        }
                        Err(error) => Inbound::End(Some(error)),
                    };
                    let ended = matches!(inbound, Inbound::End(_));
                    if wake.send(inbound.into()).is_err() || ended {
                        break;
                    }
                }
            })?;
        Ok(Self {
            pending: VecDeque::new(),
            end: None,
            credits,
        })
    }

    pub(super) fn feed(&mut self, inbound: Inbound) {
        match inbound {
            Inbound::Bytes(bytes) => {
                self.pending.extend(bytes);
                self.credits.release();
            }
            Inbound::End(error) => self.end = Some(error),
        }
    }
}

impl Read for InboundBuffer {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.pending.is_empty() {
            let count = buf.len().min(self.pending.len());
            for (slot, byte) in buf.iter_mut().zip(self.pending.drain(..count)) {
                *slot = byte;
            }
            return Ok(count);
        }
        match self.end.take() {
            None => Err(io::ErrorKind::WouldBlock.into()),
            Some(error) => {
                self.end = Some(None);
                error.map_or(Ok(0), Err)
            }
        }
    }
}

impl Drop for InboundBuffer {
    fn drop(&mut self) {
        self.credits.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Duration;

    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        (listener.accept().unwrap().0, client)
    }

    #[test]
    fn empty_buffer_never_blocks_and_delivers_bytes_then_end() {
        let (server, mut client) = pair();
        let (tx, rx) = mpsc::channel::<Inbound>();
        let mut buffer = InboundBuffer::start(&server, tx).unwrap();
        let mut out = [0; 8];
        assert_eq!(
            buffer.read(&mut out).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
        client.write_all(b"abc").unwrap();
        buffer.feed(rx.recv_timeout(Duration::from_secs(2)).unwrap());
        assert_eq!(buffer.read(&mut out).unwrap(), 3);
        assert_eq!(&out[..3], b"abc");
        drop(client);
        buffer.feed(rx.recv_timeout(Duration::from_secs(2)).unwrap());
        assert_eq!(buffer.read(&mut out).unwrap(), 0);
        assert_eq!(buffer.read(&mut out).unwrap(), 0);
    }

    #[test]
    fn reader_stops_after_the_unconsumed_chunk_limit() {
        let credits = Credits::default();
        for _ in 0..MAX_UNCONSUMED_CHUNKS {
            assert!(credits.acquire());
        }
        let credits = Arc::new(credits);
        let waiting = credits.clone();
        let reader = std::thread::spawn(move || waiting.acquire());
        std::thread::sleep(Duration::from_millis(30));
        assert!(!reader.is_finished(), "a full read-ahead must wait");
        credits.release();
        assert!(reader.join().unwrap());
        let waiting = credits.clone();
        let reader = std::thread::spawn(move || waiting.acquire());
        credits.close();
        assert!(
            !reader.join().unwrap(),
            "a dropped buffer releases the reader"
        );
    }
}
