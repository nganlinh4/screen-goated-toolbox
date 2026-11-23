//! Pinned-digest download that resumes a stalled or truncated body with an
//! HTTP `Range` request instead of restarting a multi-GB transfer from zero.
//! The SHA-256 state carries across resumes, so the bytes are still verified
//! exactly once against the pinned digest.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// Resumes allowed without forward progress between them.
const MAX_RESUMES_WITHOUT_PROGRESS: u32 = 4;
/// Received bytes that restore the resume budget after a stall.
const PROGRESS_RESETS_RESUMES: u64 = 16 * 1024 * 1024;
const RESUME_BACKOFF_BASE: Duration = Duration::from_secs(1);

pub(super) type BodyFetcher<'a> = dyn FnMut(u64) -> Result<Box<dyn Read>> + 'a;

/// The server's validators for a body, kept so a later attempt can ask for
/// the rest of the same representation with `If-Range`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct BodyValidators {
    pub(super) etag: Option<String>,
    pub(super) last_modified: Option<String>,
}

impl BodyValidators {
    /// A strong entity tag, otherwise the modification date; weak tags are not
    /// valid in `If-Range`.
    pub(super) fn if_range(&self) -> Option<&str> {
        self.etag
            .as_deref()
            .filter(|tag| !tag.starts_with("W/"))
            .or(self.last_modified.as_deref())
    }
}

/// A body opened at `offset`, or from byte 0 when the server answered a range
/// request with the complete representation.
pub(super) struct OpenedBody {
    pub(super) reader: Box<dyn Read>,
    pub(super) offset: u64,
    pub(super) validators: BodyValidators,
}

/// Downloads `url` into `output`, resuming from the received offset after a
/// read failure or early end of body, and verifies size and digest.
// Kept for a returning offline engine's model downloads.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "offline-engine shell: no engine downloads models today; see docs/OFFLINE-ENGINES.md"
    )
)]
pub(super) fn download_verified(
    url: &str,
    size: u64,
    digest: &str,
    output: &mut dyn Write,
    cancelled: &AtomicBool,
    progress: &dyn Fn(u64),
) -> Result<()> {
    let mut fetch = |offset: u64| fetch_from(url, offset, size);
    copy_resuming(
        &mut fetch,
        output,
        size,
        digest,
        cancelled,
        progress,
        &|delay| wait_unless_cancelled(delay, cancelled),
    )
}

// Kept for a returning offline engine's model downloads.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "offline-engine shell: no engine downloads models today; see docs/OFFLINE-ENGINES.md"
    )
)]
fn fetch_from(url: &str, offset: u64, size: u64) -> Result<Box<dyn Read>> {
    fetch_exactly_from(url, offset, size, None)
}

/// Opens the body at exactly `offset`; a complete response to a range
/// request is a failure here because earlier bytes were already written.
// Kept for a returning offline engine's model downloads.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "offline-engine shell: no engine downloads models today; see docs/OFFLINE-ENGINES.md"
    )
)]
pub(super) fn fetch_exactly_from(
    url: &str,
    offset: u64,
    size: u64,
    if_range: Option<&str>,
) -> Result<Box<dyn Read>> {
    let body = open_body(url, offset, size, if_range)?;
    ensure!(
        body.offset == offset,
        "download server did not resume at byte {offset} (HTTP 200)"
    );
    Ok(body.reader)
}

/// Opens `url` at `offset` with a `Range` request, conditional on `if_range`
/// when given. A complete `200` response restarts the body at byte 0.
pub(super) fn open_body(
    url: &str,
    offset: u64,
    size: u64,
    if_range: Option<&str>,
) -> Result<OpenedBody> {
    let mut request = crate::api::client::UREQ_DOWNLOAD_AGENT
        .get(url)
        .header("User-Agent", "ScreenGoatedToolbox");
    if offset > 0 {
        request = request.header("Range", format!("bytes={offset}-"));
        if let Some(validator) = if_range {
            request = request.header("If-Range", validator);
        }
    }
    let response = request.call()?;
    let status = response.status().as_u16();
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    let validators = BodyValidators {
        etag: header("etag"),
        last_modified: header("last-modified"),
    };
    let opened_at = match status {
        200 => 0,
        206 => {
            let content_range = header("content-range").unwrap_or_default();
            ensure!(
                offset > 0 && content_range_matches(&content_range, offset, size),
                "download server did not resume at byte {offset} (HTTP {status})"
            );
            offset
        }
        _ => anyhow::bail!("download did not return a complete response (HTTP {status})"),
    };
    Ok(OpenedBody {
        reader: Box::new(response.into_body().into_reader()),
        offset: opened_at,
        validators,
    })
}

fn content_range_matches(value: &str, offset: u64, size: u64) -> bool {
    let Some((range, total)) = value
        .trim()
        .strip_prefix("bytes ")
        .and_then(|rest| rest.split_once('/'))
    else {
        return false;
    };
    let Some((start, end)) = range.split_once('-') else {
        return false;
    };
    start.parse::<u64>().ok() == Some(offset)
        && end.parse::<u64>().ok() == size.checked_sub(1)
        && total.parse::<u64>().ok() == Some(size)
}

// Kept for a returning offline engine's model downloads.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "offline-engine shell: no engine downloads models today; see docs/OFFLINE-ENGINES.md"
    )
)]
fn copy_resuming(
    fetch: &mut BodyFetcher<'_>,
    output: &mut dyn Write,
    size: u64,
    digest: &str,
    cancelled: &AtomicBool,
    progress: &dyn Fn(u64),
    wait: &dyn Fn(Duration) -> bool,
) -> Result<()> {
    let reader = fetch(0)?;
    let hash = copy_from(
        reader,
        fetch,
        output,
        0,
        Sha256::new(),
        size,
        cancelled,
        progress,
        wait,
    )?;
    ensure!(
        format!("{:x}", hash.finalize()).eq_ignore_ascii_case(digest),
        "delivered content differs from its pinned digest"
    );
    Ok(())
}

/// Copies the body from `start` (whose earlier bytes are already in `hash`)
/// until `size` bytes arrived, resuming through `fetch` after a failure, and
/// returns the hash state over all `size` bytes.
#[expect(
    clippy::too_many_arguments,
    reason = "a resumed copy needs its source, sink, offset, hash and cancellation"
)]
pub(super) fn copy_from(
    first: Box<dyn Read>,
    fetch: &mut BodyFetcher<'_>,
    output: &mut dyn Write,
    start: u64,
    mut hash: Sha256,
    size: u64,
    cancelled: &AtomicBool,
    progress: &dyn Fn(u64),
    wait: &dyn Fn(Duration) -> bool,
) -> Result<Sha256> {
    let mut total = start;
    let mut buffer = vec![0_u8; 256 * 1024];
    let mut resumes = 0_u32;
    let mut since_resume = 0_u64;
    let mut reader = first;
    loop {
        ensure!(!cancelled.load(Ordering::Acquire), "download cancelled");
        let failure = match reader.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            // Every pinned byte has arrived; a connection reset before the
            // clean end of body loses nothing, and the digest still decides.
            // Resuming here would ask for a range past the end and fail.
            Ok(0) | Err(_) if total == size => break,
            Ok(0) => anyhow::anyhow!("download ended at byte {total} of {size}"),
            Ok(read) => {
                total = total
                    .checked_add(read as u64)
                    .filter(|total| *total <= size)
                    .ok_or_else(|| anyhow::anyhow!("download exceeds its pinned size"))?;
                output.write_all(&buffer[..read])?;
                hash.update(&buffer[..read]);
                progress(total);
                since_resume = since_resume.saturating_add(read as u64);
                if since_resume >= PROGRESS_RESETS_RESUMES {
                    resumes = 0;
                }
                continue;
            }
            Err(error) => error.into(),
        };
        reader = resume(fetch, total, &mut resumes, wait, failure)?;
        since_resume = 0;
    }
    Ok(hash)
}

/// Reopens the body at `offset` with bounded, backed-off attempts; returns
/// the original failure once the budget is spent.
fn resume(
    fetch: &mut BodyFetcher<'_>,
    offset: u64,
    resumes: &mut u32,
    wait: &dyn Fn(Duration) -> bool,
    mut failure: anyhow::Error,
) -> Result<Box<dyn Read>> {
    while *resumes < MAX_RESUMES_WITHOUT_PROGRESS {
        let delay = RESUME_BACKOFF_BASE.saturating_mul(1 << *resumes);
        *resumes += 1;
        crate::log_info!(
            "[Components] download stalled at byte {offset}: {failure}; resume {}/{} in {}ms",
            resumes,
            MAX_RESUMES_WITHOUT_PROGRESS,
            delay.as_millis()
        );
        ensure!(wait(delay), "download cancelled");
        match fetch(offset) {
            Ok(reader) => return Ok(reader),
            Err(error) => failure = error,
        }
    }
    Err(failure.context(format!("download could not resume at byte {offset}")))
}

pub(super) fn wait_unless_cancelled(delay: Duration, cancelled: &AtomicBool) -> bool {
    let step = Duration::from_millis(100);
    let mut waited = Duration::ZERO;
    while waited < delay {
        if cancelled.load(Ordering::Acquire) {
            return false;
        }
        std::thread::sleep(step.min(delay - waited));
        waited += step;
    }
    !cancelled.load(Ordering::Acquire)
}

#[cfg(test)]
#[path = "resumable_download_tests.rs"]
mod tests;
