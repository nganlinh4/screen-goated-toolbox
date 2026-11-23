use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result};

use super::super::{ExternalToolDelivery, ExternalToolInstallEvent};
use crate::component_registry::persisted_download;

/// Downloads (or finishes) the artifact into its persisted partial file and
/// returns the verified path. A failed install keeps it for the next attempt.
pub(super) fn download(
    delivery: &ExternalToolDelivery,
    cancelled: &AtomicBool,
    on_event: &impl Fn(ExternalToolInstallEvent),
) -> Result<PathBuf> {
    on_event(ExternalToolInstallEvent::Downloading {
        downloaded: 0,
        total: delivery.size_bytes,
    });
    persisted_download::fetch(
        delivery.id,
        &pinned_body(delivery),
        cancelled,
        &|downloaded, total| on_event(ExternalToolInstallEvent::Downloading { downloaded, total }),
    )
    .with_context(|| format!("{} download failed", delivery.id))
}

/// Transfers the artifact while holding only this tool's transfer guard, so
/// the caller runs it before the registry mutation guard.
pub(super) fn prefetch(
    delivery: &ExternalToolDelivery,
    cancelled: &AtomicBool,
    on_event: &impl Fn(ExternalToolInstallEvent),
) -> Result<()> {
    on_event(ExternalToolInstallEvent::Downloading {
        downloaded: 0,
        total: delivery.size_bytes,
    });
    persisted_download::prefetch(
        delivery.id,
        &[pinned_body(delivery)],
        cancelled,
        &|downloaded, total| on_event(ExternalToolInstallEvent::Downloading { downloaded, total }),
    )
    .with_context(|| format!("{} download failed", delivery.id))
}

fn pinned_body(delivery: &ExternalToolDelivery) -> persisted_download::PinnedBody<'_> {
    persisted_download::PinnedBody {
        url: delivery.download_url,
        size: delivery.size_bytes,
        sha256: delivery.sha256,
    }
}
