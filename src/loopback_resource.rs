use anyhow::{Context, Result, ensure};
use std::io::{self, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const IO_POLL: Duration = Duration::from_millis(100);
const MAX_ACCEPT_ERRORS: u32 = 8;

#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub workers: usize,
    pub queue: usize,
    pub timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            workers: 4,
            queue: 16,
            timeout: Duration::from_secs(5),
        }
    }
}

struct PendingConnection {
    stream: TcpStream,
    deadline: Instant,
}

pub(crate) struct Server {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    acceptor: Option<JoinHandle<()>>,
    workers: Vec<JoinHandle<()>>,
}

impl Server {
    pub(crate) fn start(
        listener: TcpListener,
        limits: Limits,
        handler: impl Fn(&mut Connection) + Send + Sync + 'static,
    ) -> Result<Self> {
        ensure!(
            limits.workers > 0 && limits.queue > 0 && !limits.timeout.is_zero(),
            "Invalid resource-server limits"
        );
        ensure!(
            Instant::now().checked_add(limits.timeout).is_some(),
            "Resource-server deadline is not representable"
        );
        let address = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::sync_channel::<PendingConnection>(limits.queue);
        let receiver = Arc::new(Mutex::new(receiver));
        let handler = Arc::new(handler);
        let mut workers = Vec::new();
        for index in 0..limits.workers {
            let receiver = receiver.clone();
            let handler = handler.clone();
            let stop = stop.clone();
            match thread::Builder::new()
                .name(format!("loopback-resource-{index}"))
                .spawn(move || {
                    loop {
                        let pending = receiver
                            .lock()
                            .ok()
                            .and_then(|receiver| receiver.recv().ok());
                        let Some(pending) = pending else { return };
                        if stop.load(Ordering::Acquire) {
                            return;
                        }
                        if Instant::now() >= pending.deadline {
                            continue;
                        }
                        handler(&mut Connection {
                            stream: pending.stream,
                            deadline: pending.deadline,
                            stop: stop.clone(),
                        });
                    }
                }) {
                Ok(worker) => workers.push(worker),
                Err(error) => {
                    drop(sender);
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(error).context("Start resource-server worker");
                }
            }
        }
        let accept_stop = stop.clone();
        let acceptor = thread::Builder::new()
            .name("loopback-resource-origin".into())
            .spawn(move || {
                accept_loop(
                    || listener.accept().map(|(stream, _)| stream),
                    sender,
                    &accept_stop,
                    limits.timeout,
                );
            });
        match acceptor {
            Ok(acceptor) => Ok(Self {
                address,
                stop,
                acceptor: Some(acceptor),
                workers,
            }),
            Err(error) => {
                // The failed spawn drops the closure and its sender, waking every receiver.
                for worker in workers {
                    let _ = worker.join();
                }
                Err(error).context("Start resource-server listener")
            }
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        !self.stop.load(Ordering::Acquire)
            && self
                .acceptor
                .as_ref()
                .is_some_and(|acceptor| !acceptor.is_finished())
            && self.workers.iter().any(|worker| !worker.is_finished())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(acceptor) = self.acceptor.take() {
            acceptor.thread().unpark();
            let _ = TcpStream::connect_timeout(&self.address, IO_POLL);
            let _ = acceptor.join();
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn accept_loop(
    mut accept: impl FnMut() -> io::Result<TcpStream>,
    sender: mpsc::SyncSender<PendingConnection>,
    stop: &AtomicBool,
    timeout: Duration,
) {
    let mut failures = 0_u32;
    while !stop.load(Ordering::Acquire) {
        match accept() {
            Ok(stream) => {
                failures = 0;
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let Some(deadline) = Instant::now().checked_add(timeout) else {
                    break;
                };
                let pending = PendingConnection { stream, deadline };
                match sender.try_send(pending) {
                    Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
                    Err(mpsc::TrySendError::Disconnected(_)) => break,
                }
            }
            Err(_) => {
                failures += 1;
                if stop.load(Ordering::Acquire) || failures >= MAX_ACCEPT_ERRORS {
                    break;
                }
                thread::park_timeout(Duration::from_millis((10_u64 << (failures - 1)).min(100)));
            }
        }
    }
    stop.store(true, Ordering::Release);
}

/// The deadline starts at accept, covers queueing and all reads/writes, and never renews.
pub(crate) struct Connection {
    stream: TcpStream,
    deadline: Instant,
    stop: Arc<AtomicBool>,
}

impl Connection {
    fn timeout(&self) -> io::Result<Duration> {
        if self.stop.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Resource server stopped",
            ));
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .map(|remaining| remaining.min(IO_POLL))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::TimedOut, "Resource request deadline expired")
            })
    }

    pub(crate) fn local_addr(&self) -> io::Result<SocketAddr> {
        self.stream.local_addr()
    }
    pub(crate) fn shutdown(&self, how: Shutdown) -> io::Result<()> {
        self.stream.shutdown(how)
    }
}

fn retryable(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::Interrupted | io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    )
}

pub(crate) fn read_http_header(stream: &mut Connection, buffer: &mut [u8]) -> io::Result<usize> {
    let mut length = 0;
    loop {
        if let Some(end) = buffer[..length]
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
        {
            return Ok(end + 4);
        }
        if length == buffer.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Resource request header exceeds its limit",
            ));
        }
        let count = stream.read(&mut buffer[length..])?;
        if count == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        length += count;
    }
}

impl Read for Connection {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        loop {
            self.stream.set_read_timeout(Some(self.timeout()?))?;
            match self.stream.read(bytes) {
                Err(error) if retryable(&error) => continue,
                result => return result,
            }
        }
    }
}

impl Write for Connection {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        loop {
            self.stream.set_write_timeout(Some(self.timeout()?))?;
            match self.stream.write(bytes) {
                Err(error) if retryable(&error) => continue,
                result => return result,
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.timeout()?;
        self.stream.flush()
    }
}

#[cfg(test)]
#[path = "loopback_resource/tests.rs"]
mod tests;
