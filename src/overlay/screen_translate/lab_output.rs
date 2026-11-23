use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

pub(super) struct StagedFile {
    temporary: Option<PathBuf>,
    destination: PathBuf,
}

impl StagedFile {
    pub(super) fn new(destination: &Path, bytes: &[u8]) -> io::Result<Self> {
        for _ in 0..128 {
            let mut name = destination.as_os_str().to_owned();
            name.push(format!(
                ".tmp-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            let temporary = PathBuf::from(name);
            let mut file = match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
            {
                Ok(file) => file,
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            };
            let staged = Self {
                temporary: Some(temporary),
                destination: destination.to_owned(),
            };
            let result = file.write_all(bytes);
            drop(file);
            result?;
            return Ok(staged);
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate temporary output",
        ))
    }

    pub(super) fn publish(&mut self) -> io::Result<()> {
        let temporary = self
            .temporary
            .as_ref()
            .expect("staged output already published");
        std::fs::rename(temporary, &self.destination)?;
        self.temporary = None;
        Ok(())
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        if let Some(temporary) = self.temporary.take() {
            let _ = std::fs::remove_file(temporary);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staging_preserves_existing_output_until_publication_and_cleans_up() {
        let root = std::env::temp_dir().join(format!(
            "sgt-lab-output-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("result.json");
        std::fs::write(&path, b"newer").unwrap();
        {
            let staged = StagedFile::new(&path, b"stale").unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), b"newer");
            drop(staged);
        }
        assert_eq!(std::fs::read(&path).unwrap(), b"newer");
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        let mut staged = StagedFile::new(&path, b"current").unwrap();
        staged.publish().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"current");
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
