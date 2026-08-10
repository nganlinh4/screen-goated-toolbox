use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const FFMPEG_ENV: &str = "SGT_FFMPEG_PATH";
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
    unsafe {
        std::env::remove_var(FFMPEG_ENV);
    }
    let configured = configured.ok_or(FfmpegCapabilityError::Missing)?;
    validate_and_lock(&configured)
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
}
