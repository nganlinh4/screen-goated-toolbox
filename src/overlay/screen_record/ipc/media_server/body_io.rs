use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::Path;

pub(super) const MAX_RESTORED_VIDEO_BYTES: u64 = 32 * 1024 * 1024 * 1024;
pub(super) const MAX_IMPORTED_VIDEO_BYTES: u64 = 32 * 1024 * 1024 * 1024;
pub(super) const MAX_IMPORTED_AUDIO_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub(super) fn write_body_bounded(
    reader: &mut dyn std::io::Read,
    output: &Path,
    max_bytes: u64,
) -> Result<u64, String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|error| format!("Create upload file: {error}"))?;
    let result = (|| -> Result<u64, String> {
        let mut total = 0u64;
        let mut buffer = [0u8; 1024 * 1024];
        loop {
            let count = reader
                .read(&mut buffer)
                .map_err(|error| format!("Read upload body: {error}"))?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count as u64)
                .ok_or_else(|| "Upload byte count overflowed".to_string())?;
            if total > max_bytes {
                return Err(format!("Upload exceeds the {max_bytes}-byte limit"));
            }
            file.write_all(&buffer[..count])
                .map_err(|error| format!("Write upload file: {error}"))?;
        }
        if total == 0 {
            return Err("Upload is empty".to_string());
        }
        file.sync_all()
            .map_err(|error| format!("Sync upload file: {error}"))?;
        Ok(total)
    })();
    drop(file);
    if result.is_err() {
        let _ = std::fs::remove_file(output);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::write_body_bounded;

    #[test]
    fn bounded_writer_rejects_one_byte_over_limit_and_removes_the_file() {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!(
            "sgt-body-io-{}-{}",
            std::process::id(),
            u64::from_ne_bytes(random)
        ));
        assert!(write_body_bounded(&mut &b"1234"[..], &path, 3).is_err());
        assert!(!path.exists());
        assert_eq!(write_body_bounded(&mut &b"123"[..], &path, 3).unwrap(), 3);
        std::fs::remove_file(path).unwrap();
    }
}
