//! The one exact-size, pinned-SHA-256 stream check. Component installers,
//! the app updater, the WebView2 bootstrapper and the cursor gallery all copy
//! or re-read pinned bytes through [`stream_matches`].

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, ensure};
use sha2::{Digest as _, Sha256};

const BUFFER_BYTES: usize = 256 * 1024;

pub(super) fn copy_verified(
    reader: impl Read,
    output: &mut dyn Write,
    size: u64,
    digest: &str,
    cancelled: &AtomicBool,
    progress: &dyn Fn(u64),
) -> Result<()> {
    ensure!(
        matches(reader, output, size, digest, cancelled, progress)?,
        "delivered content differs from its pinned digest"
    );
    Ok(())
}

pub(super) fn matches(
    reader: impl Read,
    output: &mut dyn Write,
    size: u64,
    digest: &str,
    cancelled: &AtomicBool,
    progress: &dyn Fn(u64),
) -> Result<bool> {
    stream_matches(
        reader,
        output,
        size,
        digest,
        &|| cancelled.load(Ordering::Acquire),
        progress,
    )
}

/// Streams `reader` into `output` and reports whether exactly `size` bytes
/// with SHA-256 `digest` arrived. Input that grows past `size` is an error as
/// soon as it is read, so a hostile body never writes more than one buffer
/// beyond its pin; `should_stop` is checked before every read.
pub(crate) fn stream_matches(
    mut reader: impl Read,
    output: &mut dyn Write,
    size: u64,
    digest: &str,
    should_stop: &dyn Fn() -> bool,
    progress: &dyn Fn(u64),
) -> Result<bool> {
    let mut total = 0_u64;
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    loop {
        ensure!(!should_stop(), "content transfer cancelled");
        let read = match reader.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| anyhow::anyhow!("delivered content size overflow"))?;
        ensure!(total <= size, "delivered content exceeds its pinned size");
        output.write_all(&buffer[..read])?;
        hash.update(&buffer[..read]);
        progress(total);
    }
    Ok(total == size && format!("{:x}", hash.finalize()).eq_ignore_ascii_case(digest))
}

#[cfg(test)]
mod tests {
    use super::stream_matches;
    use sha2::{Digest as _, Sha256};
    use std::cell::Cell;

    fn digest(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn run(bytes: &[u8], size: u64, pin: &str) -> anyhow::Result<(bool, Vec<u8>)> {
        let mut output = Vec::new();
        let matched = stream_matches(bytes, &mut output, size, pin, &|| false, &|_| {})?;
        Ok((matched, output))
    }

    #[test]
    fn exact_bytes_match_and_are_copied() {
        let bytes = b"pinned body";
        let (matched, output) = run(bytes, bytes.len() as u64, &digest(bytes)).unwrap();
        assert!(matched);
        assert_eq!(output, bytes);
    }

    #[test]
    fn truncation_and_wrong_digest_do_not_match() {
        let bytes = b"pinned body";
        assert!(
            !run(&bytes[..4], bytes.len() as u64, &digest(bytes))
                .unwrap()
                .0
        );
        assert!(!run(bytes, bytes.len() as u64, &"0".repeat(64)).unwrap().0);
    }

    #[test]
    fn oversized_input_fails_before_it_is_written_past_the_pin() {
        let bytes = vec![1_u8; 64];
        let mut output = Vec::new();
        let error = stream_matches(
            bytes.as_slice(),
            &mut output,
            8,
            &digest(&bytes),
            &|| false,
            &|_| {},
        )
        .unwrap_err();
        assert!(error.to_string().contains("exceeds its pinned size"));
        assert!(output.is_empty());
    }

    #[test]
    fn a_stop_request_ends_the_transfer_between_reads() {
        let bytes = vec![7_u8; 3 * super::BUFFER_BYTES];
        let checks = Cell::new(0_usize);
        let mut output = Vec::new();
        let error = stream_matches(
            bytes.as_slice(),
            &mut output,
            bytes.len() as u64,
            &digest(&bytes),
            &|| {
                checks.set(checks.get() + 1);
                checks.get() > 1
            },
            &|_| {},
        )
        .unwrap_err();
        assert!(error.to_string().contains("cancelled"));
        assert_eq!(output.len(), super::BUFFER_BYTES);
    }
}
