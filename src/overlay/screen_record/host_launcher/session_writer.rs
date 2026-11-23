//! One ordered authenticated request stream shared by command and ACK senders.

use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use anyhow::{Result, anyhow, bail};
use sgt_recorder_protocol::{Command, MAX_JSON_BYTES, PROTOCOL_VERSION, Request};

use super::REQUEST_ID;

pub(super) struct SessionWriter {
    token: String,
    output: Mutex<Option<Box<dyn Write + Send>>>,
    closed: AtomicBool,
}

impl SessionWriter {
    pub(super) fn new(token: String, output: impl Write + Send + 'static) -> Self {
        Self {
            token,
            output: Mutex::new(Some(Box::new(output))),
            closed: AtomicBool::new(false),
        }
    }

    pub(super) fn send(&self, command: Command) -> Result<u64> {
        anyhow::ensure!(
            !self.closed.load(Ordering::Acquire),
            "recorder session is closed"
        );
        command.validate().map_err(anyhow::Error::msg)?;
        let mut output = self
            .output
            .lock()
            .map_err(|_| anyhow!("recorder request writer is unavailable"))?;
        anyhow::ensure!(
            !self.closed.load(Ordering::Acquire),
            "recorder session is closed"
        );
        let output = output
            .as_mut()
            .ok_or_else(|| anyhow!("recorder session is closed"))?;
        let request_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
        let request = Request {
            protocol_version: PROTOCOL_VERSION,
            token: self.token.clone(),
            request_id,
            command,
        };
        let mut body = serde_json::to_vec(&request)?;
        if body.len() > MAX_JSON_BYTES {
            bail!("recorder request exceeds protocol limit");
        }
        body.push(b'\n');
        output.write_all(&body)?;
        output.flush()?;
        Ok(request_id)
    }

    pub(super) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        match self.output.try_lock() {
            Ok(mut output) => {
                output.take();
            }
            Err(std::sync::TryLockError::Poisoned(error)) => {
                error.into_inner().take();
            }
            Err(std::sync::TryLockError::WouldBlock) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sgt_recorder_protocol::TOKEN_BYTES;
    use std::sync::Arc;

    struct SharedOutput(Arc<Mutex<Vec<u8>>>);
    impl Write for SharedOutput {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn close_does_not_wait_for_a_blocked_write_and_rejects_later_requests() {
        struct BlockedOutput(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>);
        impl Write for BlockedOutput {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.send(()).unwrap();
                self.1
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let (entered, entry) = std::sync::mpsc::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let writer = Arc::new(SessionWriter::new(
            "01".repeat(TOKEN_BYTES),
            BlockedOutput(entered, blocked),
        ));
        let writing = writer.clone();
        let thread = std::thread::spawn(move || writing.send(Command::Ping));
        entry
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        let started = std::time::Instant::now();
        writer.close();
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        assert!(writer.send(Command::Ping).is_err());
        release.send(()).unwrap();
        thread.join().unwrap().unwrap();
        assert!(writer.send(Command::Ping).is_err());
        writer.close();
    }

    #[test]
    fn concurrent_commands_and_acks_keep_authenticated_request_order() {
        let token = "01".repeat(TOKEN_BYTES);
        let output = Arc::new(Mutex::new(Vec::new()));
        let writer = Arc::new(SessionWriter::new(
            token.clone(),
            SharedOutput(output.clone()),
        ));
        let threads: Vec<_> = (0..16)
            .map(|index| {
                let writer = writer.clone();
                std::thread::spawn(move || {
                    let command = if index % 2 == 0 {
                        Command::Ping
                    } else {
                        Command::SettingsAck {
                            event_id: index,
                            settings: None,
                            error: Some("denied".into()),
                        }
                    };
                    writer.send(command).unwrap();
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let bytes = output.lock().unwrap();
        let mut previous = 0;
        for line in bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
        {
            let request: Request = serde_json::from_slice(line).unwrap();
            request.validate(&token, previous).unwrap();
            previous = request.request_id;
        }
        assert_ne!(previous, 0);
        drop(bytes);
        writer.close();
        assert!(writer.send(Command::Ping).is_err());
    }
}
