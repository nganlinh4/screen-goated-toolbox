use std::collections::VecDeque;
use std::net::{Shutdown, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak, mpsc};

use super::types::{LiveEvent, QueuedLiveRequest};

#[cfg(test)]
mod tests;

pub(crate) struct RequestControl {
    retired: AtomicBool,
    transport: Mutex<Option<TcpStream>>,
    generation: u64,
    current_generation: Arc<AtomicU64>,
    shutdown: Arc<AtomicBool>,
}

impl RequestControl {
    pub(super) fn new(
        generation: u64,
        current_generation: Arc<AtomicU64>,
        shutdown: Arc<AtomicBool>,
    ) -> Self {
        Self {
            retired: AtomicBool::new(false),
            transport: Mutex::new(None),
            generation,
            current_generation,
            shutdown,
        }
    }

    pub(super) fn generation_is_current(&self) -> bool {
        !self.shutdown.load(Ordering::SeqCst)
            && self.generation == self.current_generation.load(Ordering::SeqCst)
    }

    pub(super) fn is_retired(&self) -> bool {
        self.retired.load(Ordering::SeqCst) || !self.generation_is_current()
    }

    pub(super) fn bind_transport(&self, socket: &TcpStream) -> anyhow::Result<()> {
        let owned = socket.try_clone()?;
        let mut current = self
            .transport
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.is_retired() {
            let _ = owned.shutdown(Shutdown::Both);
            anyhow::bail!("Gemini Live request retired before transport admission");
        }
        if let Some(previous) = current.replace(owned) {
            let _ = previous.shutdown(Shutdown::Both);
        }
        Ok(())
    }

    pub(super) fn retire(&self) {
        self.retired.store(true, Ordering::SeqCst);
        if let Some(socket) = self
            .transport
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            let _ = socket.shutdown(Shutdown::Both);
        }
    }

    pub(super) fn admit(
        &self,
        allowed: impl FnOnce() -> bool,
        effect: impl FnOnce() -> bool,
    ) -> bool {
        let mut transport = self
            .transport
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.is_retired() || !allowed() {
            return false;
        }
        if effect() {
            return true;
        }
        self.retired.store(true, Ordering::SeqCst);
        if let Some(socket) = transport.take() {
            let _ = socket.shutdown(Shutdown::Both);
        }
        false
    }
}

/// The finite response scope owns its queued work and accepted transport.
pub struct LiveResponse {
    pub id: u64,
    pub(super) receiver: mpsc::Receiver<LiveEvent>,
    pub(super) control: Arc<RequestControl>,
    queue: Weak<Mutex<VecDeque<QueuedLiveRequest>>>,
}

impl LiveResponse {
    pub(super) fn new(
        id: u64,
        receiver: mpsc::Receiver<LiveEvent>,
        control: Arc<RequestControl>,
        queue: Weak<Mutex<VecDeque<QueuedLiveRequest>>>,
    ) -> Self {
        Self {
            id,
            receiver,
            control,
            queue,
        }
    }

    pub(super) fn is_retired(&self) -> bool {
        self.control.is_retired()
    }
}

impl Drop for LiveResponse {
    fn drop(&mut self) {
        self.control.retire();
        if let Some(queue) = self.queue.upgrade() {
            queue
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .retain(|request| !Arc::ptr_eq(&request.control, &self.control));
        }
    }
}
