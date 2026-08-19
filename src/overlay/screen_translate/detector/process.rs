use std::ffi::c_void;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result, anyhow, bail};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject,
};
use windows::core::PCWSTR;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(super) struct LaunchResources {
    pub(super) detector: crate::component_registry::screen_text_detector::DetectorUse,
    pub(super) runtime: crate::component_registry::local_asr::OnnxRuntimeUse,
    _vc: crate::component_registry::vc_runtime::VcRuntimeUse,
}

impl LaunchResources {
    pub(super) fn ensure(cancelled: &AtomicBool) -> Result<Self> {
        let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(
            &crate::component_registry::screen_text_detector::localized_name(),
        );
        let vc = crate::component_registry::vc_runtime::ensure_component(|done, total| {
            badge.report(done.saturating_mul(10), total.saturating_mul(100));
        })?;
        let runtime =
            crate::component_registry::local_asr::ensure_runtime(cancelled, |done, total| {
                badge.report(
                    total
                        .saturating_mul(10)
                        .saturating_add(done.saturating_mul(50)),
                    total.saturating_mul(100),
                );
            })?;
        let detector =
            crate::component_registry::screen_text_detector::ensure(cancelled, |done, total| {
                badge.report(
                    total
                        .saturating_mul(60)
                        .saturating_add(done.saturating_mul(40)),
                    total.saturating_mul(100),
                );
            })?;
        badge.finish();
        Ok(Self {
            detector,
            runtime,
            _vc: vc,
        })
    }
}

pub(super) fn spawn_worker(resources: &LaunchResources) -> Result<Child> {
    let executable = canonical_file(resources.detector.executable(), "worker")?;
    let runtime = canonical_dir(resources.runtime.bin_dir(), "ONNX runtime")?;
    let vc = canonical_dir(resources._vc.bin_dir(), "VC runtime")?;
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| anyhow!("SystemRoot is unavailable"))?;
    let system32 = canonical_dir(&system_root.join("System32"), "Windows System32")?;
    let path = std::env::join_paths([runtime.as_path(), vc.as_path(), system32.as_path()])?;
    let temp = std::env::temp_dir();
    let workspace = crate::component_registry::worker_workspace(
        crate::component_registry::screen_text_detector::ID,
    )?;

    let mut command = Command::new(&executable);
    command
        .arg("--stdio")
        .current_dir(workspace)
        .env_clear()
        .env("SystemRoot", &system_root)
        .env("WINDIR", &system_root)
        .env("PATH", path)
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);
    command
        .spawn()
        .with_context(|| format!("start text detector worker '{}'", executable.display()))
}

pub(super) fn create_kill_on_close_job(child: &Child) -> Result<OwnedHandle> {
    let raw = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
        .context("create text detector worker job")?;
    let job = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        SetInformationJobObject(
            HANDLE(job.as_raw_handle()),
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .context("configure text detector worker job")?;
        AssignProcessToJobObject(HANDLE(job.as_raw_handle()), HANDLE(child.as_raw_handle()))
            .context("contain text detector worker process")?;
    }
    Ok(job)
}

pub(super) fn terminate_job(job: &OwnedHandle) {
    let _ = unsafe { TerminateJobObject(HANDLE(job.as_raw_handle()), 1) };
}

fn canonical_file(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize text detector {label} '{}'", path.display()))?;
    let metadata = std::fs::symlink_metadata(&canonical)?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        bail!("text detector {label} path is unsafe");
    }
    Ok(canonical)
}

fn canonical_dir(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize {label} directory '{}'", path.display()))?;
    let metadata = std::fs::symlink_metadata(&canonical)?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        bail!("{label} directory is unsafe");
    }
    Ok(canonical)
}

fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt as _;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_environment_path_has_only_owned_and_system_roots() {
        let roots = [
            Path::new(r"C:\components\onnx"),
            Path::new(r"C:\components\vc"),
            Path::new(r"C:\Windows\System32"),
        ];
        assert_eq!(
            std::env::split_paths(&std::env::join_paths(roots).unwrap()).count(),
            3
        );
    }
}
