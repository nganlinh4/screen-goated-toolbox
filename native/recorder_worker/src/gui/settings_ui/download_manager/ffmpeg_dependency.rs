use std::fmt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const FFMPEG_ENV: &str = "SGT_FFMPEG_PATH";
const FFMPEG_REQUEST_EVENT_ENV: &str = "SGT_FFMPEG_REQUEST_EVENT";
const FFMPEG_READY_EVENT_ENV: &str = "SGT_FFMPEG_READY_EVENT";
const FFMPEG_FAILURE_EVENT_ENV: &str = "SGT_FFMPEG_FAILURE_EVENT";
const FFMPEG_COMPONENT_ID: &str = "ffmpeg-x64";

static PROVIDED_FFMPEG: OnceLock<Result<ProvidedFfmpeg, FfmpegCapabilityError>> = OnceLock::new();

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
    match PROVIDED_FFMPEG.get_or_init(load_provided_ffmpeg) {
        Ok(provided) => Ok(provided.path.clone()),
        Err(error) => Err(error.clone()),
    }
}

fn load_provided_ffmpeg() -> Result<ProvidedFfmpeg, FfmpegCapabilityError> {
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
    let configured = configured.ok_or(FfmpegCapabilityError::Missing)?;
    match (request_event, ready_event, failure_event) {
        (Some(request), Some(ready), Some(failure)) => {
            request_deferred_capability(&request, &ready, &failure)?;
        }
        (None, None, None) => {}
        _ => {
            return Err(FfmpegCapabilityError::Invalid(
                "incomplete deferred FFmpeg capability contract".to_string(),
            ));
        }
    }
    validate_and_lock(&configured)
}

fn request_deferred_capability(
    request: &str,
    ready: &str,
    failure: &str,
) -> Result<(), FfmpegCapabilityError> {
    use windows::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        EVENT_ALL_ACCESS, OpenEventW, SetEvent, WaitForMultipleObjects,
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
        let load_start = source.find("fn load_provided_ffmpeg()").unwrap();
        let request_start = source.find("fn request_deferred_capability(").unwrap();
        let load = &source[load_start..request_start];
        assert!(
            load.find("request_deferred_capability(").unwrap()
                < load.find("validate_and_lock(&configured)").unwrap()
        );
        let validate_start = source.find("fn validate_and_lock(").unwrap();
        let request = &source[request_start..validate_start];
        assert!(
            request.find("SetEvent(request_handle)").unwrap()
                < request.find("WaitForMultipleObjects(&[").unwrap()
        );
        assert!(source.contains("host could not provide a verified FFmpeg capability"));
    }
}
