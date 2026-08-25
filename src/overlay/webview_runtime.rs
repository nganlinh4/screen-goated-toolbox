//! Canonical WebView2 environment and surface-lifecycle contract.
//!
//! WRY contexts remain on the COM apartment that owns their window. Compatible
//! surfaces share a user-data folder (and therefore WebView2 browser processes),
//! while compositor processes keep explicit recovery boundaries.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use wry::WebContext;

static CONTEXTS_CREATED: AtomicU64 = AtomicU64::new(0);
static CONTEXTS_ACTIVE: AtomicUsize = AtomicUsize::new(0);
static CONTEXTS_FAILED: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Profile {
    Common,
    #[cfg(not(feature = "recorder-worker"))]
    CreationDebug,
    #[cfg(not(feature = "recorder-worker"))]
    ResultNavigation,
    #[cfg(not(feature = "recorder-worker"))]
    ResultCompositor,
    #[cfg(not(feature = "recorder-worker"))]
    StatusCompositor,
    #[cfg(not(feature = "recorder-worker"))]
    RealtimeCompositor,
    Recorder,
    #[cfg(not(feature = "recorder-worker"))]
    ComputerControlOrb,
}

impl Profile {
    pub(crate) const fn directory(self) -> &'static str {
        match self {
            Self::Common => "common",
            #[cfg(not(feature = "recorder-worker"))]
            Self::CreationDebug => "creation-debug",
            #[cfg(not(feature = "recorder-worker"))]
            Self::ResultNavigation => "result-navigation",
            #[cfg(not(feature = "recorder-worker"))]
            Self::ResultCompositor => "result-compositor",
            #[cfg(not(feature = "recorder-worker"))]
            Self::StatusCompositor => "status-compositor",
            #[cfg(not(feature = "recorder-worker"))]
            Self::RealtimeCompositor => "realtime-compositor",
            Self::Recorder => "screen-recorder-worker",
            #[cfg(not(feature = "recorder-worker"))]
            Self::ComputerControlOrb => "cc-orb",
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Snapshot {
    pub(crate) contexts_created: u64,
    pub(crate) contexts_active: usize,
    pub(crate) contexts_failed: u64,
}

/// RAII owner for a thread-affine WRY context.
pub(crate) struct ManagedContext {
    profile: Profile,
    inner: WebContext,
}

impl ManagedContext {
    pub(crate) fn profile(&self) -> Profile {
        self.profile
    }
}

impl Deref for ManagedContext {
    type Target = WebContext;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for ManagedContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl Drop for ManagedContext {
    fn drop(&mut self) {
        CONTEXTS_ACTIVE.fetch_sub(1, Ordering::AcqRel);
        crate::log_info!(
            "[WebViewRuntime] context_closed profile={}",
            self.profile().directory()
        );
    }
}

pub(crate) fn data_dir(profile: Profile) -> PathBuf {
    data_dir_named(Some(profile.directory()))
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn creation_profile() -> Profile {
    if creation_debug_browser_args().is_some() {
        Profile::CreationDebug
    } else {
        Profile::Common
    }
}

#[cfg(not(feature = "recorder-worker"))]
pub(crate) fn creation_debug_browser_args() -> Option<(u16, String)> {
    let port = std::env::var("SGT_CREATION_WEBVIEW2_DEBUG_PORT")
        .ok()
        .and_then(|raw| raw.parse::<u16>().ok())
        .filter(|port| *port > 0)?;
    Some((
        port,
        format!("--remote-debugging-port={port} --remote-debugging-address=127.0.0.1"),
    ))
}

pub(crate) fn data_dir_named(subdir: Option<&str>) -> PathBuf {
    let mut path = std::env::var_os("SGT_CREATION_WEBVIEW2_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::paths::app_sgt_dir().join("webview_data"));
    if let Some(subdir) = subdir {
        path.push(subdir);
    }
    if let Err(error) = std::fs::create_dir_all(&path) {
        CONTEXTS_FAILED.fetch_add(1, Ordering::Relaxed);
        crate::log_info!(
            "[WebViewRuntime] data_dir_failed path={} error={error}",
            path.display()
        );
    }
    path
}

pub(crate) fn create_context(profile: Profile) -> ManagedContext {
    create_context_at(profile, data_dir(profile))
}

pub(crate) fn create_context_at(profile: Profile, path: impl AsRef<Path>) -> ManagedContext {
    let started = Instant::now();
    let inner = WebContext::new(Some(path.as_ref().to_path_buf()));
    CONTEXTS_CREATED.fetch_add(1, Ordering::Relaxed);
    CONTEXTS_ACTIVE.fetch_add(1, Ordering::AcqRel);
    let snapshot = snapshot();
    crate::log_info!(
        "[WebViewRuntime] context_created profile={} elapsed_ms={:.1} active={} created={} failed={}",
        profile.directory(),
        started.elapsed().as_secs_f64() * 1_000.0,
        snapshot.contexts_active,
        snapshot.contexts_created,
        snapshot.contexts_failed
    );
    ManagedContext { profile, inner }
}

pub(crate) fn snapshot() -> Snapshot {
    Snapshot {
        contexts_created: CONTEXTS_CREATED.load(Ordering::Relaxed),
        contexts_active: CONTEXTS_ACTIVE.load(Ordering::Acquire),
        contexts_failed: CONTEXTS_FAILED.load(Ordering::Relaxed),
    }
}

#[cfg(all(test, not(feature = "recorder-worker")))]
mod tests {
    use super::*;

    #[test]
    fn compatible_surfaces_share_one_profile() {
        assert_eq!(Profile::Common.directory(), "common");
        assert_ne!(
            Profile::Common.directory(),
            Profile::ResultCompositor.directory()
        );
    }

    #[test]
    fn every_profile_has_a_stable_nonempty_directory() {
        for profile in [
            Profile::Common,
            Profile::CreationDebug,
            Profile::ResultNavigation,
            Profile::ResultCompositor,
            Profile::StatusCompositor,
            Profile::RealtimeCompositor,
            Profile::Recorder,
            Profile::ComputerControlOrb,
        ] {
            assert!(!profile.directory().is_empty());
            assert!(!profile.directory().contains(['/', '\\']));
        }
    }
}
