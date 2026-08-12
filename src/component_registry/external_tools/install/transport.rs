use std::io::Write;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result, bail};

use super::super::{ExternalToolDelivery, ExternalToolInstallEvent};
use super::copy_bounded;

pub(super) fn download(
    delivery: &ExternalToolDelivery,
    target: &Path,
    cancelled: &AtomicBool,
    on_event: &impl Fn(ExternalToolInstallEvent),
) -> Result<()> {
    let response = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(delivery.download_url)
        .header("User-Agent", "ScreenGoatedToolbox")
        .call()
        .with_context(|| format!("{} download failed", delivery.id))?;
    if response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|size| size != delivery.size_bytes)
    {
        bail!("{} download size does not match this build", delivery.id);
    }
    let mut reader = response.into_body().into_reader();
    let mut output = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(target)
        .with_context(|| format!("create external tool download file {}", target.display()))?;
    on_event(ExternalToolInstallEvent::Downloading {
        downloaded: 0,
        total: delivery.size_bytes,
    });
    copy_bounded(
        &mut reader,
        &mut output,
        delivery.size_bytes,
        cancelled,
        |downloaded, total| {
            on_event(ExternalToolInstallEvent::Downloading { downloaded, total });
        },
    )?;
    output
        .flush()
        .with_context(|| format!("flush external tool download {}", target.display()))?;
    output
        .sync_all()
        .with_context(|| format!("sync external tool download {}", target.display()))?;
    Ok(())
}
