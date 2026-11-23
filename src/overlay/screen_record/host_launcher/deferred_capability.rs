use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{
    CreateEventW, EVENT_ALL_ACCESS, OpenEventW, ResetEvent, SetEvent, WaitForSingleObject,
};
use windows::core::PCWSTR;

use crate::component_registry::external_tools::ExternalTool;

const REQUEST_EVENT_ENV: &str = "SGT_FFMPEG_REQUEST_EVENT";
const READY_EVENT_ENV: &str = "SGT_FFMPEG_READY_EVENT";
const FAILURE_EVENT_ENV: &str = "SGT_FFMPEG_FAILURE_EVENT";
const FFMPEG_PATH_ENV: &str = "SGT_FFMPEG_PATH";

pub(super) struct DeferredFfmpeg {
    _request_event: OwnedHandle,
    _ready_event: OwnedHandle,
    _failure_event: OwnedHandle,
    request_name: String,
    ready_name: String,
    failure_name: String,
    path: std::path::PathBuf,
    cancelled: Arc<AtomicBool>,
}

impl DeferredFfmpeg {
    pub(super) fn prepare() -> Result<Self> {
        let mut random = [0_u8; 16];
        getrandom::fill(&mut random).context("generate deferred FFmpeg event identity")?;
        let suffix = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let request_name = format!("Local\\SGTRecorderFfmpegRequest-{suffix}");
        let ready_name = format!("Local\\SGTRecorderFfmpegReady-{suffix}");
        let failure_name = format!("Local\\SGTRecorderFfmpegFailure-{suffix}");
        let request_event = create_event(&request_name)?;
        let ready_event = create_event(&ready_name)?;
        let failure_event = create_event(&failure_name)?;
        let path = crate::component_registry::external_tools::expected_executable_path(
            ExternalTool::Ffmpeg,
        )?;
        let cancelled = Arc::new(AtomicBool::new(false));
        spawn_broker(
            [
                request_name.clone(),
                ready_name.clone(),
                failure_name.clone(),
            ],
            Arc::clone(&cancelled),
            |cancelled| {
                crate::component_registry::capabilities::resolve_external_tool_with_badge_before_capture(
                    ExternalTool::Ffmpeg,
                    cancelled,
                )
                .map(drop)
                .map_err(|error| format!("{error:#}"))
            },
        )?;

        Ok(Self {
            _request_event: request_event,
            _ready_event: ready_event,
            _failure_event: failure_event,
            request_name,
            ready_name,
            failure_name,
            path,
            cancelled,
        })
    }

    pub(super) fn configure(&self, command: &mut Command) {
        command
            .env(FFMPEG_PATH_ENV, &self.path)
            .env(REQUEST_EVENT_ENV, &self.request_name)
            .env(READY_EVENT_ENV, &self.ready_name)
            .env(FAILURE_EVENT_ENV, &self.failure_name);
    }
}

impl Drop for DeferredFfmpeg {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Serves the worker's FFmpeg requests for the whole session. A failed
/// resolution answers `failure` and keeps listening, so the worker can retry
/// (for example after the user installs FFmpeg); success is final.
fn spawn_broker(
    [request_name, ready_name, failure_name]: [String; 3],
    cancelled: Arc<AtomicBool>,
    resolve: impl Fn(&AtomicBool) -> std::result::Result<(), String> + Send + 'static,
) -> Result<()> {
    std::thread::Builder::new()
        .name("sgt-recorder-ffmpeg-broker".to_string())
        .spawn(move || {
            let (Ok(request), Ok(ready), Ok(failure)) = (
                open_event(&request_name),
                open_event(&ready_name),
                open_event(&failure_name),
            ) else {
                return;
            };
            let raw = |handle: &OwnedHandle| HANDLE(handle.as_raw_handle());
            while !cancelled.load(Ordering::Acquire) {
                let wait = unsafe { WaitForSingleObject(raw(&request), 250) };
                if wait == WAIT_TIMEOUT {
                    continue;
                }
                if wait != WAIT_OBJECT_0 {
                    return;
                }
                unsafe {
                    let _ = ResetEvent(raw(&request));
                }
                let result = resolve(&cancelled);
                if let Err(error) = &result {
                    crate::log_info!("[ScreenRecord] FFmpeg capability request failed: {error}");
                }
                unsafe {
                    let _ = SetEvent(raw(if result.is_ok() { &ready } else { &failure }));
                }
                if result.is_ok() {
                    return;
                }
            }
        })
        .context("start deferred FFmpeg capability broker")?;
    Ok(())
}

fn create_event(name: &str) -> Result<OwnedHandle> {
    let wide = name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle = unsafe { CreateEventW(None, true, false, PCWSTR(wide.as_ptr())) }
        .with_context(|| format!("create capability event {name}"))?;
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.0) })
}

fn open_event(name: &str) -> Result<OwnedHandle> {
    let wide = name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle = unsafe { OpenEventW(EVENT_ALL_ACCESS, false, PCWSTR(wide.as_ptr())) }
        .with_context(|| format!("open capability event {name}"))?;
    Ok(unsafe { OwnedHandle::from_raw_handle(handle.0) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn wait(handle: &OwnedHandle) -> bool {
        unsafe { WaitForSingleObject(HANDLE(handle.as_raw_handle()), 5_000) == WAIT_OBJECT_0 }
    }

    #[test]
    fn broker_keeps_serving_after_a_failure_and_stops_after_success() {
        let mut random = [0_u8; 8];
        getrandom::fill(&mut random).unwrap();
        let suffix = u64::from_ne_bytes(random);
        let names = ["Request", "Ready", "Failure"]
            .map(|kind| format!(r"Local\SGTRecorderFfmpegTest{kind}-{suffix}"));
        let [request, ready, failure] = names.clone().map(|name| create_event(&name).unwrap());
        let attempts = Arc::new(AtomicUsize::new(0));
        let thread_attempts = Arc::clone(&attempts);
        let cancelled = Arc::new(AtomicBool::new(false));
        spawn_broker(
            names,
            Arc::clone(&cancelled),
            move |_| match thread_attempts.fetch_add(1, Ordering::SeqCst) {
                0 => Err("not installed".to_string()),
                _ => Ok(()),
            },
        )
        .unwrap();

        // The worker side: request, see failure, reset it, and request again.
        unsafe { SetEvent(HANDLE(request.as_raw_handle())).unwrap() };
        assert!(wait(&failure));
        unsafe { ResetEvent(HANDLE(failure.as_raw_handle())).unwrap() };
        unsafe { SetEvent(HANDLE(request.as_raw_handle())).unwrap() };
        assert!(wait(&ready));
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
        cancelled.store(true, Ordering::Release);
    }
}
