//! Bounded asynchronous capture. Detached descendants keep their output pipes
//! drained without retaining their output or blocking the action worker.

use anyhow::{Result, anyhow};
use std::io::{ErrorKind, Read};
use std::process::{ExitStatus, Output};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

pub(super) const MAX_CAPTURE_BYTES: usize = 16 * 1024;
const MAX_READER_SLOTS: usize = 128;
const FINAL_DRAIN_WAIT: Duration = Duration::from_millis(150);
static READER_POOL: LazyLock<Arc<SlotPool>> = LazyLock::new(|| {
    Arc::new(SlotPool {
        in_use: AtomicUsize::new(0),
        limit: MAX_READER_SLOTS,
    })
});

struct SlotPool {
    in_use: AtomicUsize,
    limit: usize,
}

struct Slot(Arc<SlotPool>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.in_use.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(super) struct Reservation {
    remaining: usize,
    pool: Arc<SlotPool>,
}

impl Reservation {
    pub(super) fn acquire() -> Result<Self> {
        Self::acquire_from(&READER_POOL)
    }

    fn acquire_from(pool: &Arc<SlotPool>) -> Result<Self> {
        pool.in_use
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
                count.checked_add(2).filter(|next| *next <= pool.limit)
            })
            .map_err(|_| anyhow!("command output reader capacity is exhausted"))?;
        Ok(Self {
            remaining: 2,
            pool: pool.clone(),
        })
    }

    fn take(&mut self) -> Slot {
        self.remaining -= 1;
        Slot(self.pool.clone())
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.pool.in_use.fetch_sub(self.remaining, Ordering::SeqCst);
    }
}

#[derive(Default)]
struct State {
    bytes: Vec<u8>,
    done: bool,
    failed: bool,
    detached: bool,
}

pub(super) struct CommandCapture {
    stdout: Arc<Mutex<State>>,
    stderr: Arc<Mutex<State>>,
}

pub(super) struct CapturedOutput {
    pub(super) output: Output,
    pub(super) stdout_complete: bool,
    pub(super) stderr_complete: bool,
}

impl CommandCapture {
    pub(super) fn start(
        stdout: impl Read + Send + 'static,
        stderr: impl Read + Send + 'static,
        mut reservation: Reservation,
    ) -> Result<Self> {
        let stdout = start_reader(stdout, reservation.take())?;
        let stderr = start_reader(stderr, reservation.take())?;
        Ok(Self { stdout, stderr })
    }

    pub(super) fn finish(&self, status: ExitStatus, cancel: &AtomicBool) -> CapturedOutput {
        let deadline = Instant::now() + FINAL_DRAIN_WAIT;
        while !cancel.load(Ordering::SeqCst)
            && Instant::now() < deadline
            && (!is_done(&self.stdout) || !is_done(&self.stderr))
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        let (stdout, stdout_complete) = snapshot(&self.stdout);
        let (stderr, stderr_complete) = snapshot(&self.stderr);
        CapturedOutput {
            output: Output {
                status,
                stdout,
                stderr,
            },
            stdout_complete,
            stderr_complete,
        }
    }
}

fn start_reader(mut pipe: impl Read + Send + 'static, slot: Slot) -> Result<Arc<Mutex<State>>> {
    let state = Arc::new(Mutex::new(State::default()));
    let worker_state = state.clone();
    std::thread::Builder::new()
        .name("cc-command-output".into())
        .stack_size(128 * 1024)
        .spawn(move || {
            let _slot = slot;
            let mut buffer = [0_u8; 8192];
            loop {
                match pipe.read(&mut buffer) {
                    Ok(0) => {
                        lock(&worker_state).done = true;
                        break;
                    }
                    Ok(count) => {
                        let mut state = lock(&worker_state);
                        if !state.detached {
                            let remaining =
                                (MAX_CAPTURE_BYTES + 1).saturating_sub(state.bytes.len());
                            state
                                .bytes
                                .extend_from_slice(&buffer[..count.min(remaining)]);
                        }
                    }
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => {
                        let mut state = lock(&worker_state);
                        state.done = true;
                        state.failed = true;
                        break;
                    }
                }
            }
        })
        .map_err(|error| anyhow!("start command output reader: {error}"))?;
    Ok(state)
}

fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn is_done(state: &Mutex<State>) -> bool {
    lock(state).done
}

fn snapshot(state: &Mutex<State>) -> (Vec<u8>, bool) {
    let mut state = lock(state);
    state.detached = true;
    (
        std::mem::take(&mut state.bytes),
        state.done && !state.failed,
    )
}

#[cfg(test)]
impl CapturedOutput {
    pub(super) fn complete(output: Output) -> Self {
        Self {
            output,
            stdout_complete: true,
            stderr_complete: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::computer_control::executor::shell::job;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use std::process::{Command, Stdio};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
    };

    #[test]
    fn capacity_failure_is_explicit_and_recovers() {
        const CHILD: &str = "SGT_CC_CAPTURE_CAPACITY_TEST";
        if std::env::var_os(CHILD).is_none() {
            let result = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "overlay::computer_control::executor::shell::capture::tests::capacity_failure_is_explicit_and_recovers", "--nocapture"])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            return;
        }
        let mut reservations = Vec::new();
        while let Ok(reservation) = Reservation::acquire() {
            reservations.push(reservation);
        }
        let result = super::super::run_command_cancellable(
            &serde_json::json!({"command": "Write-Output example"}),
            Duration::from_secs(5),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(result["status"], "resource_unavailable");
        assert_eq!(result["retryable"], true);
        assert_eq!(result["effect_may_have_occurred"], false);
        drop(reservations.pop());
        let recovered = super::super::run_command_cancellable(
            &serde_json::json!({"command": "Write-Output example"}),
            Duration::from_secs(5),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(recovered["ok"], true);
    }

    #[test]
    fn retained_child_pipe_exhausts_then_releases_bounded_capacity() {
        let pool = Arc::new(SlotPool {
            in_use: AtomicUsize::new(0),
            limit: 2,
        });
        let reservation = Reservation::acquire_from(&pool).unwrap();
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$s = [Diagnostics.ProcessStartInfo]::new((Join-Path $PSHOME 'powershell.exe'), '-NoProfile -Command Start-Sleep -Seconds 30'); $s.UseShellExecute = $false; $p = [Diagnostics.Process]::Start($s); [Console]::Out.Write($p.Id)",
        ]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = job::ContainedChild::spawn(&mut command).unwrap();
        let capture = CommandCapture::start(
            child.take_stdout().unwrap(),
            child.take_stderr().unwrap(),
            reservation,
        )
        .unwrap();
        child.resume().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline, "parent process did not exit");
            std::thread::sleep(Duration::from_millis(10));
        };
        let output = capture.finish(status, &AtomicBool::new(false));
        let pid: u32 = String::from_utf8(output.output.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let raw =
            unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE, false, pid) }.unwrap();
        let process = unsafe { OwnedHandle::from_raw_handle(raw.0) };
        child.release_descendants().unwrap();
        drop(child);
        let incomplete = !output.stdout_complete || !output.stderr_complete;
        let exhausted = Reservation::acquire_from(&pool).is_err();
        let _ = unsafe { TerminateProcess(HANDLE(process.as_raw_handle()), 1) };
        let _ = unsafe { WaitForSingleObject(HANDLE(process.as_raw_handle()), 5000) };
        assert!(incomplete);
        assert!(exhausted);
        let deadline = Instant::now() + Duration::from_secs(5);
        while pool.in_use.load(Ordering::SeqCst) != 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(pool.in_use.load(Ordering::SeqCst), 0);
        assert!(Reservation::acquire_from(&pool).is_ok());
    }
}
