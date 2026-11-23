use crate::component_registry::{canonical_regular_dir, canonical_regular_file};
use crate::process_job::{HelperCommand, OwnedProcess};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result, anyhow, bail};

pub(super) struct LaunchResources {
    pub(super) detector: crate::component_registry::screen_text_detector::DetectorUse,
    pub(super) runtime: crate::component_registry::local_asr::OnnxRuntimeUse,
    pub(super) vc: crate::component_registry::vc_runtime::VcRuntimeUse,
}

impl LaunchResources {
    pub(super) fn ensure(cancelled: &Arc<AtomicBool>) -> Result<Self> {
        crate::log_info!("[Screen Translate] preparing verified component resources");
        let started = std::time::Instant::now();
        let language = crate::APP
            .lock()
            .map(|app| app.config.ui_language.clone())
            .unwrap_or_else(|_| "en".to_string());
        let names = &crate::gui::locale::LocaleText::get(&language)
            .auxiliary
            .managed_tools;
        // Each badge describes actual bytes for its named transfer. Installed
        // dependencies report nothing and must not advance another download.
        let badge =
            crate::overlay::auto_copy_badge::DownloadProgressBadge::new(names.tool_vc_runtime);
        let vc = crate::component_registry::vc_runtime::ensure_component_with_cancel(
            cancelled,
            |done, total| {
                badge.report(done, total);
            },
        )?;
        badge.finish();
        let vc_ms = started.elapsed().as_secs_f64() * 1000.0;
        crate::log_info!("[Screen Translate] VC runtime ready ms={vc_ms:.1}");
        let started = std::time::Instant::now();
        let badge =
            crate::overlay::auto_copy_badge::DownloadProgressBadge::new(names.tool_ai_runtime);
        let runtime =
            crate::component_registry::local_asr::ensure_runtime(cancelled, |done, total| {
                badge.report(done, total);
            })?;
        badge.finish();
        let runtime_ms = started.elapsed().as_secs_f64() * 1000.0;
        crate::log_info!("[Screen Translate] ONNX runtime ready ms={runtime_ms:.1}");
        let started = std::time::Instant::now();
        let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(
            names.tool_screen_translate_detector,
        );
        let detector =
            crate::component_registry::screen_text_detector::ensure(cancelled, |done, total| {
                badge.report(done, total);
            })?;
        crate::log_info!(
            "[Screen Translate] detector_delivery vc_ms={vc_ms:.1} runtime_ms={runtime_ms:.1} models_ms={:.1}",
            started.elapsed().as_secs_f64() * 1000.0
        );
        badge.finish();
        if cancelled.load(Ordering::Acquire) {
            bail!("detector preparation cancelled");
        }
        Ok(Self {
            detector,
            runtime,
            vc,
        })
    }
}

pub(super) fn spawn_worker(
    resources: Arc<LaunchResources>,
    cancelled: &AtomicBool,
) -> Result<OwnedProcess> {
    let executable =
        canonical_regular_file(resources.detector.executable(), "text detector worker")?;
    let runtime = canonical_regular_dir(resources.runtime.bin_dir(), "ONNX runtime")?;
    let vc = canonical_regular_dir(resources.vc.bin_dir(), "VC runtime")?;
    let system_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| anyhow!("SystemRoot is unavailable"))?;
    let system32 = canonical_regular_dir(&system_root.join("System32"), "Windows System32")?;
    let path = worker_search_path(&runtime, &vc, &system32)?;
    let temp = crate::paths::child_process_temp_dir()?;
    let workspace = crate::component_registry::worker_workspace(
        crate::component_registry::screen_text_detector::ID,
    )?;

    let mut command = HelperCommand::new(&executable);
    command
        .arg("--stdio")
        .current_dir(workspace)
        .env_clear()
        .env("SystemRoot", &system_root)
        .env("WINDIR", &system_root)
        .env("PATH", path)
        .env("TEMP", &temp)
        .env("TMP", &temp)
        .pipe_stdin();
    if cancelled.load(Ordering::Acquire) {
        bail!("detector startup cancelled");
    }
    OwnedProcess::spawn_helper(&command, resources)
        .with_context(|| format!("start text detector worker '{}'", executable.display()))
}

/// The worker's DLL search path: the verified ONNX and VC runtimes, then
/// System32, and nothing inherited from the host environment.
fn worker_search_path(runtime: &Path, vc: &Path, system32: &Path) -> Result<OsString> {
    Ok(std::env::join_paths([runtime, vc, system32])?)
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
        let path = worker_search_path(roots[0], roots[1], roots[2]).unwrap();
        assert_eq!(std::env::split_paths(&path).collect::<Vec<_>>(), roots);
    }
}
