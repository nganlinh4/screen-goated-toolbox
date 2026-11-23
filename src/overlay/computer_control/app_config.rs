//! Computer Control reads the in-memory application configuration that
//! `load_config` produced once at startup. Runtime paths never reload or
//! migrate the file from disk, so unsaved in-memory settings apply and an
//! unreadable or corrupt file cannot affect a running session.

use crate::config::Config;

/// Read under the shared state lock. Keep `read` short and free of I/O.
pub(super) fn read<T>(read: impl FnOnce(&Config) -> T) -> T {
    let app = crate::APP
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    read(&app.config)
}

/// An owned copy for work that must not hold the lock, such as provider calls.
pub(super) fn snapshot() -> Config {
    read(Config::clone)
}
