use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) const MAX_TEXT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DropError {
    TooLarge,
    Unsupported,
    ReadFailed,
    Busy,
    ProcessingFailed,
}

pub(super) struct SelectionGuard<'a>(&'a AtomicBool);

impl<'a> SelectionGuard<'a> {
    pub fn try_acquire(flag: &'a AtomicBool) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(flag))
    }
}

impl Drop for SelectionGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

pub(super) fn read_file(path: &Path, limit: usize) -> Result<Vec<u8>, DropError> {
    let file = std::fs::File::open(path).map_err(|_| DropError::ReadFailed)?;
    let metadata = file.metadata().map_err(|_| DropError::ReadFailed)?;
    if !metadata.is_file() {
        return Err(DropError::Unsupported);
    }
    if metadata.len() > limit as u64 {
        return Err(DropError::TooLarge);
    }
    read_bounded(file, limit)
}

fn read_bounded(reader: impl Read, limit: usize) -> Result<Vec<u8>, DropError> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| DropError::ReadFailed)?;
    if bytes.len() > limit {
        Err(DropError::TooLarge)
    } else {
        Ok(bytes)
    }
}

pub(super) fn text(bytes: Vec<u8>) -> Result<String, DropError> {
    if bytes.len() > MAX_TEXT_BYTES {
        return Err(DropError::TooLarge);
    }
    // Common binary containers can contain valid UTF-8 and no NULs.
    if [
        b"%PDF-".as_slice(),
        b"PK\x03\x04",
        b"PK\x05\x06",
        b"PK\x07\x08",
        b"\x7fELF",
        b"\x1f\x8b",
        b"7z\xbc\xaf\x27\x1c",
    ]
    .iter()
    .any(|magic| bytes.starts_with(magic))
    {
        return Err(DropError::Unsupported);
    }
    let text = String::from_utf8(bytes).map_err(|_| DropError::Unsupported)?;
    if text
        .chars()
        .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t' | '\u{c}'))
    {
        return Err(DropError::Unsupported);
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_no_more_than_one_byte_over_the_limit() {
        let mut input = std::io::Cursor::new(b"abcdef");
        assert_eq!(read_bounded(&mut input, 4), Err(DropError::TooLarge));
        assert_eq!(input.position(), 5);
        assert_eq!(read_bounded(&b"four"[..], 4).unwrap(), b"four");
    }

    #[test]
    fn text_sniffing_preserves_unicode_and_rejects_binary() {
        assert_eq!(
            text("Tiếng Việt 한글\n".as_bytes().to_vec()).unwrap(),
            "Tiếng Việt 한글\n"
        );
        for bytes in [
            b"hello\0world".as_slice(),
            b"%PDF-1.4\n",
            b"PK\x03\x04stuff",
            b"\xff",
        ] {
            assert_eq!(text(bytes.to_vec()), Err(DropError::Unsupported));
        }
    }

    #[test]
    fn only_one_drop_selector_can_own_the_wheel() {
        let flag = AtomicBool::new(false);
        let first = SelectionGuard::try_acquire(&flag).unwrap();
        assert!(SelectionGuard::try_acquire(&flag).is_none());
        drop(first);
        assert!(SelectionGuard::try_acquire(&flag).is_some());
    }

    #[test]
    fn selection_tracks_preset_identity_after_reorder_or_deletion() {
        let mut config = crate::config::Config::default();
        let first = crate::config::Preset {
            id: "first".into(),
            ..Default::default()
        };
        let second = crate::config::Preset {
            id: "second".into(),
            ..Default::default()
        };
        config.presets = vec![second, first];
        let selected = super::super::select_current_preset(&mut config, "first").unwrap();
        assert_eq!(selected.id, "first");
        assert_eq!(config.active_preset_idx, 1);
        config.presets.remove(1);
        assert!(super::super::select_current_preset(&mut config, "first").is_none());
    }
}
