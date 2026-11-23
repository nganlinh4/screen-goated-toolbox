//! Ownership of intermediate or not-yet-published files.
//!
//! A worker step that creates a file it may have to abandon holds it in an
//! [`OwnedFile`]. Every early return, `?`, or panic then removes the file; only
//! an explicit [`OwnedFile::keep`] hands the path on to the caller.

use std::path::{Path, PathBuf};

pub(crate) struct OwnedFile {
    path: Option<PathBuf>,
}

impl OwnedFile {
    pub(crate) fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: Some(path.into()),
        }
    }

    pub(crate) fn path(&self) -> &Path {
        self.path
            .as_deref()
            .expect("owned file path is present until kept")
    }

    /// Releases ownership: the file survives this guard.
    pub(crate) fn keep(mut self) -> PathBuf {
        self.path
            .take()
            .expect("owned file path is present until kept")
    }
}

impl Drop for OwnedFile {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::OwnedFile;

    fn temp_file(name: &str) -> std::path::PathBuf {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let path = std::env::temp_dir().join(format!(
            "sgt-owned-file-{name}-{}-{}",
            std::process::id(),
            u64::from_ne_bytes(random)
        ));
        std::fs::write(&path, b"data").unwrap();
        path
    }

    #[test]
    fn dropped_guard_removes_the_file_and_kept_guard_does_not() {
        let abandoned = temp_file("abandoned");
        drop(OwnedFile::new(&abandoned));
        assert!(!abandoned.exists());

        let published = temp_file("published");
        let kept = OwnedFile::new(&published).keep();
        assert!(kept.exists());
        std::fs::remove_file(kept).unwrap();
    }
}
