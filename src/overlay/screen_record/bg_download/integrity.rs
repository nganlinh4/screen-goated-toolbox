//! Pinned catalog integrity. Each downloadable background carries the SHA-256
//! and byte length of the exact bytes the catalog was reviewed with, so a
//! replaced or truncated upstream asset fails loudly instead of installing
//! different art.

use sha2::{Digest as _, Sha256};
use std::io::{Read, Write};

pub(super) struct PinnedAsset<'a> {
    pub sha256: &'a str,
    pub bytes: u64,
}

/// A pin is usable only when it names a positive length and a lowercase hex digest.
pub(super) fn validate_pin(pin: &PinnedAsset<'_>) -> Result<(), String> {
    let hex = pin.sha256.len() == 64
        && pin
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if !hex || pin.bytes == 0 || pin.bytes > super::MAX_BACKGROUND_BYTES {
        return Err("Background catalog entry has no valid integrity pin".to_string());
    }
    Ok(())
}

/// Copies the response into `out`, never past the pinned length, and accepts
/// it only when length and digest both match. `before_chunk` runs before every
/// read (cancellation); `progress` receives the bytes copied so far.
pub(super) fn copy_pinned(
    reader: &mut impl Read,
    out: &mut impl Write,
    pin: &PinnedAsset<'_>,
    mut before_chunk: impl FnMut() -> Result<(), String>,
    mut progress: impl FnMut(u64),
) -> Result<(), String> {
    validate_pin(pin)?;
    let mut hasher = Sha256::new();
    let mut copied = 0u64;
    let mut buffer = [0u8; 16_384];
    loop {
        before_chunk()?;
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("Download read error: {error}"))?;
        if count == 0 {
            break;
        }
        copied = copied
            .checked_add(count as u64)
            .filter(|total| *total <= pin.bytes)
            .ok_or_else(|| "Background image is larger than the catalog pin".to_string())?;
        hasher.update(&buffer[..count]);
        out.write_all(&buffer[..count])
            .map_err(|error| format!("Download write error: {error}"))?;
        progress(copied);
    }
    if copied != pin.bytes {
        return Err(format!(
            "Background image is {copied} bytes, the catalog pin is {}",
            pin.bytes
        ));
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if digest != pin.sha256 {
        return Err("Background image does not match the catalog integrity pin".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin_for(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn copy(body: &[u8], sha256: &str, bytes: u64) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        copy_pinned(
            &mut std::io::Cursor::new(body),
            &mut out,
            &PinnedAsset { sha256, bytes },
            || Ok(()),
            |_| {},
        )?;
        Ok(out)
    }

    #[test]
    fn accepts_only_the_pinned_bytes() {
        let body = b"pinned background bytes";
        let sha = pin_for(body);
        assert_eq!(copy(body, &sha, body.len() as u64).unwrap(), body);
        let replaced = b"replaced background byte";
        assert!(copy(replaced, &sha, body.len() as u64).is_err());
        assert!(copy(&body[..10], &sha, body.len() as u64).is_err());
        assert!(copy(body, &sha, 10).is_err());
        assert!(copy(body, "not-a-digest", body.len() as u64).is_err());
    }

    #[test]
    fn every_catalog_entry_is_pinned() {
        let catalog = super::super::downloadable_backgrounds();
        assert_eq!(catalog.len(), 29, "the embedded catalog must parse in full");
        for entry in catalog {
            validate_pin(&PinnedAsset {
                sha256: &entry.sha256,
                bytes: entry.bytes,
            })
            .unwrap_or_else(|error| panic!("{}: {error}", entry.id));
        }
    }
}
