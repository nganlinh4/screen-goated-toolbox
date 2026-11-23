use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use super::EventSupervisor;
use crate::overlay::creation_process::CreationProcess;

const EXIT_GRACE: Duration = Duration::from_secs(2);
const REAP_GRACE: Duration = Duration::from_secs(2);
const EXIT_POLL: Duration = Duration::from_millis(10);

impl EventSupervisor {
    pub(crate) fn finish(&mut self, child: &mut CreationProcess) -> Result<(), String> {
        self.finish_with_limits(child, EXIT_GRACE, REAP_GRACE)
            .inspect_err(|error| {
                crate::log_info!("[Creation] Runtime exit cleanup is pending: {error}");
            })
    }

    fn finish_with_limits(
        &mut self,
        child: &mut CreationProcess,
        grace: Duration,
        reap: Duration,
    ) -> Result<(), String> {
        let mut deadline = Instant::now() + grace;
        let mut forced = false;
        let mut disconnected = false;
        loop {
            let exited = child
                .try_wait()
                .map_err(|error| format!("Could not reap creation process: {error}"))?
                .is_some();
            let stopped = child
                .active_processes()
                .map_err(|error| format!("Could not verify creation process tree: {error}"))?
                == 0;
            if exited && stopped {
                return Ok(());
            }
            let now = Instant::now();
            if now >= deadline {
                if forced {
                    return Err("Creation process tree has not stopped yet.".into());
                }
                child
                    .terminate()
                    .map_err(|error| format!("Could not stop creation process tree: {error}"))?;
                forced = true;
                deadline = now + reap;
            }
            let wait = EXIT_POLL.min(deadline.saturating_duration_since(Instant::now()));
            if disconnected {
                std::thread::sleep(wait);
            } else {
                // Consume trailing events without interpreting another result;
                // the bounded reader channel must not hold stdout backpressure.
                match self.receiver.recv_timeout(wait) {
                    Err(RecvTimeoutError::Disconnected) => disconnected = true,
                    Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
