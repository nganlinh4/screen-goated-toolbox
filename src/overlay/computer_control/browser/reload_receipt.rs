//! Correlate a reload with terminal evidence from a later authenticated worker.

use std::sync::{Mutex, OnceLock, mpsc};
use std::time::Instant;

use serde_json::{Value, json};

struct Owner {
    token: String,
    epoch: u64,
    reply: mpsc::Sender<Value>,
}

fn owner() -> &'static Mutex<Option<Owner>> {
    static OWNER: OnceLock<Mutex<Option<Owner>>> = OnceLock::new();
    OWNER.get_or_init(|| Mutex::new(None))
}

pub(super) struct ReloadReceipt {
    token: String,
    receiver: mpsc::Receiver<Value>,
}

impl ReloadReceipt {
    pub(super) fn register(token: String, epoch: u64) -> anyhow::Result<Self> {
        anyhow::ensure!(epoch != 0, "browser extension is not connected");
        let mut slot = owner().lock().unwrap_or_else(|error| error.into_inner());
        anyhow::ensure!(
            slot.is_none(),
            "a browser extension reload is already pending"
        );
        let (reply, receiver) = mpsc::channel();
        *slot = Some(Owner {
            token: token.clone(),
            epoch,
            reply,
        });
        Ok(Self { token, receiver })
    }

    pub(super) fn wait(&self, deadline: Instant) -> anyhow::Result<u64> {
        let cancel = super::readiness::current_cancel();
        let value = super::bridge_wait::receive(&self.receiver, deadline, cancel.as_deref())?;
        value
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("invalid browser reload receipt"))
    }
}

impl Drop for ReloadReceipt {
    fn drop(&mut self) {
        let mut slot = owner().lock().unwrap_or_else(|error| error.into_inner());
        if slot.as_ref().is_some_and(|owner| owner.token == self.token) {
            *slot = None;
        }
    }
}

pub(super) fn record(token: &str, epoch: u64) {
    let slot = owner().lock().unwrap_or_else(|error| error.into_inner());
    if let Some(owner) = slot.as_ref()
        && owner.token == token
        && epoch > owner.epoch
    {
        let _ = owner.reply.send(json!(epoch));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn only_exact_token_from_a_later_connection_can_complete_reload() {
        let receipt = ReloadReceipt::register("owned".to_string(), 7).unwrap();
        record("other", 8);
        record("owned", 7);
        assert!(receipt.receiver.try_recv().is_err());
        record("owned", 8);
        assert_eq!(
            receipt
                .wait(Instant::now() + Duration::from_secs(1))
                .unwrap(),
            8
        );
    }
}
