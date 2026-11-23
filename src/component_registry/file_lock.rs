use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use std::os::windows::fs::OpenOptionsExt as _;
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_SHARE_READ,
    FILE_SHARE_WRITE,
};

use super::receipt::is_reparse_point;

pub(super) struct FileLease {
    file: Option<File>,
    path: PathBuf,
    _directories: Vec<File>,
}

impl FileLease {
    pub(super) fn shared(id: &str) -> Result<Self> {
        Self::open_in(&lock_root(), id, false)?.context("component is being modified")
    }

    pub(super) fn exclusive(id: &str) -> Result<Option<Self>> {
        Self::open_in(&lock_root(), id, true)
    }

    fn open_in(root: &Path, id: &str, exclusive: bool) -> Result<Option<Self>> {
        super::catalog::validate_identifier(id)?;
        let directories = lock_directory_tree(root)?;
        let path = root.join(format!("{id}.lock"));
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match OpenOptions::new().write(true).create_new(true).open(&path) {
                    Ok(file) => drop(file),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        }
        let file = open_lock(&path, exclusive)?;
        Ok(file.map(|file| Self {
            file: Some(file),
            path,
            _directories: directories,
        }))
    }

    pub(super) fn upgrade(&mut self) -> Result<bool> {
        drop(self.file.take());
        match open_lock(&self.path, true) {
            Ok(Some(file)) => {
                self.file = Some(file);
                Ok(true)
            }
            result => {
                self.file = open_lock(&self.path, false)?;
                if self.file.is_none() {
                    bail!("could not restore component lease after upgrade failure");
                }
                result.map(|_| false)
            }
        }
    }
}

fn open_lock(path: &Path, exclusive: bool) -> Result<Option<File>> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .access_mode(FILE_GENERIC_READ.0)
        .share_mode(if exclusive { 0 } else { FILE_SHARE_READ.0 })
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) if matches!(error.raw_os_error(), Some(32 | 33)) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("component lease file is unsafe");
    }
    Ok(Some(file))
}

fn lock_directory_tree(root: &Path) -> Result<Vec<File>> {
    if !root.is_absolute() {
        bail!("component lease directory must be absolute");
    }
    let mut paths = root.ancestors().collect::<Vec<_>>();
    paths.reverse();
    let mut directories = Vec::with_capacity(paths.len());
    for path in paths {
        match std::fs::symlink_metadata(path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match std::fs::create_dir(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        }
        let file = OpenOptions::new()
            .read(true)
            .access_mode(FILE_GENERIC_READ.0)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0 | FILE_FLAG_BACKUP_SEMANTICS.0)
            .open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_dir() || is_reparse_point(&metadata) {
            bail!("component lease directory is unsafe");
        }
        directories.push(file);
    }
    Ok(directories)
}

fn lock_root() -> PathBuf {
    #[cfg(test)]
    return std::env::temp_dir().join(format!("sgt-component-leases-test-{}", std::process::id()));
    #[cfg(not(test))]
    super::components_root()
        .parent()
        .expect("component root has a parent")
        .join("component-leases")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    struct ChildGuard(Child);

    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn lease_child_process() {
        let Some(root) = std::env::var_os("SGT_LEASE_TEST_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let id = std::env::var("SGT_LEASE_TEST_ID").unwrap_or_else(|_| "component".to_string());
        let _lease = FileLease::open_in(&root, &id, false).unwrap().unwrap();
        std::fs::write(root.join(format!("{id}.ready")), b"ready").unwrap();
        loop {
            std::thread::park();
        }
    }

    #[test]
    fn shared_leases_block_exclusive_until_other_process_dies() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "sgt-cross-process-lease-{}-{unique}",
            std::process::id()
        ));
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "component_registry::file_lock::tests::lease_child_process",
            ])
            .env("SGT_LEASE_TEST_ROOT", &root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut child = ChildGuard(child);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !root.join("component.ready").is_file() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "lease child exited early"
            );
            assert!(
                Instant::now() < deadline,
                "lease child did not become ready"
            );
            std::thread::sleep(Duration::from_millis(20));
        }

        let local = FileLease::open_in(&root, "component", false)
            .unwrap()
            .unwrap();
        assert!(
            FileLease::open_in(&root, "component", true)
                .unwrap()
                .is_none()
        );
        drop(local);
        assert!(
            FileLease::open_in(&root, "component", true)
                .unwrap()
                .is_none()
        );
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let exclusive = FileLease::open_in(&root, "component", true)
            .unwrap()
            .unwrap();
        assert!(
            FileLease::open_in(&root, "component", false)
                .unwrap()
                .is_none()
        );
        drop(exclusive);
        std::fs::remove_file(root.join("component.ready")).unwrap();
        std::fs::remove_file(root.join("component.lock")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn pending_removal_retries_after_foreign_process_terminates() {
        let id = format!("test-cross-process-{}", std::process::id());
        let root = lock_root();
        let ready = root.join(format!("{id}.ready"));
        let child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "component_registry::file_lock::tests::lease_child_process",
            ])
            .env("SGT_LEASE_TEST_ROOT", &root)
            .env("SGT_LEASE_TEST_ID", &id)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut child = ChildGuard(child);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !ready.is_file() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "lease child exited early"
            );
            assert!(
                Instant::now() < deadline,
                "lease child did not become ready"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            super::super::removal::request_remove(&id).unwrap(),
            super::super::RemovalOutcome::Pending
        );
        assert!(super::super::acquire(&id).is_err());
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(lease) = super::super::acquire(&id) {
                drop(lease);
                break;
            }
            assert!(Instant::now() < deadline, "pending removal did not resume");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(matches!(
            super::super::removal::request_remove_and_wait(&id).unwrap(),
            super::super::RemovalOutcome::Missing | super::super::RemovalOutcome::Removed
        ));
        std::fs::remove_file(ready).unwrap();
        std::fs::remove_file(root.join(format!("{id}.lock"))).unwrap();
    }
}
