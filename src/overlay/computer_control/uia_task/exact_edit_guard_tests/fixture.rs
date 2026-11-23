use super::*;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct Fixture {
    root: PathBuf,
    files: RefCell<Vec<PathBuf>>,
    directories: RefCell<Vec<PathBuf>>,
}

impl Fixture {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "sgt-exact-resource-identity-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self {
            root,
            files: RefCell::new(Vec::new()),
            directories: RefCell::new(Vec::new()),
        }
    }

    pub(super) fn path(&self, name: &str) -> PathBuf {
        assert!(
            Path::new(name)
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
        );
        self.root.join(name)
    }

    pub(super) fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(name);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        self.files.borrow_mut().push(path.clone());
        file.write_all(bytes).unwrap();
        path
    }

    pub(super) fn args(&self, name: &str) -> Value {
        let path = self.path(name);
        if !self.files.borrow().contains(&path) {
            self.file(name, b"Label,Pending\n");
        }
        json!({"path":path})
    }

    pub(super) fn link(&self, source: &str, target: &str) -> Value {
        let target = self.path(target);
        fs::hard_link(self.path(source), &target).unwrap();
        self.files.borrow_mut().push(target.clone());
        json!({"path":target})
    }

    pub(super) fn directory(&self, name: &str) {
        let path = self.path(name);
        fs::create_dir(&path).unwrap();
        self.directories.borrow_mut().push(path);
    }

    pub(super) fn rename_directory(
        &self,
        source: &str,
        target: &str,
        files: &[&str],
    ) -> std::io::Result<()> {
        let destination = self.path(target);
        fs::rename(self.path(source), &destination)?;
        self.directories.borrow_mut().push(destination.clone());
        for name in files {
            self.files.borrow_mut().push(destination.join(name));
        }
        Ok(())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for path in self.files.get_mut().iter().rev() {
            assert!(path.starts_with(&self.root));
            if let Err(error) = fs::remove_file(path) {
                assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
            }
        }
        for path in self.directories.get_mut().iter().rev() {
            assert!(path.starts_with(&self.root));
            if let Err(error) = fs::remove_dir(path) {
                assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
            }
        }
        fs::remove_dir(&self.root).unwrap();
    }
}

pub(super) fn receipt(args: &Value, mutation: bool) -> Value {
    let bytes = fs::read(args["path"].as_str().unwrap()).unwrap();
    json!({"ok":true,"effect_verified":mutation,"sha256":format!("{:x}",Sha256::digest(bytes))})
}

pub(super) fn record(guard: &mut ExactEditGuard, tool: &str, args: &Value, result: &mut Value) {
    let blocked = guard.before_action(tool, args);
    if result["ok"] == true {
        assert!(
            blocked.is_none(),
            "successful effect was preflight-blocked: {blocked:?}"
        );
    }
    if result["ok"] == true && result.get("sha256").is_none() {
        result["sha256"] = receipt(args, tool != "read_text_file")["sha256"].clone();
    }
    guard.record_result(tool, args, result);
}
