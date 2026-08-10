use std::io::{BufReader, BufWriter};
use std::os::windows::ffi::OsStrExt as _;
use std::path::Path;
use std::process::{Child, ChildStdin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
#[cfg(not(feature = "recorder-worker"))]
use sgt_local_asr_protocol::EOU_CHUNK_SAMPLES;
#[cfg(feature = "recorder-worker")]
use sgt_local_asr_protocol::TimedToken;
use sgt_local_asr_protocol::{
    ClientMessage, Mode, ServerMessage, WORKER_VERSION, read_server, write_client,
};

use super::model_lease::ModelLease;
use super::process::{LaunchResources, create_kill_on_close_job, spawn_worker, terminate_job};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(300);
#[cfg(not(feature = "recorder-worker"))]
const EOU_TIMEOUT: Duration = Duration::from_secs(60);
#[cfg(feature = "recorder-worker")]
const TDT_TIMEOUT: Duration = Duration::from_secs(300);
const WAIT_INTERVAL: Duration = Duration::from_millis(40);

enum ReaderEvent {
    Message(u64, ServerMessage),
    Failed(String),
}

pub(crate) struct LocalAsrClient {
    child: Child,
    stdin: Option<BufWriter<ChildStdin>>,
    responses: Receiver<ReaderEvent>,
    reader: Option<JoinHandle<()>>,
    job: std::os::windows::io::OwnedHandle,
    next_request_id: u64,
    mode: Mode,
    _model: ModelLease,
    _resources: LaunchResources,
}

impl LocalAsrClient {
    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn prepare(cancelled: &AtomicBool) -> Result<()> {
        drop(LaunchResources::ensure(cancelled)?);
        Ok(())
    }

    pub(crate) fn start(mode: Mode, model_dir: &Path, cancelled: &AtomicBool) -> Result<Self> {
        let model = ModelLease::acquire(mode, model_dir)?;
        let resources = LaunchResources::ensure(cancelled)?;
        let mut child = spawn_worker(&resources)?;
        let job = match create_kill_on_close_job(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let stdin = child.stdin.take().context("open local ASR worker stdin")?;
        let stdout = child
            .stdout
            .take()
            .context("open local ASR worker stdout")?;
        let (sender, responses) = std::sync::mpsc::sync_channel(4);
        let reader = std::thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            loop {
                match read_server(&mut stdout) {
                    Ok((request_id, message)) => {
                        if sender
                            .send(ReaderEvent::Message(request_id, message))
                            .is_err()
                        {
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(ReaderEvent::Failed(error.to_string()));
                        return;
                    }
                }
            }
        });
        let mut client = Self {
            child,
            stdin: Some(BufWriter::new(stdin)),
            responses,
            reader: Some(reader),
            job,
            next_request_id: 1,
            mode,
            _model: model,
            _resources: resources,
        };
        client.handshake(model_dir, cancelled)?;
        Ok(client)
    }

    #[cfg(not(feature = "recorder-worker"))]
    pub(crate) fn transcribe_eou(
        &mut self,
        samples: &[f32],
        cancelled: &AtomicBool,
    ) -> Result<String> {
        if self.mode != Mode::RealtimeEou || samples.len() != EOU_CHUNK_SAMPLES {
            bail!("invalid realtime EOU request");
        }
        let response = self.request(
            ClientMessage::EouChunk {
                samples: samples.to_vec(),
            },
            EOU_TIMEOUT,
            Some(cancelled),
        )?;
        match response {
            ServerMessage::EouText(text) => Ok(text),
            ServerMessage::Error(error) => Err(anyhow!(error)),
            _ => bail!("local ASR worker returned an unexpected EOU response"),
        }
    }

    #[cfg(feature = "recorder-worker")]
    pub(crate) fn transcribe_tdt(
        &mut self,
        samples: Vec<f32>,
        cancelled: &AtomicBool,
    ) -> Result<Vec<TimedToken>> {
        if self.mode != Mode::SubtitleTdt {
            bail!("invalid subtitle TDT request");
        }
        let response = self.request(
            ClientMessage::TdtChunk {
                sample_rate: 16_000,
                channels: 1,
                samples,
            },
            TDT_TIMEOUT,
            Some(cancelled),
        )?;
        match response {
            ServerMessage::TdtTokens(tokens) => Ok(tokens),
            ServerMessage::Error(error) => Err(anyhow!(error)),
            _ => bail!("local ASR worker returned an unexpected TDT response"),
        }
    }

    fn handshake(&mut self, model_dir: &Path, cancelled: &AtomicBool) -> Result<()> {
        let runtime_dir = self._resources.runtime.bin_dir();
        let mut nonce = [0_u8; 32];
        getrandom::fill(&mut nonce).map_err(|error| anyhow!("create ASR nonce: {error}"))?;
        let response = self.request(
            ClientMessage::Hello {
                nonce,
                mode: self.mode,
                runtime_dir: runtime_dir.as_os_str().encode_wide().collect(),
                model_dir: model_dir.as_os_str().encode_wide().collect(),
            },
            HANDSHAKE_TIMEOUT,
            Some(cancelled),
        )?;
        match response {
            ServerMessage::Ready {
                nonce: echoed,
                mode,
                worker_version,
            } if echoed == nonce && mode == self.mode && worker_version == WORKER_VERSION => Ok(()),
            ServerMessage::Error(error) => Err(anyhow!(error)),
            _ => bail!("local ASR worker handshake identity mismatch"),
        }
    }

    fn request(
        &mut self,
        message: ClientMessage,
        timeout: Duration,
        cancelled: Option<&AtomicBool>,
    ) -> Result<ServerMessage> {
        let request_id = self.next_request_id;
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or_else(|| anyhow!("local ASR request counter exhausted"))?;
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("local ASR worker input is closed"))?;
        write_client(stdin, request_id, &message).context("send local ASR request")?;
        let deadline = Instant::now() + timeout;
        loop {
            if cancelled.is_some_and(|signal| signal.load(Ordering::SeqCst)) {
                self.terminate();
                bail!("local ASR request cancelled");
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.terminate();
                bail!("local ASR worker timed out");
            }
            match self.responses.recv_timeout(remaining.min(WAIT_INTERVAL)) {
                Ok(ReaderEvent::Message(response_id, response)) if response_id == request_id => {
                    return Ok(response);
                }
                Ok(ReaderEvent::Message(response_id, _)) => {
                    self.terminate();
                    bail!(
                        "local ASR response id mismatch: expected {request_id}, got {response_id}"
                    );
                }
                Ok(ReaderEvent::Failed(error)) => {
                    let status = self
                        .child
                        .try_wait()
                        .ok()
                        .flatten()
                        .map(|status| format!(" ({status})"))
                        .unwrap_or_default();
                    bail!("local ASR worker protocol failed{status}: {error}");
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    bail!("local ASR worker response channel closed")
                }
            }
        }
    }

    fn terminate(&mut self) {
        self.stdin.take();
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            terminate_job(&self.job);
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for LocalAsrClient {
    fn drop(&mut self) {
        if let Some(stdin) = self.stdin.as_mut() {
            let request_id = self.next_request_id;
            let _ = write_client(stdin, request_id, &ClientMessage::Shutdown);
        }
        self.terminate();
    }
}
