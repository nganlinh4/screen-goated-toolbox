//! Acknowledged activation handoff from a later launch to the running instance.
//!
//! The running instance owns one same-user pipe instance and admits each
//! request into its own activation queue before replying, so a launch at a
//! different integrity level needs no access to that instance's private
//! objects or TEMP directory, and success means the request was accepted.

use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use windows::Win32::Foundation::{ERROR_PIPE_CONNECTED, HANDLE};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FlushFileBuffers, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE, PIPE_WAIT, SetNamedPipeHandleState,
    WaitNamedPipeW,
};
use windows::core::PCWSTR;

use super::instance_security::ObjectSecurity;

const MAGIC: &[u8] = b"SGT-ACTIVATION-1";
const RESTORE: u8 = 0;
const OPEN_FILE: u8 = 1;
const MAX_PATH_UNITS: usize = 32_767;
const MAX_REQUEST_BYTES: usize = MAGIC.len() + 1 + MAX_PATH_UNITS * 2;
const ACCEPTED: u8 = 1;
const REJECTED: u8 = 0;
const ERROR_FILE_NOT_FOUND: i32 = 2;
const ERROR_PIPE_BUSY: i32 = 231;

pub(super) fn encode_request(file: Option<&Path>) -> io::Result<Vec<u8>> {
    let mut request = MAGIC.to_vec();
    let Some(file) = file else {
        request.push(RESTORE);
        return Ok(request);
    };
    let units = file.as_os_str().encode_wide().collect::<Vec<_>>();
    if units.is_empty() || units.len() > MAX_PATH_UNITS || units.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "file path cannot be handed to the running instance",
        ));
    }
    request.push(OPEN_FILE);
    request.extend(units.iter().flat_map(|unit| unit.to_le_bytes()));
    Ok(request)
}

/// `None` rejects the request; `Some(None)` restores the window.
pub(super) fn decode_request(request: &[u8]) -> Option<Option<PathBuf>> {
    let body = request.strip_prefix(MAGIC)?;
    let (&kind, path) = body.split_first()?;
    match kind {
        RESTORE if path.is_empty() => Some(None),
        OPEN_FILE if !path.is_empty() && path.len() % 2 == 0 => {
            let units = path
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
                .collect::<Vec<_>>();
            if units.len() > MAX_PATH_UNITS || units.contains(&0) {
                return None;
            }
            let path = PathBuf::from(std::ffi::OsString::from_wide(&units));
            path.is_absolute().then_some(Some(path))
        }
        _ => None,
    }
}

/// Create the only server instance now, so a squatting or second server fails
/// before this process announces itself, then serve requests on one thread.
pub(super) fn start_server(
    name: &str,
    security: &ObjectSecurity,
    accept: impl Fn(Option<PathBuf>) -> bool + Send + 'static,
) -> Result<()> {
    let wide = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let attributes = security.attributes();
    let raw = unsafe {
        CreateNamedPipeW(
            PCWSTR(wide.as_ptr()),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            64,
            MAX_REQUEST_BYTES as u32,
            0,
            Some(&attributes),
        )
    };
    if raw.is_invalid() {
        return Err(windows::core::Error::from_thread()).context("Create the activation pipe");
    }
    let pipe = unsafe { OwnedHandle::from_raw_handle(raw.0) };
    std::thread::Builder::new()
        .name("sgt-activation-pipe".into())
        .spawn(move || {
            loop {
                if let Err(error) = serve_one(&pipe, &accept) {
                    crate::log_info!("[Activation] Handoff request failed: {error:#}");
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        })
        .context("Start the activation pipe")?;
    Ok(())
}

fn serve_one(pipe: &OwnedHandle, accept: &impl Fn(Option<PathBuf>) -> bool) -> Result<()> {
    let handle = HANDLE(pipe.as_raw_handle());
    match unsafe { ConnectNamedPipe(handle, None) } {
        Ok(()) => {}
        Err(error) if error.code() == ERROR_PIPE_CONNECTED.to_hresult() => {}
        Err(error) => return Err(error).context("Wait for an activation request"),
    }
    let result = answer(handle, accept);
    unsafe {
        let _ = FlushFileBuffers(handle);
        let _ = DisconnectNamedPipe(handle);
    }
    result
}

fn answer(handle: HANDLE, accept: &impl Fn(Option<PathBuf>) -> bool) -> Result<()> {
    let mut buffer = vec![0_u8; MAX_REQUEST_BYTES];
    let mut read = 0;
    // Message mode: an oversized request fails here instead of being split.
    let received = unsafe { ReadFile(handle, Some(&mut buffer), Some(&mut read), None) };
    let accepted = received.is_ok()
        && decode_request(&buffer[..read as usize])
            .is_some_and(|request| request.as_deref().is_none_or(Path::is_file) && accept(request));
    let reply = [if accepted { ACCEPTED } else { REJECTED }];
    unsafe { WriteFile(handle, Some(&reply), None, None) }.context("Reply to activation")?;
    received.context("Read activation request")
}

/// Hand a request to the running instance. The exchange runs on its own thread
/// so an unresponsive owner cannot hold this launch past `timeout`.
pub(super) fn deliver(pipe: &str, request: Vec<u8>, timeout: Duration) -> io::Result<()> {
    let pipe = pipe.to_owned();
    let deadline = Instant::now() + timeout;
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(exchange(&pipe, &request, deadline));
    });
    receiver.recv_timeout(timeout).unwrap_or_else(|_| {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "no activation reply",
        ))
    })
}

fn exchange(pipe: &str, request: &[u8], deadline: Instant) -> io::Result<()> {
    let mut file = loop {
        match OpenOptions::new().read(true).write(true).open(pipe) {
            Ok(file) => break file,
            // The owner may still be between taking the mutex and serving.
            Err(error)
                if error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND)
                    && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error)
                if error.raw_os_error() == Some(ERROR_PIPE_BUSY) && Instant::now() < deadline =>
            {
                let wide = pipe.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
                let remaining = deadline.saturating_duration_since(Instant::now());
                let wait_ms = remaining.as_millis().clamp(1, 1_000) as u32;
                let _ = unsafe { WaitNamedPipeW(PCWSTR(wide.as_ptr()), wait_ms) };
            }
            Err(error) => return Err(error),
        }
    };
    let mode = PIPE_READMODE_MESSAGE;
    unsafe { SetNamedPipeHandleState(HANDLE(file.as_raw_handle()), Some(&mode), None, None) }
        .map_err(io::Error::other)?;
    file.write_all(request)?;
    let mut reply = [REJECTED; 1];
    file.read_exact(&mut reply)?;
    if reply[0] == ACCEPTED {
        Ok(())
    } else {
        Err(io::Error::other(
            "the running instance rejected the request",
        ))
    }
}

#[cfg(test)]
#[path = "instance_channel_tests.rs"]
mod tests;
