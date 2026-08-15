use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

use anyhow::{Context, Result};
use windows::Win32::Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{
    CreateEventW, EVENT_ALL_ACCESS, OpenEventW, SetEvent, WaitForSingleObject,
};
use windows::core::PCWSTR;

use crate::component_registry::external_tools::{ExternalTool, ExternalToolUse};

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
    _resolved: Receiver<Result<ExternalToolUse, String>>,
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
        let (sender, receiver) = mpsc::sync_channel(1);

        let thread_request_name = request_name.clone();
        let thread_ready_name = ready_name.clone();
        let thread_failure_name = failure_name.clone();
        let thread_cancelled = Arc::clone(&cancelled);
        std::thread::Builder::new()
            .name("sgt-recorder-ffmpeg-broker".to_string())
            .spawn(move || {
                let (Ok(request), Ok(ready), Ok(failure)) = (
                    open_event(&thread_request_name),
                    open_event(&thread_ready_name),
                    open_event(&thread_failure_name),
                ) else {
                    return;
                };
                while !thread_cancelled.load(Ordering::Acquire) {
                    let request_handle =
                        windows::Win32::Foundation::HANDLE(request.as_raw_handle());
                    let wait = unsafe { WaitForSingleObject(request_handle, 250) };
                    if wait == WAIT_OBJECT_0 {
                        let result =
                            crate::component_registry::capabilities::resolve_external_tool(
                                ExternalTool::Ffmpeg,
                                &thread_cancelled,
                                |_| {},
                            )
                            .map_err(|error| format!("{error:#}"));
                        let succeeded = result.is_ok();
                        let _ = sender.send(result);
                        let ready_handle =
                            windows::Win32::Foundation::HANDLE(ready.as_raw_handle());
                        let failure_handle =
                            windows::Win32::Foundation::HANDLE(failure.as_raw_handle());
                        unsafe {
                            let _ = SetEvent(if succeeded {
                                ready_handle
                            } else {
                                failure_handle
                            });
                        }
                        return;
                    }
                    if wait != WAIT_TIMEOUT {
                        return;
                    }
                }
            })
            .context("start deferred FFmpeg capability broker")?;

        Ok(Self {
            _request_event: request_event,
            _ready_event: ready_event,
            _failure_event: failure_event,
            request_name,
            ready_name,
            failure_name,
            path,
            cancelled,
            _resolved: receiver,
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
