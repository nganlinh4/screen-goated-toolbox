//! Producers publish complete request files by renaming a sibling temporary file
//! to `request.json`. The consumer claims that name before reading or removing it.
use std::collections::VecDeque;
use std::io::{self, Read};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{
    Condvar, Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};

static NEXT_CLAIM: AtomicU64 = AtomicU64::new(0);
const MAX_REQUEST_BYTES: u64 = 1024 * 1024;
const MAX_UNREAD_REQUESTS: usize = 32;
static VALIDATION: Mutex<bool> = Mutex::new(false);
static VALIDATION_READY: Condvar = Condvar::new();

pub(super) struct Validation;

pub(super) fn begin_validation() -> Validation {
    let mut state = validation_permit();
    *state = true;
    Validation
}

pub(super) fn validation_permit() -> MutexGuard<'static, bool> {
    let mut state = VALIDATION.lock().unwrap_or_else(|error| error.into_inner());
    while *state {
        state = VALIDATION_READY
            .wait(state)
            .unwrap_or_else(|error| error.into_inner());
    }
    state
}

impl Drop for Validation {
    fn drop(&mut self) {
        let mut state = VALIDATION.lock().unwrap_or_else(|error| error.into_inner());
        *state = false;
        VALIDATION_READY.notify_all();
    }
}

pub(super) struct ClaimedRequest {
    #[cfg(test)]
    directory: PathBuf,
    path: PathBuf,
    owns_file: bool,
    file_identity: Option<String>,
    file_handle: Option<std::fs::File>,
    directory_handle: Option<std::fs::File>,
}

fn file_identity(file: &std::fs::File) -> io::Result<String> {
    crate::overlay::creation_file_identity::from_file(file).map_err(io::Error::other)
}

fn delete_handle(file: &std::fs::File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
    };
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    }
    .map_err(|error| io::Error::from_raw_os_error(error.code().0 & 0xffff))
}

fn rename_handle(file: &std::fs::File, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    let name = target.as_os_str().encode_wide().collect::<Vec<_>>();
    crate::verified_scratch::rename_file_by_handle(file, &name, false)
        .map_err(|error| io::Error::from_raw_os_error(error.code().0 & 0xffff))
}

fn create_owned_directory(parent: &std::fs::File, name: &str) -> io::Result<std::fs::File> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use windows::Wdk::Foundation::OBJECT_ATTRIBUTES;
    use windows::Wdk::Storage::FileSystem::{
        FILE_CREATE, FILE_DIRECTORY_FILE, FILE_SYNCHRONOUS_IO_NONALERT, NtCreateFile,
    };
    use windows::Win32::Foundation::{
        HANDLE, OBJ_CASE_INSENSITIVE, RtlNtStatusToDosError, UNICODE_STRING,
    };
    use windows::Win32::Storage::FileSystem::{
        DELETE, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_SHARE_READ, FILE_SHARE_WRITE,
        SYNCHRONIZE,
    };
    use windows::Win32::System::IO::IO_STATUS_BLOCK;
    let mut wide = name.encode_utf16().collect::<Vec<_>>();
    let length = u16::try_from(wide.len() * 2)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "lab claim name is too long"))?;
    let name = UNICODE_STRING {
        Length: length,
        MaximumLength: length,
        Buffer: windows::core::PWSTR(wide.as_mut_ptr()),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: HANDLE(parent.as_raw_handle()),
        ObjectName: &name,
        Attributes: OBJ_CASE_INSENSITIVE,
        ..Default::default()
    };
    let mut handle = HANDLE::default();
    let mut status = IO_STATUS_BLOCK::default();
    let result = unsafe {
        NtCreateFile(
            &mut handle,
            FILE_GENERIC_READ | DELETE | SYNCHRONIZE,
            &attributes,
            &mut status,
            None,
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            FILE_CREATE,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT,
            None,
            0,
        )
    };
    if result.0 < 0 {
        return Err(io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(result) } as i32,
        ));
    }
    Ok(unsafe { std::fs::File::from_raw_handle(handle.0) })
}

impl ClaimedRequest {
    pub(super) fn read(&self) -> io::Result<Vec<u8>> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&self.path)?;
        if self.file_identity.as_deref() != Some(file_identity(&file)?.as_str()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "lab claim file identity changed",
            ));
        }
        let mut bytes = Vec::new();
        file.take(MAX_REQUEST_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_REQUEST_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "lab request exceeds size limit",
            ));
        }
        Ok(bytes)
    }

    fn try_cleanup(&mut self) -> io::Result<()> {
        if self.owns_file {
            use windows::Win32::Storage::FileSystem::{
                DELETE, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_SHARE_READ,
                FILE_SHARE_WRITE,
            };
            let file = match std::fs::OpenOptions::new()
                .read(true)
                .access_mode(FILE_GENERIC_READ.0 | DELETE.0)
                .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
                .open(&self.path)
            {
                Ok(file) => Some(file),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            if let Some(file) = file {
                if self.file_identity.as_deref() != Some(file_identity(&file)?.as_str()) {
                    return Err(io::Error::other("lab claim file identity changed"));
                }
                delete_handle(&file)?;
            }
            self.owns_file = false;
            self.file_handle = None;
        }
        if let Some(directory) = &self.directory_handle {
            delete_handle(directory)?;
            self.directory_handle = None;
        }
        Ok(())
    }
}

impl Drop for ClaimedRequest {
    fn drop(&mut self) {
        if let Err(error) = self.try_cleanup() {
            crate::log_info!("[Screen Translate] lab claim cleanup failed: {error}");
        }
    }
}

fn claim(
    queue: &Path,
    on_failed: impl FnOnce(ClaimedRequest),
) -> io::Result<Option<ClaimedRequest>> {
    claim_using(
        queue,
        |file, _, target| rename_handle(file, target),
        on_failed,
    )
}

fn claim_using(
    queue: &Path,
    rename: impl FnOnce(&std::fs::File, &Path, &Path) -> io::Result<()>,
    on_failed: impl FnOnce(ClaimedRequest),
) -> io::Result<Option<ClaimedRequest>> {
    use windows::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let source = match std::fs::OpenOptions::new()
        .read(true)
        .access_mode(FILE_GENERIC_READ.0 | DELETE.0)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(queue.join("request.json"))
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let source_identity = file_identity(&source)?;
    let parent = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(queue)?;
    let mut selected = None;
    for _ in 0..128 {
        let id = NEXT_CLAIM.fetch_add(1, Ordering::Relaxed);
        let name = format!(".request-{}-{id}", std::process::id());
        let directory = queue.join(&name);
        match create_owned_directory(&parent, &name) {
            Ok(handle) => {
                selected = Some((directory, handle));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    let (directory, handle) = selected.ok_or_else(|| {
        io::Error::new(io::ErrorKind::AlreadyExists, "could not allocate lab claim")
    })?;
    let mut claimed = ClaimedRequest {
        path: directory.join("request.json"),
        #[cfg(test)]
        directory,
        owns_file: false,
        file_identity: Some(source_identity.clone()),
        file_handle: None,
        directory_handle: Some(handle),
    };
    match rename(&source, &queue.join("request.json"), &claimed.path) {
        Ok(()) => {
            claimed.owns_file = true;
            let anchor = std::fs::OpenOptions::new()
                .access_mode(FILE_READ_ATTRIBUTES.0)
                .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
                .open(&claimed.path);
            let Ok(anchor) = anchor else {
                claimed.owns_file = false;
                on_failed(claimed);
                return Err(io::Error::other(
                    "lab request identity could not be retained",
                ));
            };
            if !matches!(file_identity(&anchor), Ok(ref target) if *target == source_identity) {
                claimed.owns_file = false;
                on_failed(claimed);
                return Err(io::Error::other("lab request changed during claim"));
            }
            claimed.file_handle = Some(anchor);
            Ok(Some(claimed))
        }
        Err(error) => {
            on_failed(claimed);
            if error.kind() == io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(error)
            }
        }
    }
}

#[derive(Default)]
pub(super) struct PendingRequests {
    unread: VecDeque<ClaimedRequest>,
    ready: Option<(ClaimedRequest, serde_json::Value)>,
    retired: VecDeque<ClaimedRequest>,
}

impl PendingRequests {
    pub(super) fn poll(
        &mut self,
        queue: &Path,
        parse: impl Fn(&[u8]) -> Option<serde_json::Value>,
    ) {
        self.retired
            .retain_mut(|request| request.try_cleanup().is_err());
        if queue.join("request.json").is_file()
            && let Ok(Some(request)) = claim(queue, |failed| self.retire(failed))
        {
            if self.unread.len() == MAX_UNREAD_REQUESTS {
                if let Some(retired) = self.unread.pop_front() {
                    self.retire(retired);
                }
                crate::log_info!("[Screen Translate] lab unread request limit reached");
            }
            self.unread.push_back(request);
        }
        let mut retry = VecDeque::new();
        while let Some(request) = self.unread.pop_front() {
            match request.read() {
                Ok(bytes) => {
                    if let Some(value) = parse(&bytes) {
                        // Any uncertain request encountered earlier cannot
                        // supersede this later, successfully parsed request.
                        for retired in retry.drain(..) {
                            self.retire(retired);
                        }
                        if let Some((previous, _)) = self.ready.replace((request, value)) {
                            self.retire(previous);
                        }
                    } else {
                        self.retire(request);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::InvalidData => self.retire(request),
                Err(_) => retry.push_back(request),
            }
        }
        self.unread = retry;
    }

    pub(super) fn is_ready(&self) -> bool {
        self.ready.is_some()
    }

    pub(super) fn take_ready(&mut self) -> Option<serde_json::Value> {
        let (request, value) = self.ready.take()?;
        self.retire(request);
        Some(value)
    }

    fn retire(&mut self, mut request: ClaimedRequest) {
        if request.try_cleanup().is_ok() {
            return;
        }
        if self.retired.len() == MAX_UNREAD_REQUESTS {
            self.retired.pop_front();
            crate::log_info!("[Screen Translate] lab cleanup retry limit reached");
        }
        self.retired.push_back(request);
    }
}

#[cfg(test)]
#[path = "lab_queue_tests.rs"]
mod tests;
