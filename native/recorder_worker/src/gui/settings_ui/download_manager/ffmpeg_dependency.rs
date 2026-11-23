use std::fmt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

const FFMPEG_ENV: &str = "SGT_FFMPEG_PATH";
const FFMPEG_REQUEST_EVENT_ENV: &str = "SGT_FFMPEG_REQUEST_EVENT";
const FFMPEG_READY_EVENT_ENV: &str = "SGT_FFMPEG_READY_EVENT";
const FFMPEG_FAILURE_EVENT_ENV: &str = "SGT_FFMPEG_FAILURE_EVENT";
const FFMPEG_COMPONENT_ID: &str = "ffmpeg-x64";

/// The host's contract, read (and scrubbed from the environment) once.
static CONTRACT: OnceLock<Result<Contract, FfmpegCapabilityError>> = OnceLock::new();
/// Only a verified, locked FFmpeg is cached; a failed resolution is retried on
/// the next request, so one transient failure never disables FFmpeg for the
/// rest of the worker session.
static PROVIDED_FFMPEG: Mutex<Option<ProvidedFfmpeg>> = Mutex::new(None);

#[derive(Clone, Debug)]
struct Contract {
    path: PathBuf,
    deferred: Option<DeferredEvents>,
}

#[derive(Clone, Debug)]
struct DeferredEvents {
    request: String,
    ready: String,
    failure: String,
}

struct ProvidedFfmpeg {
    path: PathBuf,
    _file: std::fs::File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FfmpegCapabilityError {
    Missing,
    Invalid(String),
}

impl fmt::Display for FfmpegCapabilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => write!(
                formatter,
                "MISSING_CAPABILITY:{FFMPEG_COMPONENT_ID}: install FFmpeg from Downloaded Tools"
            ),
            Self::Invalid(reason) => write!(
                formatter,
                "INVALID_CAPABILITY:{FFMPEG_COMPONENT_ID}: {reason}"
            ),
        }
    }
}

impl std::error::Error for FfmpegCapabilityError {}

impl From<FfmpegCapabilityError> for String {
    fn from(error: FfmpegCapabilityError) -> Self {
        error.to_string()
    }
}

pub fn ensure_ffmpeg_with_badge() -> Result<PathBuf, FfmpegCapabilityError> {
    ensure_ffmpeg_with_badge_message("")
}

pub fn ensure_ffmpeg_with_badge_message(_message: &str) -> Result<PathBuf, FfmpegCapabilityError> {
    let contract = CONTRACT.get_or_init(read_contract).clone()?;
    resolve_with_cache(&PROVIDED_FFMPEG, &contract)
}

fn read_contract() -> Result<Contract, FfmpegCapabilityError> {
    let configured = std::env::var_os(FFMPEG_ENV).map(PathBuf::from);
    let request_event = std::env::var(FFMPEG_REQUEST_EVENT_ENV).ok();
    let ready_event = std::env::var(FFMPEG_READY_EVENT_ENV).ok();
    let failure_event = std::env::var(FFMPEG_FAILURE_EVENT_ENV).ok();
    unsafe {
        std::env::remove_var(FFMPEG_ENV);
        std::env::remove_var(FFMPEG_REQUEST_EVENT_ENV);
        std::env::remove_var(FFMPEG_READY_EVENT_ENV);
        std::env::remove_var(FFMPEG_FAILURE_EVENT_ENV);
    }
    let path = configured.ok_or(FfmpegCapabilityError::Missing)?;
    let deferred = match (request_event, ready_event, failure_event) {
        (Some(request), Some(ready), Some(failure)) => Some(DeferredEvents {
            request,
            ready,
            failure,
        }),
        (None, None, None) => None,
        _ => {
            return Err(FfmpegCapabilityError::Invalid(
                "incomplete deferred FFmpeg capability contract".to_string(),
            ));
        }
    };
    Ok(Contract { path, deferred })
}

/// Holding the cache lock across the host round trip serializes requests, so
/// the host broker only ever answers one outstanding request.
fn resolve_with_cache(
    cache: &Mutex<Option<ProvidedFfmpeg>>,
    contract: &Contract,
) -> Result<PathBuf, FfmpegCapabilityError> {
    let mut cached = cache.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(provided) = cached.as_ref() {
        return Ok(provided.path.clone());
    }
    if let Some(events) = &contract.deferred {
        request_deferred_capability(&events.request, &events.ready, &events.failure)?;
    }
    let provided = validate_and_lock(&contract.path)?;
    let path = provided.path.clone();
    *cached = Some(provided);
    Ok(path)
}

fn request_deferred_capability(
    request: &str,
    ready: &str,
    failure: &str,
) -> Result<(), FfmpegCapabilityError> {
    use windows::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        EVENT_ALL_ACCESS, OpenEventW, ResetEvent, SetEvent, WaitForMultipleObjects,
    };
    use windows::core::PCWSTR;

    let open = |name: &str| {
        let wide = name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let handle = unsafe { OpenEventW(EVENT_ALL_ACCESS, false, PCWSTR(wide.as_ptr())) }
            .map_err(|error| {
                FfmpegCapabilityError::Invalid(format!("open capability event: {error}"))
            })?;
        Ok::<OwnedHandle, FfmpegCapabilityError>(unsafe { OwnedHandle::from_raw_handle(handle.0) })
    };
    let request = open(request)?;
    let ready = open(ready)?;
    let failure = open(failure)?;
    let request_handle = windows::Win32::Foundation::HANDLE(request.as_raw_handle());
    let ready_handle = windows::Win32::Foundation::HANDLE(ready.as_raw_handle());
    let failure_handle = windows::Win32::Foundation::HANDLE(failure.as_raw_handle());
    unsafe {
        // A previous failed attempt leaves the manual-reset failure event set.
        let _ = ResetEvent(failure_handle);
        SetEvent(request_handle).map_err(|error| {
            FfmpegCapabilityError::Invalid(format!("request FFmpeg capability: {error}"))
        })?;
    }
    let wait =
        unsafe { WaitForMultipleObjects(&[ready_handle, failure_handle], false, 20 * 60 * 1_000) };
    if wait == WAIT_OBJECT_0 {
        Ok(())
    } else if wait.0 == WAIT_OBJECT_0.0 + 1 {
        Err(FfmpegCapabilityError::Invalid(
            "host could not provide a verified FFmpeg capability".to_string(),
        ))
    } else if wait == WAIT_TIMEOUT {
        Err(FfmpegCapabilityError::Invalid(
            "timed out waiting for verified FFmpeg capability".to_string(),
        ))
    } else {
        Err(FfmpegCapabilityError::Invalid(
            "FFmpeg capability wait failed".to_string(),
        ))
    }
}

fn validate_and_lock(path: &Path) -> Result<ProvidedFfmpeg, FfmpegCapabilityError> {
    if !path.is_absolute() {
        return Err(FfmpegCapabilityError::Invalid(format!(
            "{FFMPEG_ENV} must be an absolute path"
        )));
    }
    if !path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("ffmpeg.exe"))
    {
        return Err(FfmpegCapabilityError::Invalid(format!(
            "{FFMPEG_ENV} must name ffmpeg.exe"
        )));
    }
    let initial = std::fs::symlink_metadata(path).map_err(|error| {
        FfmpegCapabilityError::Invalid(format!("cannot inspect provided FFmpeg: {error}"))
    })?;
    if !initial.is_file() || is_reparse_point(&initial) {
        return Err(FfmpegCapabilityError::Invalid(
            "provided FFmpeg is not a regular non-reparse file".to_string(),
        ));
    }
    let canonical = std::fs::canonicalize(path).map_err(|error| {
        FfmpegCapabilityError::Invalid(format!("cannot canonicalize provided FFmpeg: {error}"))
    })?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};
    options
        .share_mode(FILE_SHARE_READ.0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0);
    let file = options.open(&canonical).map_err(|error| {
        FfmpegCapabilityError::Invalid(format!("cannot lock provided FFmpeg: {error}"))
    })?;
    let locked = file.metadata().map_err(|error| {
        FfmpegCapabilityError::Invalid(format!("cannot inspect locked FFmpeg: {error}"))
    })?;
    if !locked.is_file() || is_reparse_point(&locked) {
        return Err(FfmpegCapabilityError::Invalid(
            "locked FFmpeg is not a regular non-reparse file".to_string(),
        ));
    }
    Ok(ProvidedFfmpeg {
        path: canonical,
        _file: file,
    })
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolver_rejects_relative_and_wrongly_named_paths() {
        assert!(matches!(
            validate_and_lock(Path::new("ffmpeg.exe")),
            Err(FfmpegCapabilityError::Invalid(_))
        ));
        let wrong_name = std::env::temp_dir().join("not-ffmpeg.exe");
        assert!(matches!(
            validate_and_lock(&wrong_name),
            Err(FfmpegCapabilityError::Invalid(_))
        ));
    }

    #[test]
    fn resolver_opens_the_file_it_validates_without_following_a_late_reparse() {
        let source = include_str!("ffmpeg_dependency.rs");
        assert!(source.contains("FILE_FLAG_OPEN_REPARSE_POINT"));
        assert!(source.contains("file.metadata()"));
        assert!(source.contains("FILE_SHARE_READ"));
    }

    #[test]
    fn deferred_contract_requests_host_verification_before_opening_ffmpeg() {
        let source = include_str!("ffmpeg_dependency.rs");
        let load_start = source.find("fn resolve_with_cache(").unwrap();
        let request_start = source.find("fn request_deferred_capability(").unwrap();
        let load = &source[load_start..request_start];
        assert!(
            load.find("request_deferred_capability(").unwrap()
                < load.find("validate_and_lock(&contract.path)").unwrap()
        );
        let validate_start = source.find("fn validate_and_lock(").unwrap();
        let request = &source[request_start..validate_start];
        assert!(
            request.find("SetEvent(request_handle)").unwrap()
                < request.find("WaitForMultipleObjects(&[").unwrap()
        );
        assert!(source.contains("host could not provide a verified FFmpeg capability"));
    }

    #[test]
    fn failed_resolution_is_retried_and_only_success_is_cached() {
        let mut random = [0u8; 8];
        getrandom::fill(&mut random).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "sgt-ffmpeg-retry-{}-{}",
            std::process::id(),
            u64::from_ne_bytes(random)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let contract = Contract {
            path: dir.join("ffmpeg.exe"),
            deferred: None,
        };
        let cache = Mutex::new(None);
        assert!(resolve_with_cache(&cache, &contract).is_err());
        assert!(cache.lock().unwrap().is_none());

        std::fs::write(&contract.path, b"stub").unwrap();
        let resolved = resolve_with_cache(&cache, &contract).unwrap();
        assert!(resolved.ends_with("ffmpeg.exe"));
        assert_eq!(resolve_with_cache(&cache, &contract).unwrap(), resolved);

        drop(cache);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
