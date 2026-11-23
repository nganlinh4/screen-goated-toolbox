use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Result;
use parking_lot::Mutex;
use serde_json::Value;

use super::super::client_protocol::{ClientLifecycleKind, ClientLifecycleSignal};
use crate::process_job::OwnedProcess;

pub(super) struct ServerProcess {
    owner: Arc<Mutex<OwnedProcess>>,
    alive: Arc<AtomicBool>,
    watcher: Mutex<Option<JoinHandle<()>>>,
}

impl ServerProcess {
    pub(super) fn new(
        owner: OwnedProcess,
        alive: Arc<AtomicBool>,
        pending: Arc<Mutex<HashMap<u64, Sender<Value>>>>,
        lifecycle: Option<ClientLifecycleSignal>,
    ) -> Result<Self> {
        let owner = Arc::new(Mutex::new(owner));
        let process = owner.clone();
        let running = alive.clone();
        let watcher = std::thread::Builder::new()
            .name("sgt-mcp-process".into())
            .spawn(move || {
                loop {
                    let exited = !matches!(process.lock().try_wait(), Ok(None));
                    if exited || !running.load(Ordering::Acquire) {
                        running.store(false, Ordering::Release);
                        pending.lock().clear();
                        if let Some(signal) = &lifecycle {
                            signal.raise(ClientLifecycleKind::Disconnected);
                        }
                        if let Err(error) = process.lock().stop_within(Duration::from_millis(500)) {
                            eprintln!("[MCP] process-tree cleanup pending: {error}");
                        }
                        pending.lock().clear();
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            })?;
        Ok(Self {
            owner,
            alive,
            watcher: Mutex::new(Some(watcher)),
        })
    }

    pub(super) fn resume(&self) -> Result<()> {
        self.owner.lock().resume()
    }

    pub(super) fn stop(&self) {
        self.alive.store(false, Ordering::Release);
        if let Some(mut owner) = self.owner.try_lock_for(Duration::from_millis(50))
            && let Err(error) = owner.stop_within(Duration::from_millis(500))
        {
            eprintln!("[MCP] process-tree cleanup pending: {error}");
        }
        let mut watcher = self.watcher.lock();
        if watcher.as_ref().is_some_and(JoinHandle::is_finished)
            && let Some(watcher) = watcher.take()
        {
            let _ = watcher.join();
        }
    }

    #[cfg(test)]
    pub(super) fn control(&self) -> crate::process_job::JobControl {
        self.owner.lock().control()
    }

    #[cfg(test)]
    pub(super) fn stopped(&self) -> bool {
        self.owner.lock().stopped()
    }
}

impl Drop for ServerProcess {
    fn drop(&mut self) {
        self.stop();
    }
}
