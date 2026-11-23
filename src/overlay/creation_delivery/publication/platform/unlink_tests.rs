use std::io::Read as _;
use std::os::windows::fs::OpenOptionsExt as _;
use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
use std::path::PathBuf;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE};
use windows::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_READ, MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, PAGE_READONLY,
    SEC_IMAGE, UnmapViewOfFile,
};
use windows::core::PCWSTR;

use crate::overlay::creation_delivery::publication::{lock_owned_path, unlink_owned};
use crate::overlay::creation_file_identity;

struct Fixture {
    root: PathBuf,
    path: PathBuf,
    original_permissions: Option<std::fs::Permissions>,
}

impl Fixture {
    fn new() -> Self {
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).unwrap();
        let nonce = nonce
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let root = std::env::temp_dir().join(format!("sgt-owned-unlink-{nonce}"));
        std::fs::create_dir(&root).unwrap();
        let path = root.join("owned");
        Self {
            root,
            path,
            original_permissions: None,
        }
    }

    fn read_shared(&self) -> std::fs::File {
        std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
            .open(&self.path)
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(permissions) = self.original_permissions.take() {
            let _ = std::fs::set_permissions(&self.path, permissions);
        }
        for name in ["owned", "foreign"] {
            let path = self.root.join(name);
            if let Err(error) = std::fs::remove_file(&path)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                eprintln!("unlink fixture retained at {}: {error}", path.display());
            }
        }
        if let Err(error) = std::fs::remove_dir(&self.root) {
            eprintln!(
                "unlink fixture retained at {}: {error}",
                self.root.display()
            );
        }
    }
}

#[test]
fn unlink_retires_only_the_attested_name_while_old_readers_survive_reuse() {
    let fixture = Fixture::new();
    std::fs::write(&fixture.path, b"original").unwrap();
    let foreign = fixture.root.join("foreign");
    std::fs::write(&foreign, b"unrelated").unwrap();
    let mut observer = fixture.read_shared();
    let original = creation_file_identity::from_file(&observer).unwrap();
    let owned = lock_owned_path(&fixture.path, Some(&original), false, true).unwrap();
    assert!(std::fs::rename(&fixture.path, fixture.root.join("moved")).is_err());
    // Windows must act on the verified handle, never the supplied path string.
    unlink_owned(owned, &foreign).unwrap();
    let replacement = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&fixture.path)
        .unwrap();
    drop(replacement);
    std::fs::write(&fixture.path, b"replacement").unwrap();
    let mut bytes = Vec::new();
    observer.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"original");
    assert_eq!(
        creation_file_identity::from_file(&observer).unwrap(),
        original
    );
    assert_ne!(
        creation_file_identity::from_path(&fixture.path).unwrap(),
        original
    );
    drop(observer);
    assert_eq!(std::fs::read(&fixture.path).unwrap(), b"replacement");
    assert_eq!(std::fs::read(foreign).unwrap(), b"unrelated");
}

#[test]
fn unlink_without_delete_access_preserves_the_file_and_releases_the_handle() {
    let fixture = Fixture::new();
    std::fs::write(&fixture.path, b"retained").unwrap();
    let file = fixture.read_shared();
    let identity = creation_file_identity::from_file(&file).unwrap();
    assert!(unlink_owned(file, &fixture.path).is_err());
    assert_eq!(std::fs::read(&fixture.path).unwrap(), b"retained");
    assert_eq!(
        creation_file_identity::from_path(&fixture.path).unwrap(),
        identity
    );
    let owned = lock_owned_path(&fixture.path, Some(&identity), false, true).unwrap();
    unlink_owned(owned, &fixture.path).unwrap();
    assert_eq!(
        std::fs::symlink_metadata(&fixture.path).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
}

#[test]
fn unlink_does_not_override_the_readonly_attribute() {
    let mut fixture = Fixture::new();
    std::fs::write(&fixture.path, b"retained").unwrap();
    let identity = creation_file_identity::from_path(&fixture.path).unwrap();
    let original = std::fs::metadata(&fixture.path).unwrap().permissions();
    fixture.original_permissions = Some(original.clone());
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&fixture.path, readonly).unwrap();
    let owned = lock_owned_path(&fixture.path, Some(&identity), false, true).unwrap();
    let result = unlink_owned(owned, &fixture.path);
    let permissions = std::fs::metadata(&fixture.path).unwrap().permissions();
    std::fs::set_permissions(&fixture.path, original).unwrap();
    assert!(result.is_err());
    assert!(permissions.readonly());
    assert_eq!(std::fs::read(&fixture.path).unwrap(), b"retained");
    assert_eq!(
        creation_file_identity::from_path(&fixture.path).unwrap(),
        identity
    );
    let owned = lock_owned_path(&fixture.path, Some(&identity), false, true).unwrap();
    unlink_owned(owned, &fixture.path).unwrap();
}

struct ImageView(MEMORY_MAPPED_VIEW_ADDRESS);

impl Drop for ImageView {
    fn drop(&mut self) {
        let _ = unsafe { UnmapViewOfFile(self.0) };
    }
}

#[test]
fn unlink_preserves_a_mapped_image_until_its_view_is_released() {
    let fixture = Fixture::new();
    std::fs::copy(std::env::current_exe().unwrap(), &fixture.path).unwrap();
    let identity = creation_file_identity::from_path(&fixture.path).unwrap();
    {
        let source = fixture.read_shared();
        let mapping = unsafe {
            CreateFileMappingW(
                HANDLE(source.as_raw_handle()),
                None,
                PAGE_READONLY | SEC_IMAGE,
                0,
                0,
                PCWSTR::null(),
            )
        }
        .unwrap();
        let _mapping = unsafe { OwnedHandle::from_raw_handle(mapping.0) };
        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0) };
        assert!(!view.Value.is_null());
        let _view = ImageView(view);
        let owned = lock_owned_path(&fixture.path, Some(&identity), false, true).unwrap();
        assert!(unlink_owned(owned, &fixture.path).is_err());
        assert_eq!(
            creation_file_identity::from_path(&fixture.path).unwrap(),
            identity
        );
    }
    let owned = lock_owned_path(&fixture.path, Some(&identity), false, true).unwrap();
    unlink_owned(owned, &fixture.path).unwrap();
    assert_eq!(
        std::fs::symlink_metadata(&fixture.path).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
}
