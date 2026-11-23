//! Bounded DNS and TCP connection attempts for the Live transport.

use std::collections::{HashSet, VecDeque};
use std::io;
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// DNS and TCP share this budget within the caller's overall deadline.
pub(super) const TOTAL_BUDGET: Duration = Duration::from_secs(10);
const PER_ADDRESS_LIMIT: Duration = Duration::from_secs(2);
const MAX_ADDRESSES: usize = 32;
const MAX_RESOLVED: usize = 128;
const MAX_RESOLVERS: usize = 2;
static RESOLVERS: AtomicUsize = AtomicUsize::new(0);

pub(super) fn connect_until(
    host: &str,
    port: u16,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
) -> io::Result<TcpStream> {
    let host = host.to_owned();
    let addresses = resolve_with_cancel(
        &RESOLVERS,
        deadline.saturating_duration_since(Instant::now()),
        cancelled,
        move || {
            Ok((host.as_str(), port)
                .to_socket_addrs()?
                .take(MAX_RESOLVED)
                .collect())
        },
    )?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    connect_with_cancel(addresses, remaining, cancelled, TcpStream::connect_timeout)
}

struct ResolverPermit(&'static AtomicUsize);

impl Drop for ResolverPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
fn resolve_with(
    active: &'static AtomicUsize,
    budget: Duration,
    resolve: impl FnOnce() -> io::Result<Vec<SocketAddr>> + Send + 'static,
) -> io::Result<Vec<SocketAddr>> {
    resolve_with_cancel(active, budget, &|| false, resolve)
}

fn resolve_with_cancel(
    active: &'static AtomicUsize,
    budget: Duration,
    cancelled: &impl Fn() -> bool,
    resolve: impl FnOnce() -> io::Result<Vec<SocketAddr>> + Send + 'static,
) -> io::Result<Vec<SocketAddr>> {
    let started = Instant::now();
    if cancelled() {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "DNS resolution cancelled",
        ));
    }
    if budget.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "Live DNS resolution timed out",
        ));
    }
    active
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
            (count < MAX_RESOLVERS).then(|| count + 1)
        })
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                "Live DNS resolver capacity is busy",
            )
        })?;
    let permit = ResolverPermit(active);
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("gemini-live-dns".into())
        .spawn(move || {
            let _permit = permit;
            let _ = sender.send(resolve());
        })?;
    loop {
        if cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "DNS resolution cancelled",
            ));
        }
        let remaining = budget.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Live DNS resolution timed out",
            ));
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(result) => return result,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(io::Error::other("Live DNS resolver stopped"));
            }
        }
    }
}

fn interleave(addresses: Vec<SocketAddr>) -> Vec<SocketAddr> {
    let mut seen = HashSet::new();
    let mut v4 = VecDeque::new();
    let mut v6 = VecDeque::new();
    let mut first_v6 = None;
    for address in addresses {
        if !seen.insert(address) {
            continue;
        }
        first_v6.get_or_insert(address.is_ipv6());
        if address.is_ipv6() {
            v6.push_back(address);
        } else {
            v4.push_back(address);
        }
    }
    let mut prefer_v6 = first_v6.unwrap_or(false);
    let mut result = Vec::new();
    while result.len() < MAX_ADDRESSES {
        let address = if prefer_v6 {
            v6.pop_front().or_else(|| v4.pop_front())
        } else {
            v4.pop_front().or_else(|| v6.pop_front())
        };
        let Some(address) = address else { break };
        result.push(address);
        prefer_v6 = !prefer_v6;
    }
    result
}

#[cfg(test)]
fn connect_with<T>(
    addresses: Vec<SocketAddr>,
    budget: Duration,
    connect: impl FnMut(&SocketAddr, Duration) -> io::Result<T>,
) -> io::Result<T> {
    connect_with_cancel(addresses, budget, &|| false, connect)
}

fn connect_with_cancel<T>(
    addresses: Vec<SocketAddr>,
    budget: Duration,
    cancelled: &impl Fn() -> bool,
    mut connect: impl FnMut(&SocketAddr, Duration) -> io::Result<T>,
) -> io::Result<T> {
    let addresses = interleave(addresses);
    if addresses.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "Live DNS returned no addresses",
        ));
    }
    let started = Instant::now();
    let mut last_error = None;
    for (index, address) in addresses.iter().enumerate() {
        if cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "TCP connection cancelled",
            ));
        }
        let remaining = budget.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Live TCP connection timed out",
            ));
        }
        let slice = (remaining / (addresses.len() - index) as u32).min(PER_ADDRESS_LIMIT);
        if slice.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Live TCP connection timed out",
            ));
        }
        match connect(address, slice) {
            Ok(_) if cancelled() => {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "TCP connection cancelled",
                ));
            }
            Ok(_) if started.elapsed() >= budget => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Live TCP connection timed out",
                ));
            }
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "Live DNS returned no addresses",
        )
    }))
}

#[cfg(test)]
#[path = "connect_tests.rs"]
mod tests;
