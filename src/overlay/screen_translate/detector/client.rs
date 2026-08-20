use std::io::{BufReader, BufWriter, Read as _};
use std::os::windows::ffi::OsStrExt as _;
use std::process::{Child, ChildStdin};
use std::sync::atomic::{AtomicBool, Ordering};
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
const WARMUP_PROGRESS_INTERVAL: Duration = Duration::from_millis(120);
const WARMUP_PROGRESS_CEILING: f32 = 90.0;
const WARMUP_PROGRESS_TIME_CONSTANT_SECS: f32 = 0.45;

enum ReaderEvent {
    Message(u64, ServerMessage),
    Failed(String),
}

struct WarmupProgress {
    badge: Arc<crate::overlay::auto_copy_badge::DownloadProgressBadge>,
    complete: Arc<AtomicBool>,
    message: &'static str,
}

impl WarmupProgress {
    fn start(title: &'static str, message: &'static str) -> Self {
        let badge = Arc::new(
            crate::overlay::auto_copy_badge::DownloadProgressBadge::with_text(title, message),
        );
        badge.set_phase(message, 0.0);
        let complete = Arc::new(AtomicBool::new(false));
        simulate_warmup_progress(Arc::clone(&badge), Arc::clone(&complete), message);
        Self {
            badge,
            complete,
            message,
        }
    }

    fn finish(self) {
        self.badge.set_phase(self.message, 100.0);
    }
}

impl Drop for WarmupProgress {
    fn drop(&mut self) {
        self.complete.store(true, Ordering::Release);
        self.badge.finish();
    }
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
        let resources = LaunchResources::ensure(cancelled)?;
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
            loop {
                let Ok(read) = stderr.read(&mut bytes) else {
                    return;
                };
                if read == 0 {
                    return;
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

fn simulate_warmup_progress(
    badge: Arc<crate::overlay::auto_copy_badge::DownloadProgressBadge>,
    complete: Arc<AtomicBool>,
    message: &'static str,
) {
    let _ = std::thread::Builder::new()
        .name("sgt-screen-text-detector-warmup-progress".to_string())
        .spawn(move || {
            let started = Instant::now();
            while !complete.load(Ordering::Acquire) {
                std::thread::sleep(WARMUP_PROGRESS_INTERVAL);
                badge.set_phase(message, estimated_warmup_progress(started.elapsed()));
            }
        });
}

fn estimated_warmup_progress(elapsed: Duration) -> f32 {
    let progress = WARMUP_PROGRESS_CEILING
        * (1.0 - (-elapsed.as_secs_f32() / WARMUP_PROGRESS_TIME_CONSTANT_SECS).exp());
    progress.min(WARMUP_PROGRESS_CEILING)
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
mod warmup_progress_tests {
    use super::*;

    #[test]
    fn estimate_matches_the_lazy_gpu_startup_without_finishing_early() {
        assert_eq!(estimated_warmup_progress(Duration::ZERO), 0.0);
        assert!(estimated_warmup_progress(Duration::from_millis(800)) > 70.0);
        assert!(estimated_warmup_progress(Duration::from_secs(30)) <= 90.0);
    }
}
