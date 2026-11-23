//! A scheduled elevated executable must remain outside unelevated control.

use anyhow::{Context, Result, ensure};
use std::fs::{File, OpenOptions};
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::fs::{MetadataExt as _, OpenOptionsExt as _};
use std::os::windows::io::AsRawHandle as _;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Globalization::{CSTR_EQUAL, CompareStringOrdinal};
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_GENERIC_READ, FILE_NAME_NORMALIZED, FILE_SHARE_READ, FILE_TYPE_DISK,
    GETFINALPATHNAMEBYHANDLE_FLAGS, GetDriveTypeW, GetFileType, GetFinalPathNameByHandleW,
    GetVolumeInformationByHandleW, VOLUME_NAME_DOS, VOLUME_NAME_GUID,
};
use windows::Win32::System::SystemServices::FILE_PERSISTENT_ACLS;
use windows::Win32::System::WindowsProgramming::DRIVE_FIXED;
use windows::core::PCWSTR;

mod acl;

pub(super) struct ProtectedTarget {
    path: PathBuf,
    _file: File,
    _ancestors: Vec<File>,
}

struct GuardedPath {
    path: PathBuf,
    file: File,
    ancestors: Vec<File>,
}

impl ProtectedTarget {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

pub(super) fn validate_task_descriptor(sddl: &str) -> Result<()> {
    acl::check_task(sddl)
}

/// Guards remain alive through task registration and its read-back validation.
pub(super) fn open(path: &Path) -> Result<ProtectedTarget> {
    let guarded = guard_path(path)?.context("Startup target is missing")?;
    let mut volume_flags = 0;
    unsafe {
        GetVolumeInformationByHandleW(
            HANDLE(guarded.ancestors[0].as_raw_handle()),
            None,
            None,
            None,
            Some(&mut volume_flags),
            None,
        )
    }
    .context("Startup volume security is unavailable")?;
    ensure!(
        volume_flags & FILE_PERSISTENT_ACLS != 0,
        "Startup volume does not preserve file ACLs"
    );
    for (index, directory) in guarded.ancestors.iter().enumerate() {
        let kind = if index + 1 == guarded.ancestors.len() {
            acl::Kind::Parent
        } else {
            acl::Kind::Ancestor
        };
        acl::check(directory, kind).context("Unprotected startup directory")?;
    }
    acl::check(&guarded.file, acl::Kind::File).context("Unprotected startup executable")?;
    Ok(ProtectedTarget {
        path: guarded.path,
        _file: guarded.file,
        _ancestors: guarded.ancestors,
    })
}

/// Presence is an authorization aid only; it never admits an elevated target.
pub(super) fn reference_exists(path: &Path) -> Result<bool> {
    Ok(guard_path(path)?.is_some())
}

fn guard_path(path: &Path) -> Result<Option<GuardedPath>> {
    let input = Input::parse(path)?;
    require_fixed_root(&input.root)?;
    ensure!(
        !input.parts.is_empty(),
        "Startup target must name an executable"
    );
    let root_kind = if input.parts.len() == 1 {
        acl::Kind::Parent
    } else {
        acl::Kind::Ancestor
    };
    let root = open_component(&input.root, root_kind)?;
    let canonical_root = final_path(&root, VOLUME_NAME_GUID)?;
    let root_input = Input::parse(&canonical_root)?;
    ensure!(
        matches!(root_input.namespace, Namespace::Volume) && root_input.parts.is_empty(),
        "Startup drive resolves through an ambiguous alias"
    );
    require_fixed_root(&canonical_root)?;
    verify_spelling(&root, &input.root, &canonical_root, input.namespace)?;
    let mut ancestors = vec![root];
    let mut exact_path = canonical_root;
    let mut input_path = input.root.clone();
    for (index, part) in input.parts.iter().enumerate() {
        exact_path.push(part);
        input_path.push(part);
        let kind = if index + 1 == input.parts.len() {
            acl::Kind::File
        } else if index + 2 == input.parts.len() {
            acl::Kind::Parent
        } else {
            acl::Kind::Ancestor
        };
        let component = match open_raw(&exact_path) {
            Ok(component) => component,
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        check_component(&component, kind)?;
        let canonical = final_path(&component, VOLUME_NAME_GUID)?;
        ensure!(
            same_path(&canonical, &exact_path),
            "Startup path contains a filename alias"
        );
        let spelling = if matches!(kind, acl::Kind::File) {
            &input.path
        } else {
            &input_path
        };
        verify_spelling(&component, spelling, &canonical, input.namespace)?;
        if matches!(kind, acl::Kind::File) {
            return Ok(Some(GuardedPath {
                path: canonical,
                file: component,
                ancestors,
            }));
        }
        ancestors.push(component);
    }
    unreachable!("Nonempty startup path must include a final component")
}

fn verify_spelling(
    file: &File,
    input: &Path,
    canonical: &Path,
    namespace: Namespace,
) -> Result<()> {
    let expected = match namespace {
        Namespace::Drive => final_path(file, VOLUME_NAME_DOS)?,
        Namespace::Volume => canonical.to_path_buf(),
    };
    ensure!(
        same_path(&expected, input),
        "Startup path contains an ambiguous alias"
    );
    Ok(())
}

fn require_fixed_root(root: &Path) -> Result<()> {
    let mut wide = root.as_os_str().encode_wide().collect::<Vec<_>>();
    wide.push(0);
    ensure!(
        unsafe { GetDriveTypeW(PCWSTR(wide.as_ptr())) } == DRIVE_FIXED,
        "Elevated startup requires a fixed local volume"
    );
    Ok(())
}

fn open_component(path: &Path, kind: acl::Kind) -> Result<File> {
    let file = open_raw(path)?;
    check_component(&file, kind)?;
    Ok(file)
}

fn check_component(file: &File, kind: acl::Kind) -> Result<()> {
    ensure!(
        unsafe { GetFileType(HANDLE(file.as_raw_handle())) } == FILE_TYPE_DISK,
        "Startup path is not a local disk object"
    );
    let metadata = file.metadata()?;
    ensure!(
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 == 0,
        "Startup path contains a reparse point"
    );
    ensure!(
        match kind {
            acl::Kind::File => metadata.is_file(),
            _ => metadata.is_dir(),
        },
        "Startup path has an unexpected object type"
    );
    Ok(())
}

fn open_raw(path: &Path) -> Result<File> {
    OpenOptions::new()
        .read(true)
        .access_mode(FILE_GENERIC_READ.0)
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(path)
        .with_context(|| format!("Cannot protect startup path {}", path.display()))
}

fn final_path(file: &File, volume: GETFINALPATHNAMEBYHANDLE_FLAGS) -> Result<PathBuf> {
    let mut buffer = vec![0_u16; 512];
    loop {
        let length = unsafe {
            GetFinalPathNameByHandleW(
                HANDLE(file.as_raw_handle()),
                &mut buffer,
                GETFINALPATHNAMEBYHANDLE_FLAGS(FILE_NAME_NORMALIZED.0 | volume.0),
            )
        } as usize;
        ensure!(
            length != 0,
            "Cannot resolve startup target: {}",
            std::io::Error::last_os_error()
        );
        ensure!(length < 32_768, "Startup target path is too long");
        if length < buffer.len() {
            return Ok(PathBuf::from(String::from_utf16(&buffer[..length])?));
        }
        buffer.resize(length + 1, 0);
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    let left = left.as_os_str().encode_wide().collect::<Vec<_>>();
    let right = right.as_os_str().encode_wide().collect::<Vec<_>>();
    unsafe { CompareStringOrdinal(&left, &right, true) == CSTR_EQUAL }
}

#[derive(Clone, Copy)]
enum Namespace {
    Drive,
    Volume,
}

struct Input {
    namespace: Namespace,
    root: PathBuf,
    path: PathBuf,
    parts: Vec<String>,
}

impl Input {
    fn parse(path: &Path) -> Result<Self> {
        let raw = path.to_str().context("Startup path is not valid Unicode")?;
        ensure!(
            !raw.chars().any(|character| character < ' '),
            "Startup path contains control characters"
        );
        let (namespace, root, tail) = if let Some(rest) = raw.strip_prefix(r"\\?\Volume{") {
            let (guid, tail) = rest
                .split_once("}\\")
                .context("Invalid startup volume path")?;
            ensure!(valid_guid(guid), "Invalid startup volume identifier");
            (Namespace::Volume, format!(r"\\?\Volume{{{guid}}}\"), tail)
        } else {
            let raw = raw.strip_prefix(r"\\?\").unwrap_or(raw);
            let bytes = raw.as_bytes();
            ensure!(
                bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1..3] == *b":\\",
                "Startup path must use a local drive or volume identifier"
            );
            (
                Namespace::Drive,
                format!(r"\\?\{}:\", bytes[0] as char),
                &raw[3..],
            )
        };
        let parts = if tail.is_empty() {
            Vec::new()
        } else {
            tail.split('\\')
                .map(|part| {
                    ensure!(
                        !part.is_empty()
                            && part != "."
                            && part != ".."
                            && !part.ends_with([' ', '.'])
                            && !part.contains(['<', '>', ':', '"', '/', '|', '?', '*'])
                            && !reserved_name(part),
                        "Startup path has an ambiguous filename"
                    );
                    Ok(part.to_owned())
                })
                .collect::<Result<Vec<_>>>()?
        };
        let root = PathBuf::from(root);
        let path = parts
            .iter()
            .fold(root.clone(), |path, part| path.join(part));
        Ok(Self {
            namespace,
            root,
            path,
            parts,
        })
    }
}

fn valid_guid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn reserved_name(value: &str) -> bool {
    let stem = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|suffix| {
            matches!(
                suffix,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    })
}

#[cfg(test)]
mod tests;
