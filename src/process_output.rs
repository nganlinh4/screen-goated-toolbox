//! Bounded diagnostic capture for owned noninteractive helper processes.
use crate::process_job::{HelperCommand, OwnedProcess};
use anyhow::{Context, Result, bail, ensure};
use std::collections::VecDeque;
use std::io::Read;
use std::process::Output;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const TAIL_BYTES: usize = 64 * 1024;

pub(crate) fn run(
    command: &HelperCommand,
    resources: impl Send + 'static,
    cancelled: impl Fn() -> bool,
    timeout: Duration,
) -> Result<Output> {
    ensure!(!cancelled(), "Operation cancelled");
    ensure!(!timeout.is_zero(), "Helper deadline must be positive");
    let deadline = Instant::now() + timeout;
    let mut process = OwnedProcess::spawn_helper(command, resources)?;
    let stdout = process.take_stdout().context("Helper stdout is missing")?;
    let stderr = process.take_stderr().context("Helper stderr is missing")?;
    let (send, receive) = mpsc::sync_channel(2);
    for (index, mut pipe) in [
        (0, Box::new(stdout) as Box<dyn Read + Send>),
        (1, Box::new(stderr) as Box<dyn Read + Send>),
    ] {
        let send = send.clone();
        std::thread::Builder::new()
            .name("sgt-helper-output".into())
            .spawn(move || {
                let result = tail(&mut pipe);
                let _ = send.send((index, result));
            })
            .context("Start helper output reader")?;
    }
    drop(send);
    process.resume()?;
    let result = (|| {
        let mut output = [None, None];
        loop {
            ensure!(!cancelled(), "Operation cancelled");
            ensure!(Instant::now() < deadline, "Helper execution timed out");
            let status = process.try_wait()?;
            match receive.recv_timeout(Duration::from_millis(25)) {
                Ok((index, bytes)) => output[index] = Some(bytes?),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    std::thread::sleep(Duration::from_millis(25))
                }
            }
            if let Some(status) = status
                && process.stopped()
                && output.iter().all(Option::is_some)
            {
                ensure!(!cancelled(), "Operation cancelled");
                return Ok(Output {
                    status,
                    stdout: output[0].take().unwrap(),
                    stderr: output[1].take().unwrap(),
                });
            }
        }
    })();
    if let Err(error) = process.stop_within(Duration::from_secs(2)) {
        bail!(
            "Helper cleanup is still pending: {error:#}; operation: {}",
            result
                .err()
                .map(|error: anyhow::Error| error.to_string())
                .unwrap_or_default()
        );
    }
    result
}

fn tail(reader: &mut impl Read) -> std::io::Result<Vec<u8>> {
    let mut tail = VecDeque::new();
    let mut bytes = [0_u8; 4096];
    loop {
        let count = reader.read(&mut bytes)?;
        if count == 0 {
            return Ok(tail.into_iter().collect());
        }
        tail.extend(&bytes[..count]);
        while tail.len() > TAIL_BYTES {
            tail.pop_front();
        }
    }
}

#[cfg(test)]
#[path = "process_output/tests.rs"]
mod tests;
