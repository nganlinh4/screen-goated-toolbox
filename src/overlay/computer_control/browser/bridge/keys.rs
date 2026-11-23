//! The install's pairing keys, loaded lazily and retried after a failure so a
//! transient storage problem at startup does not disable pairing until restart.
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::secret;

pub(super) struct PairingKeys {
    /// Per-install bootstrap key, also stamped into the extension's own files
    /// (`bootstrap.js`). Both peers prove knowledge before deriving the reconnect key.
    pub bootstrap: String,
    /// Durable reconnect key independently derived by both peers at pairing.
    pub durable: String,
}

pub(super) struct KeyStore {
    path: PathBuf,
    loaded: Mutex<Option<Arc<PairingKeys>>>,
}

impl KeyStore {
    pub(super) fn new(path: PathBuf) -> Self {
        Self {
            path,
            loaded: Mutex::new(None),
        }
    }

    /// The committed keys. Until one load succeeds, every call retries it.
    pub(super) fn current(&self) -> Result<Arc<PairingKeys>, secret::Error> {
        let mut loaded = self.loaded.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(keys) = loaded.as_ref() {
            return Ok(keys.clone());
        }
        let bootstrap = secret::load_or_make(&self.path)?;
        let keys = Arc::new(PairingKeys {
            durable: super::super::bridge_auth::durable_key(&bootstrap),
            bootstrap,
        });
        *loaded = Some(keys.clone());
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_first_load_is_retried_and_success_is_stable() {
        let name = super::super::super::crypto::random_hex(16).unwrap();
        let root = std::env::temp_dir().join(format!("sgt-pairing-retry-{name}"));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("key");
        std::fs::create_dir(&path).unwrap();
        let store = KeyStore::new(path.clone());
        assert!(store.current().is_err(), "a directory is not a key");
        std::fs::remove_dir(&path).unwrap();
        let keys = store.current().unwrap();
        assert_eq!(
            keys.durable,
            super::super::super::bridge_auth::durable_key(&keys.bootstrap)
        );
        std::fs::remove_file(&path).unwrap();
        assert!(
            Arc::ptr_eq(&keys, &store.current().unwrap()),
            "committed keys are never reloaded or rotated"
        );
        std::fs::remove_dir(&root).unwrap();
    }
}
