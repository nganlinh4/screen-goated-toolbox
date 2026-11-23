//! Bounded, request-owned analysis process.

use crate::overlay::computer_control::shell_job as contained;

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const MAX_STDOUT: u64 = 16 * 1024 * 1024;
const MAX_STDERR: u64 = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(90);

pub(super) fn run(
    command: &mut Command,
    cancel: &AtomicBool,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    if cancel.load(Ordering::SeqCst) {
        return Err("Cancelled".into());
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = contained::ContainedChild::spawn(command).map_err(|error| error.to_string())?;
    let (Some(stdout), Some(stderr)) = (child.take_stdout(), child.take_stderr()) else {
        stop_and_wait(&child);
        return Err("Analysis output pipes unavailable".into());
    };
    let overflow = Arc::new(AtomicBool::new(false));
    let stdout_overflow = overflow.clone();
    let out = match thread::Builder::new()
        .name("video-scan-stdout".into())
        .spawn(move || read_bounded(stdout, MAX_STDOUT, &stdout_overflow))
    {
        Ok(reader) => reader,
        Err(error) => {
            stop_and_wait(&child);
            return Err(error.to_string());
        }
    };
    let stderr_overflow = overflow.clone();
    let err = match thread::Builder::new()
        .name("video-scan-stderr".into())
        .spawn(move || read_bounded(stderr, MAX_STDERR, &stderr_overflow))
    {
        Ok(reader) => reader,
        Err(error) => {
            stop_and_wait(&child);
            let _ = out.join();
            return Err(error.to_string());
        }
    };
    let resume = if cancel.load(Ordering::SeqCst) {
        Err("Cancelled".into())
    } else {
        child.resume().map_err(|error| error.to_string())
    };
    let start = Instant::now();
    let mut stopped = resume.err();
    if stopped.is_some() {
        let _ = child.terminate();
    }
    let status = loop {
        if stopped.is_none()
            && (cancel.load(Ordering::SeqCst)
                || overflow.load(Ordering::SeqCst)
                || start.elapsed() >= TIMEOUT)
        {
            stopped = Some(if cancel.load(Ordering::SeqCst) {
                "Cancelled".to_string()
            } else if overflow.load(Ordering::SeqCst) {
                "Analysis output exceeded its size limit".to_string()
            } else {
                "Analysis timed out".to_string()
            });
            let _ = child.terminate();
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => break Err(error.to_string()),
        }
    };
    // A descendant may retain pipe handles even after yt-dlp exits.
    stop_and_wait(&child);
    drop(child);
    let stdout = out.join().map_err(|_| "Analysis stdout reader failed")??;
    let stderr = err.join().map_err(|_| "Analysis stderr reader failed")??;
    if let Some(reason) = stopped {
        return Err(reason);
    }
    if !status?.success() {
        return Err(super::utils::bounded_tool_error(
            &String::from_utf8_lossy(&stderr),
            "yt-dlp could not analyze this URL",
        ));
    }
    Ok((stdout, stderr))
}

fn stop_and_wait(child: &contained::ContainedChild) {
    let _ = child.terminate();
    // Keep the scan permit until every process in the owned job has exited.
    // If the job cannot be queried, retain ownership rather than admit an
    // unbounded number of uncertain descendants.
    while !matches!(child.active_processes(), Ok(0)) {
        thread::sleep(Duration::from_millis(50));
    }
    let _ = child.release_descendants();
}

fn read_bounded(reader: impl Read, limit: u64, overflow: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > limit {
        overflow.store(true, Ordering::SeqCst);
        return Err("Analysis output exceeded its size limit".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    #[test]
    fn scan_output_is_bounded_without_starting_a_process() {
        let overflow = super::AtomicBool::new(false);
        assert_eq!(
            super::read_bounded(&b"abc"[..], 3, &overflow).unwrap(),
            b"abc"
        );
        assert!(super::read_bounded(&b"abcd"[..], 3, &overflow).is_err());
        assert!(overflow.load(super::Ordering::SeqCst));
    }
}
