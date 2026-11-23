use crate::overlay::creation_process::CreationProcess;
use std::io::{Read as _, Write as _};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

pub(super) struct BoundedOutput {
    pub(super) status: ExitStatus,
    pub(super) bytes: Vec<u8>,
    pub(super) truncated: bool,
}

struct RuntimeProcessGuard {
    pid: u32,
}

impl Drop for RuntimeProcessGuard {
    fn drop(&mut self) {
        super::unregister_runtime_process(self.pid);
    }
}

fn terminate_and_reap(child: &mut CreationProcess) {
    let _ = child.terminate();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) && matches!(child.active_processes(), Ok(0)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub(super) fn run(
    command: &mut Command,
    input: Option<&[u8]>,
    timeout: Duration,
    maximum_output_bytes: usize,
) -> Option<BoundedOutput> {
    run_cancellable(command, input, timeout, maximum_output_bytes, || false)
}

pub(super) fn run_cancellable(
    command: &mut Command,
    input: Option<&[u8]>,
    timeout: Duration,
    maximum_output_bytes: usize,
    cancelled: impl Fn() -> bool,
) -> Option<BoundedOutput> {
    if maximum_output_bytes == 0
        || timeout.is_zero()
        || super::runtime_shutting_down()
        || cancelled()
    {
        return None;
    }
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    command.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    super::hide_command_window(command);
    let component_lease = crate::component_registry::acquire("creation-windows").ok()?;
    let deadline = Instant::now() + timeout;
    let mut child = CreationProcess::spawn(command, component_lease).ok()?;
    if !super::register_runtime_process(&child) {
        terminate_and_reap(&mut child);
        return None;
    }
    let _process_guard = RuntimeProcessGuard { pid: child.id() };
    let input_result = if let Some(input) = input {
        let mut stdin = child.take_stdin()?;
        let bytes = input.to_vec();
        let (sent, result) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = sent.send(stdin.write_all(&bytes).is_ok());
        });
        Some(result)
    } else {
        None
    };

    let Some(mut stdout) = child.take_stdout() else {
        terminate_and_reap(&mut child);
        return None;
    };
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut tail = Vec::new();
        let mut truncated = false;
        let mut buffer = [0_u8; 8 * 1024];
        let result = loop {
            match stdout.read(&mut buffer) {
                Ok(0) => break Some((tail, truncated)),
                Ok(read) => {
                    if read >= maximum_output_bytes {
                        tail.clear();
                        tail.extend_from_slice(&buffer[read - maximum_output_bytes..read]);
                        truncated = true;
                    } else {
                        let excess = tail
                            .len()
                            .saturating_add(read)
                            .saturating_sub(maximum_output_bytes);
                        if excess > 0 {
                            tail.drain(..excess);
                            truncated = true;
                        }
                        tail.extend_from_slice(&buffer[..read]);
                    }
                }
                Err(_) => break None,
            }
        };
        let _ = sender.send(result);
    });

    if super::runtime_shutting_down()
        || cancelled()
        || Instant::now() >= deadline
        || child.resume().is_err()
    {
        terminate_and_reap(&mut child);
        return None;
    }
    let mut input_written = input_result.is_none();
    let status = loop {
        if let Some(result) = input_result.as_ref().filter(|_| !input_written) {
            match result.try_recv() {
                Ok(true) => input_written = true,
                Ok(false) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    terminate_and_reap(&mut child);
                    return None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(_) => {
                terminate_and_reap(&mut child);
                return None;
            }
        }
        if super::runtime_shutting_down() || cancelled() || Instant::now() >= deadline {
            terminate_and_reap(&mut child);
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let remaining =
        || Duration::from_secs(1).min(deadline.saturating_duration_since(Instant::now()));
    if !input_written && !input_result?.recv_timeout(remaining()).ok()? {
        return None;
    }
    let (bytes, truncated) = receiver.recv_timeout(remaining()).ok()??;
    if super::runtime_shutting_down() || cancelled() || Instant::now() >= deadline {
        return None;
    }
    Some(BoundedOutput {
        status,
        bytes,
        truncated,
    })
}

#[cfg(test)]
#[path = "process_query/tests.rs"]
mod process_tests;

#[cfg(test)]
mod tests {
    #[test]
    fn tail_buffer_keeps_the_limit_invariant() {
        let maximum = 64;
        let chunks = [vec![1_u8; 40], vec![2_u8; 40], vec![3_u8; 80]];
        let mut tail = Vec::new();
        for chunk in chunks {
            if chunk.len() >= maximum {
                tail.clear();
                tail.extend_from_slice(&chunk[chunk.len() - maximum..]);
            } else {
                let excess = tail
                    .len()
                    .saturating_add(chunk.len())
                    .saturating_sub(maximum);
                if excess > 0 {
                    tail.drain(..excess);
                }
                tail.extend_from_slice(&chunk);
            }
            assert!(tail.len() <= maximum);
        }
        assert_eq!(tail, vec![3_u8; maximum]);
    }
}
