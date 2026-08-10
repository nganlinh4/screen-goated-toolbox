use std::fs;
use std::io::{Read as _, Write as _};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const MAX_LISTING_BYTES: u64 = 1024 * 1024;
const MAX_CURSOR_BYTES: u64 = 8 * 1024 * 1024;
const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) fn list_entries(
    archive: &Path,
    stop_signal: &AtomicBool,
) -> Result<Vec<String>, String> {
    let mut child = hidden_command()
        .arg("-tf")
        .arg(archive)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start Windows archive reader: {error}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Windows archive reader did not expose output".to_string())?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .by_ref()
            .take(MAX_LISTING_BYTES + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let status = wait_for_child(&mut child, stop_signal);
    let bytes = reader
        .join()
        .map_err(|_| "Windows archive listing reader panicked".to_string())?
        .map_err(|error| format!("Could not read archive listing: {error}"))?;
    status?;
    if bytes.len() as u64 > MAX_LISTING_BYTES {
        return Err("Archive listing is larger than allowed".to_string());
    }
    let listing = String::from_utf8(bytes)
        .map_err(|_| "Archive listing contains unsupported file names".to_string())?;
    Ok(listing
        .lines()
        .filter(|entry| !entry.trim().is_empty())
        .map(str::to_string)
        .collect())
}

pub(super) fn extract_entry(
    archive: &Path,
    entry: &str,
    destination: &Path,
    stop_signal: &AtomicBool,
) -> Result<(), String> {
    let temporary = destination.with_extension("tmp");
    prepare_temporary(&temporary)?;
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("Could not create cursor staging file: {error}"))?;
    let mut child = match hidden_command()
        .arg("-xOf")
        .arg(archive)
        .arg("--")
        .arg(entry)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            return Err(format!("Could not start Windows archive reader: {error}"));
        }
    };
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        let _ = fs::remove_file(&temporary);
        return Err("Windows archive reader did not expose file data".to_string());
    };
    let writer = std::thread::spawn(move || -> std::io::Result<u64> {
        let mut output = output;
        let copied = std::io::copy(&mut stdout.by_ref().take(MAX_CURSOR_BYTES + 1), &mut output)?;
        output.flush()?;
        output.sync_all()?;
        Ok(copied)
    });
    let status = wait_for_child(&mut child, stop_signal);
    let copied = match writer.join() {
        Ok(Ok(copied)) => copied,
        Ok(Err(error)) => {
            let _ = fs::remove_file(&temporary);
            return Err(format!("Could not stage cursor file: {error}"));
        }
        Err(_) => {
            let _ = fs::remove_file(&temporary);
            return Err("Windows archive extraction reader panicked".to_string());
        }
    };
    if let Err(error) = status {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    if copied == 0 || copied > MAX_CURSOR_BYTES {
        let _ = fs::remove_file(&temporary);
        return Err("Extracted cursor file has an invalid size".to_string());
    }
    if destination.exists() {
        let _ = fs::remove_file(&temporary);
        return Err("Cursor destination unexpectedly already exists".to_string());
    }
    fs::rename(&temporary, destination).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("Could not finalize cursor file: {error}")
    })
}

fn prepare_temporary(path: &Path) -> Result<(), String> {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };
    use std::os::windows::fs::MetadataExt as _;
    if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
        return Err("Cursor staging path is not a regular file".to_string());
    }
    fs::remove_file(path).map_err(|error| format!("Could not clear cursor staging file: {error}"))
}

fn hidden_command() -> Command {
    use std::os::windows::process::CommandExt as _;
    let mut command = Command::new("tar.exe");
    command.creation_flags(0x0800_0000);
    command
}

fn wait_for_child(child: &mut Child, stop_signal: &AtomicBool) -> Result<(), String> {
    let started = Instant::now();
    loop {
        if stop_signal.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Cursor archive extraction was cancelled".to_string());
        }
        if started.elapsed() >= PROCESS_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Cursor archive extraction timed out".to_string());
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(format!("Windows archive reader failed with {status}"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Could not wait for Windows archive reader: {error}"
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_and_extracts_only_the_selected_entry() {
        let root = std::env::temp_dir().join(format!(
            "sgt-bsdtar-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        fs::create_dir_all(source.join("pack")).unwrap();
        fs::write(source.join("pack/Arrow.cur"), b"cursor-bytes").unwrap();
        fs::write(source.join("ignored.txt"), b"ignored").unwrap();
        let archive = root.join("fixture.tar");
        let status = hidden_command()
            .args(["-cf"])
            .arg(&archive)
            .arg("-C")
            .arg(&source)
            .arg(".")
            .status()
            .unwrap();
        assert!(status.success());

        let stop = AtomicBool::new(false);
        let entries = list_entries(&archive, &stop).unwrap();
        let selected = entries
            .iter()
            .find(|entry| entry.ends_with("pack/Arrow.cur"))
            .unwrap();
        let output = root.join("Arrow.cur");
        extract_entry(&archive, selected, &output, &stop).unwrap();
        assert_eq!(fs::read(&output).unwrap(), b"cursor-bytes");

        fs::remove_file(output).unwrap();
        fs::remove_file(archive).unwrap();
        fs::remove_file(source.join("pack/Arrow.cur")).unwrap();
        fs::remove_file(source.join("ignored.txt")).unwrap();
        fs::remove_dir(source.join("pack")).unwrap();
        fs::remove_dir(source).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
