//! A single absolute deadline covering upgrade, pairing reads and pairing writes.
//! Once authenticated, reads are served from the dedicated reader thread.
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use super::inbound::{Inbound, InboundBuffer};

pub(super) struct DeadlineStream {
    stream: TcpStream,
    deadline: Option<Instant>,
    remaining_bytes: usize,
    inbound: Option<InboundBuffer>,
}

impl DeadlineStream {
    pub(super) fn new(stream: TcpStream, deadline: Instant) -> Self {
        Self {
            stream,
            deadline: Some(deadline),
            remaining_bytes: 256 * 1024,
            inbound: None,
        }
    }

    /// Lift the handshake deadline and hand reads to a reader thread that wakes
    /// the pump through `wake`. Bytes then arrive only through [`Self::feed`].
    pub(super) fn authenticated<T>(&mut self, wake: Sender<T>) -> io::Result<()>
    where
        T: From<Inbound> + Send + 'static,
    {
        self.deadline = None;
        self.stream.set_read_timeout(None)?;
        self.stream.set_write_timeout(None)?;
        self.inbound = Some(InboundBuffer::start(&self.stream, wake)?);
        Ok(())
    }

    /// Replace the pre-authentication deadline for the next handshake phase.
    pub(super) fn set_deadline(&mut self, deadline: Instant) {
        if self.inbound.is_none() {
            self.deadline = Some(deadline);
        }
    }

    pub(super) fn feed(&mut self, inbound: Inbound) {
        if let Some(buffer) = &mut self.inbound {
            buffer.feed(inbound);
        }
    }

    pub(super) fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        self.stream.set_write_timeout(timeout)
    }

    fn remaining(&self) -> io::Result<Option<Duration>> {
        match self.deadline {
            None => Ok(None),
            Some(deadline) => deadline
                .checked_duration_since(Instant::now())
                .filter(|duration| !duration.is_zero())
                .map(Some)
                .ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::TimedOut,
                        "browser handshake deadline exceeded",
                    )
                }),
        }
    }
}

impl Read for DeadlineStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(buffer) = &mut self.inbound {
            return buffer.read(buf);
        }
        if let Some(remaining) = self.remaining()? {
            self.stream.set_read_timeout(Some(remaining))?;
            if self.remaining_bytes == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "browser handshake byte limit exceeded",
                ));
            }
            let limit = buf.len().min(self.remaining_bytes);
            let read = self.stream.read(&mut buf[..limit])?;
            self.remaining_bytes -= read;
            return Ok(read);
        }
        self.stream.read(buf)
    }
}

impl Write for DeadlineStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if let Some(remaining) = self.remaining()? {
            self.stream.set_write_timeout(Some(remaining))?;
        }
        self.stream.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.remaining()?;
        self.stream.flush()
    }
}

impl Drop for DeadlineStream {
    fn drop(&mut self) {
        if self.inbound.is_some() {
            // The reader thread blocks on a clone of this socket.
            let _ = self.stream.shutdown(Shutdown::Both);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn trickled_bytes_cannot_extend_the_absolute_upgrade_deadline() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        let writer = std::thread::spawn(move || {
            for _ in 0..100 {
                if client.write_all(b"x").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        });
        let mut stream = DeadlineStream::new(server, Instant::now() + Duration::from_millis(100));
        let mut byte = [0];
        let error = loop {
            match stream.read(&mut byte) {
                Ok(0) => panic!("unexpected EOF"),
                Ok(_) => {}
                Err(error)
                    if error.kind() == io::ErrorKind::TimedOut && stream.remaining().is_ok() => {}
                Err(error) => break error,
            }
        };
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        drop(stream);
        writer.join().unwrap();
    }

    #[test]
    fn preauth_read_budget_is_enforced_without_waiting_for_eof() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        client.write_all(b"abc").unwrap();
        let mut stream = DeadlineStream::new(server, Instant::now() + Duration::from_secs(1));
        stream.remaining_bytes = 2;
        let mut buffer = [0; 3];
        stream.read_exact(&mut buffer[..2]).unwrap();
        assert_eq!(
            stream.read(&mut buffer).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn valid_upgrade_can_pause_longer_than_pump_tick() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        let worker = std::thread::spawn(move || {
            let stream = DeadlineStream::new(server, Instant::now() + Duration::from_secs(2));
            tungstenite::accept(stream).is_ok()
        });
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n")
            .unwrap();
        std::thread::sleep(Duration::from_millis(120));
        client.write_all(b"Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").unwrap();
        assert!(worker.join().unwrap());
    }
}
