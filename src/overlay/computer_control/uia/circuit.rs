//! Bounded per-surface timeout circuits for slow accessibility providers.
//!
//! A surface is the exact window a worker enumerates, so selector aliases and
//! later focus changes cannot bypass its cooldown. Each admitted worker holds
//! its kind's slot until it actually exits: a worker still blocked inside a
//! provider after its caller timed out prevents another worker from piling
//! onto the same surface.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

const COOLDOWN: Duration = Duration::from_secs(30);
const MAX_SURFACES: usize = 256;
const OPTIONAL_SURFACE_LIMIT: usize = MAX_SURFACES / 2;
const MAX_KEY_BYTES: usize = 4096;
static STATE: OnceLock<Mutex<CircuitState>> = OnceLock::new();

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Authoritative,
    Optional,
}

struct Block {
    until: Instant,
    // A unique identity also prevents a late success from clearing a replacement.
    generation: Arc<()>,
}

#[derive(Default)]
struct Surface {
    authoritative: Option<Block>,
    optional: Option<Block>,
    in_flight: usize,
    authoritative_in_flight: bool,
    optional_in_flight: bool,
}

#[derive(Default)]
struct CircuitState {
    surfaces: HashMap<String, Surface>,
}

#[derive(Clone)]
struct Ticket {
    surface: String,
    kind: Kind,
    authoritative: Option<Arc<()>>,
    optional: Option<Arc<()>>,
}

#[derive(Debug)]
pub(super) enum AdmissionError {
    CoolingDown(Duration),
    Capacity,
    InFlight,
}

enum Outcome {
    Success,
    Timeout,
}

fn expire(block: &mut Option<Block>, now: Instant) {
    if block.as_ref().is_some_and(|block| block.until <= now) {
        *block = None;
    }
}

fn same_generation(block: &Option<Block>, observed: &Option<Arc<()>>) -> bool {
    match (block, observed) {
        (Some(block), Some(observed)) => Arc::ptr_eq(&block.generation, observed),
        _ => false,
    }
}

impl CircuitState {
    fn admit(&mut self, key: &str, kind: Kind, now: Instant) -> Result<Ticket, AdmissionError> {
        self.surfaces.retain(|_, surface| {
            expire(&mut surface.authoritative, now);
            expire(&mut surface.optional, now);
            surface.in_flight != 0 || surface.authoritative.is_some() || surface.optional.is_some()
        });
        let limit = match kind {
            Kind::Authoritative => MAX_SURFACES,
            Kind::Optional => OPTIONAL_SURFACE_LIMIT,
        };
        if key.len() > MAX_KEY_BYTES
            || (!self.surfaces.contains_key(key) && self.surfaces.len() >= limit)
        {
            return Err(AdmissionError::Capacity);
        }
        let surface = self.surfaces.entry(key.to_string()).or_default();
        let blocking = surface.authoritative.as_ref().or(match kind {
            Kind::Authoritative => None,
            Kind::Optional => surface.optional.as_ref(),
        });
        if let Some(block) = blocking {
            return Err(AdmissionError::CoolingDown(
                block.until.saturating_duration_since(now),
            ));
        }
        let in_flight = match kind {
            Kind::Authoritative => &mut surface.authoritative_in_flight,
            Kind::Optional => &mut surface.optional_in_flight,
        };
        if *in_flight {
            return Err(AdmissionError::InFlight);
        }
        *in_flight = true;
        surface.in_flight += 1;
        Ok(Ticket {
            surface: key.to_string(),
            kind,
            authoritative: surface
                .authoritative
                .as_ref()
                .map(|block| block.generation.clone()),
            optional: surface
                .optional
                .as_ref()
                .map(|block| block.generation.clone()),
        })
    }

    /// Apply the caller's observed outcome without releasing the worker slot.
    fn record(&mut self, ticket: &Ticket, outcome: Outcome, now: Instant) {
        let surface = match outcome {
            Outcome::Success => self.surfaces.get_mut(&ticket.surface),
            Outcome::Timeout => {
                // A worker that exited just after its caller gave up may already
                // have released the entry; the timeout still earns a cooldown.
                if !self.surfaces.contains_key(&ticket.surface)
                    && self.surfaces.len() >= MAX_SURFACES
                {
                    return;
                }
                Some(self.surfaces.entry(ticket.surface.clone()).or_default())
            }
        };
        let Some(surface) = surface else {
            return;
        };
        match outcome {
            Outcome::Success => {
                if same_generation(&surface.authoritative, &ticket.authoritative) {
                    surface.authoritative = None;
                }
                if same_generation(&surface.optional, &ticket.optional) {
                    surface.optional = None;
                }
            }
            Outcome::Timeout => {
                let block = Some(Block {
                    until: now + COOLDOWN,
                    generation: Arc::new(()),
                });
                match ticket.kind {
                    Kind::Authoritative => surface.authoritative = block,
                    Kind::Optional => surface.optional = block,
                }
            }
        }
        self.remove_if_idle(&ticket.surface);
    }

    /// Release the worker slot once the worker itself has exited.
    fn release(&mut self, ticket: Ticket) {
        let Some(surface) = self.surfaces.get_mut(&ticket.surface) else {
            return;
        };
        match ticket.kind {
            Kind::Authoritative => surface.authoritative_in_flight = false,
            Kind::Optional => surface.optional_in_flight = false,
        }
        surface.in_flight -= 1;
        self.remove_if_idle(&ticket.surface);
    }

    fn remove_if_idle(&mut self, key: &str) {
        if self.surfaces.get(key).is_some_and(|surface| {
            surface.in_flight == 0 && surface.authoritative.is_none() && surface.optional.is_none()
        }) {
            self.surfaces.remove(key);
        }
    }

    #[cfg(test)]
    fn finish(&mut self, ticket: Ticket, outcome: Outcome, now: Instant) {
        self.record(&ticket, outcome, now);
        self.release(ticket);
    }
}

fn state() -> std::sync::MutexGuard<'static, CircuitState> {
    STATE
        .get_or_init(|| Mutex::new(CircuitState::default()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The caller's handle for reporting a worker's observed outcome. Dropping it
/// unreported changes no cooldown.
pub(super) struct Permit(Ticket);

impl Permit {
    pub(super) fn success(self) {
        state().record(&self.0, Outcome::Success, Instant::now());
    }

    pub(super) fn timeout(self) {
        state().record(&self.0, Outcome::Timeout, Instant::now());
    }
}

/// Owned by the worker; its slot is released only when the worker exits.
pub(super) struct WorkerLease(Option<Ticket>);

impl Drop for WorkerLease {
    fn drop(&mut self) {
        if let Some(ticket) = self.0.take() {
            state().release(ticket);
        }
    }
}

pub(super) fn admit(
    surface: &ResolvedSurface,
    kind: Kind,
) -> Result<(Permit, WorkerLease), AdmissionError> {
    state()
        .admit(&surface.key(), kind, Instant::now())
        .map(|ticket| (Permit(ticket.clone()), WorkerLease(Some(ticket))))
}

/// The exact window identity an enumeration will read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ResolvedSurface {
    pub(super) hwnd: usize,
    pid: u32,
}

impl ResolvedSurface {
    pub(super) fn key(&self) -> String {
        format!("window:{}:{}", self.hwnd, self.pid)
    }
}

pub(super) fn resolve_surface(target: Option<&str>) -> anyhow::Result<ResolvedSurface> {
    let hwnd = super::active_window(target)?;
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    Ok(ResolvedSurface {
        hwnd: hwnd.0 as usize,
        pid,
    })
}

#[derive(Debug)]
struct EnumerationTimeout;

impl fmt::Display for EnumerationTimeout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UIA enumeration timed out")
    }
}

impl Error for EnumerationTimeout {}

pub(super) fn timeout_error() -> anyhow::Error {
    anyhow::Error::new(EnumerationTimeout)
}

pub(super) fn is_timeout(error: &anyhow::Error) -> bool {
    error.is::<EnumerationTimeout>()
}

#[cfg(test)]
#[path = "circuit_tests.rs"]
mod tests;
