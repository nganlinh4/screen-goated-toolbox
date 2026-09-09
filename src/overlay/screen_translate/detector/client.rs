use std::io::{BufReader, BufWriter, Read as _};
use std::os::windows::ffi::OsStrExt as _;
use std::process::{Child, ChildStdin};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use sgt_screen_text_detector_protocol::{
    ClientMessage, DetectedRegion, DetectionTimings, ServerMessage, WORKER_VERSION, read_server,
    write_client,
};

use super::process::{LaunchResources, create_kill_on_close_job, spawn_worker, terminate_job};

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(60);
const DETECT_TIMEOUT: Duration = Duration::from_secs(15);
const WAIT_INTERVAL: Duration = Duration::from_millis(40);
const STDERR_TAIL_LIMIT: usize = 8 * 1024;

enum ReaderEvent {
    Message(u64, ServerMessage),
    Failed(String),
}

pub(super) struct WarmupProgress {
    state: Arc<AtomicU8>,
}

impl WarmupProgress {
    pub(super) fn start(title: &'static str, message: &'static str) -> Self {
        let badge =
            crate::overlay::auto_copy_badge::DownloadProgressBadge::with_text(title, message);
        let message = format!("≈ {message}");
        badge.set_phase(&message, 0.0);
        let state = Arc::new(AtomicU8::new(0));
        let worker_state = Arc::clone(&state);
        std::thread::spawn(move || {
            let started = Instant::now();
            loop {
                match worker_state.load(Ordering::Acquire) {
                    0 => badge.set_phase(&message, estimated_loading_percent(started.elapsed())),
                    1 => {
                        badge.set_phase(&message, 100.0);
                        std::thread::sleep(Duration::from_millis(350));
                        break;
                    }
                    _ => break,
                }
                std::thread::sleep(Duration::from_millis(80));
            }
            badge.finish();
        });
        Self { state }
    }

    pub(super) fn finish(self) {
        self.state.store(1, Ordering::Release);
    }
}

impl Drop for WarmupProgress {
    fn drop(&mut self) {
        let _ = self
            .state
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire);
    }
}

fn estimated_loading_percent(elapsed: Duration) -> f32 {
    let seconds = (elapsed.as_secs_f32() - 0.35).max(0.0);
    (99.0 * (1.0 - (-seconds / 3.0).exp())).min(99.0)
}

pub(super) struct DetectorClient {
    child: Child,
    stdin: Option<BufWriter<ChildStdin>>,
    responses: Receiver<ReaderEvent>,
    reader: Option<JoinHandle<()>>,
    stderr_reader: Option<JoinHandle<()>>,
    stderr_tail: Arc<Mutex<String>>,
    job: std::os::windows::io::OwnedHandle,
    next_request_id: u64,
    resources: LaunchResources,
}

impl DetectorClient {
    pub(super) fn start(cancelled: &AtomicBool) -> Result<Self> {
        let preparation = Instant::now();
        let resources = LaunchResources::ensure(cancelled)?;
        crate::log_info!(
            "[Screen Translate] detector_resources_ms={:.1}",
            preparation.elapsed().as_secs_f64() * 1000.0
        );
        let startup = Instant::now();
        let language = crate::APP
            .lock()
            .map(|app| app.config.ui_language.clone())
            .unwrap_or_else(|_| "en".to_string());
        let locale = crate::gui::locale::LocaleText::get(&language).screen_translate;
        let warmup_progress = WarmupProgress::start(
            locale.screen_translate_title,
            locale.screen_translate_preparing,
        );
        let mut child = spawn_worker(&resources)?;
        let job = match create_kill_on_close_job(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let stdin = child.stdin.take().context("open text detector stdin")?;
        let stdout = child.stdout.take().context("open text detector stdout")?;
        let stderr = child.stderr.take().context("open text detector stderr")?;
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
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        let stderr_target = Arc::clone(&stderr_tail);
        let stderr_reader = std::thread::spawn(move || {
            let mut stderr = BufReader::new(stderr);
            let mut bytes = [0_u8; 1024];
            let mut pending_line = String::new();
            loop {
                let Ok(read) = stderr.read(&mut bytes) else {
                    return;
                };
                if read == 0 {
                    return;
                }
                pending_line.push_str(&String::from_utf8_lossy(&bytes[..read]));
                while let Some(end) = pending_line.find('\n') {
                    let line = pending_line.drain(..=end).collect::<String>();
                    if line.starts_with("[DetectorPerf]") {
                        crate::log_info!("[Screen Translate] {}", line.trim_end());
                    }
                }
                if pending_line.len() > STDERR_TAIL_LIMIT {
                    pending_line.clear();
                }
                let mut tail = stderr_target
                    .lock()
                    .unwrap_or_else(|value| value.into_inner());
                tail.push_str(&String::from_utf8_lossy(&bytes[..read]));
                if tail.len() > STDERR_TAIL_LIMIT {
                    let keep_from = tail.len() - STDERR_TAIL_LIMIT;
                    let boundary = tail
                        .char_indices()
                        .find_map(|(index, _)| (index >= keep_from).then_some(index))
                        .unwrap_or(0);
                    tail.drain(..boundary);
                }
            }
        });
        let mut client = Self {
            child,
            stdin: Some(BufWriter::new(stdin)),
            responses,
            reader: Some(reader),
            stderr_reader: Some(stderr_reader),
            stderr_tail,
            job,
            next_request_id: 1,
            resources,
        };
        client.handshake(cancelled)?;
        crate::log_info!(
            "[Screen Translate] detector_spawn_handshake_ms={:.1}",
            startup.elapsed().as_secs_f64() * 1000.0
        );
        warmup_progress.finish();
        Ok(client)
    }

    pub(super) fn detect(
        &mut self,
        jpeg: &[u8],
        cancelled: &AtomicBool,
    ) -> Result<(u32, u32, Vec<DetectedRegion>)> {
        let response = self.request(
            ClientMessage::DetectJpeg(jpeg.to_vec()),
            DETECT_TIMEOUT,
            cancelled,
        )?;
        match response {
            ServerMessage::Regions {
                image_width,
                image_height,
                timings,
                regions,
            } => {
                log_detector_timings(timings, regions.len());
                Ok((image_width, image_height, regions))
            }
            ServerMessage::Error(error) => Err(anyhow!(error)),
            _ => bail!("text detector returned an unexpected response"),
        }
    }

    fn handshake(&mut self, cancelled: &AtomicBool) -> Result<()> {
        let mut nonce = [0_u8; 32];
        getrandom::fill(&mut nonce).map_err(|error| anyhow!("create detector nonce: {error}"))?;
        let response = self.request(
            ClientMessage::Hello {
                nonce,
                runtime_dir: self
                    .resources
                    .runtime
                    .bin_dir()
                    .as_os_str()
                    .encode_wide()
                    .collect(),
                model_dir: self
                    .resources
                    .detector
                    .model_dir()
                    .as_os_str()
                    .encode_wide()
                    .collect(),
            },
            HANDSHAKE_TIMEOUT,
            cancelled,
        )?;
        match response {
            ServerMessage::Ready {
                nonce: echoed,
                worker_version,
            } if echoed == nonce && worker_version == WORKER_VERSION => Ok(()),
            ServerMessage::Error(error) => Err(anyhow!(error)),
            _ => bail!("text detector handshake identity mismatch"),
        }
    }

    fn request(
        &mut self,
        message: ClientMessage,
        timeout: Duration,
        cancelled: &AtomicBool,
    ) -> Result<ServerMessage> {
        let request_id = self.next_request_id;
        self.next_request_id = self
            .next_request_id
            .checked_add(1)
            .ok_or_else(|| anyhow!("text detector request counter exhausted"))?;
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("text detector input is closed"))?;
        write_client(stdin, request_id, &message).context("send text detector request")?;
        let deadline = Instant::now() + timeout;
        loop {
            if cancelled.load(Ordering::SeqCst) {
                self.terminate();
                bail!("text detector request cancelled");
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.terminate();
                bail!("text detector timed out");
            }
            match self.responses.recv_timeout(remaining.min(WAIT_INTERVAL)) {
                Ok(ReaderEvent::Message(response_id, response)) if response_id == request_id => {
                    return Ok(response);
                }
                Ok(ReaderEvent::Message(response_id, _)) => {
                    self.terminate();
                    bail!(
                        "text detector response id mismatch: expected {request_id}, got {response_id}"
                    );
                }
                Ok(ReaderEvent::Failed(error)) => {
                    self.terminate();
                    let status = self
                        .child
                        .try_wait()
                        .ok()
                        .flatten()
                        .map(|status| format!(" ({status})"))
                        .unwrap_or_default();
                    let details = self
                        .stderr_tail
                        .lock()
                        .unwrap_or_else(|value| value.into_inner())
                        .trim()
                        .to_string();
                    if details.is_empty() {
                        bail!("text detector protocol failed{status}: {error}");
                    }
                    bail!("text detector protocol failed{status}: {error}; worker: {details}");
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    bail!("text detector response channel closed")
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
        if let Some(reader) = self.stderr_reader.take() {
            let _ = reader.join();
        }
    }
}

fn log_detector_timings(timings: DetectionTimings, region_count: usize) {
    crate::log_info!(
        "[Screen Translate] detector stages: total={:.1}ms decode={:.1}ms locator={:.1}ms primary={:.1}ms specialists={:.1}ms compose={:.1}ms regions={region_count}",
        timings.total_us as f32 / 1_000.0,
        timings.decode_us as f32 / 1_000.0,
        timings.locator_us as f32 / 1_000.0,
        timings.primary_recognition_us as f32 / 1_000.0,
        timings.specialist_recognition_us as f32 / 1_000.0,
        timings.composition_us as f32 / 1_000.0,
    );
}

impl Drop for DetectorClient {
    fn drop(&mut self) {
        if let Some(stdin) = self.stdin.as_mut() {
            let _ = write_client(stdin, self.next_request_id, &ClientMessage::Shutdown);
        }
        self.terminate();
    }
}

#[cfg(test)]
mod progress_tests {
    use super::*;

    #[test]
    fn estimate_starts_at_zero_and_never_claims_readiness() {
        assert_eq!(estimated_loading_percent(Duration::ZERO), 0.0);
        let mut previous = 0.0;
        for millis in (0..120_000).step_by(80) {
            let percent = estimated_loading_percent(Duration::from_millis(millis));
            assert!((previous..100.0).contains(&percent));
            previous = percent;
        }
    }

    #[test]
    fn dropping_completed_progress_preserves_its_completion_hold() {
        let state = Arc::new(AtomicU8::new(0));
        WarmupProgress {
            state: Arc::clone(&state),
        }
        .finish();
        assert_eq!(state.load(Ordering::Acquire), 1);
        state.store(0, Ordering::Release);
        drop(WarmupProgress {
            state: Arc::clone(&state),
        });
        assert_eq!(state.load(Ordering::Acquire), 2);
    }
}
