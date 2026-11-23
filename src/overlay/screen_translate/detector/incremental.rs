use super::process::{LaunchResources, spawn_worker};
use crate::process_job::{Input, OwnedProcess};
use anyhow::{Context, Result, bail};
use sgt_screen_text_detector_protocol::{
    recognition::CaptureReadings,
    stream::{self, Event, Hello, Request},
};
use std::io::{BufReader, BufWriter, Read as _};
use std::os::windows::ffi::OsStrExt as _;
use std::sync::{
    Arc, LazyLock,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};
use std::time::{Duration, Instant};

static CLIENT: LazyLock<super::client_slot::ClientSlot<Client>> =
    LazyLock::new(super::client_slot::ClientSlot::new);

pub(crate) fn prepare(cancelled: Arc<AtomicBool>) {
    let Some(mut client) = CLIENT.begin_prepare(Arc::clone(&cancelled)) else {
        return;
    };
    std::thread::spawn(move || {
        crate::log_info!("[Screen Translate] incremental preparation queued");
        match Client::start(&cancelled) {
            Ok(value) => {
                client.value = Some(value);
                if !cancelled.load(Ordering::Acquire) {
                    client.retain();
                }
            }
            Err(error) if !cancelled.load(Ordering::Acquire) => {
                crate::log_info!("[Screen Translate] incremental preparation failed: {error:#}")
            }
            Err(_) => {}
        }
    });
}

pub(crate) fn stop() {
    CLIENT.stop();
}

pub(crate) fn detect(
    jpeg: &[u8],
    width: u32,
    height: u32,
    cancelled: &Arc<AtomicBool>,
    mut event: impl FnMut(Event) -> Result<()>,
) -> Result<()> {
    super::super::image_payload::validate_detector_input(jpeg, width, height)?;
    let mut slot = CLIENT.acquire(Arc::clone(cancelled))?;
    if slot.value.is_none() {
        slot.value = Some(Client::start(cancelled)?);
    }
    let result = slot
        .value
        .as_mut()
        .expect("initialized client")
        .capture(jpeg, width, height, cancelled, &mut event);
    if result.is_ok()
        || slot
            .value
            .as_ref()
            .is_some_and(|client| client.terminal_drained)
    {
        slot.retain();
    }
    result
}

struct Client {
    child: OwnedProcess,
    stdin: BufWriter<Input>,
    responses: Receiver<Result<(u64, Event), String>>,
    _resources: Arc<LaunchResources>,
    next_id: u64,
    terminal_drained: bool,
}

impl Client {
    fn start(cancelled: &Arc<AtomicBool>) -> Result<Self> {
        let resources = Arc::new(LaunchResources::ensure(cancelled)?);
        let language = crate::APP
            .lock()
            .map(|app| app.config.ui_language.clone())
            .unwrap_or_else(|_| "en".to_string());
        let locale = crate::gui::locale::LocaleText::get(&language).screen_translate;
        let warmup_progress = super::warmup::WarmupProgress::start(
            locale.screen_translate_title,
            locale.screen_translate_preparing,
        );
        let wide = |path: &std::path::Path| path.as_os_str().encode_wide().collect();
        let mut nonce = [0_u8; 32];
        getrandom::fill(&mut nonce).map_err(|e| anyhow::anyhow!("worker nonce: {e}"))?;
        let models = resources.detector.model_dir();
        let hello = Hello {
            nonce,
            runtime_dir: wide(resources.runtime.bin_dir()),
            detector_model: wide(&models.join("detector.onnx")),
            reader_catalog: wide(&models.join("readers.json")),
        };
        let mut child = spawn_worker(resources.clone(), cancelled)?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.take_stdin(), child.take_stdout(), child.take_stderr())
        else {
            bail!("worker pipes missing");
        };
        child.resume()?;
        let (sender, responses) = mpsc::sync_channel(8);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match stream::read_event(&mut reader) {
                    Ok(value) => {
                        if sender.send(Ok(value)).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error.to_string()));
                        break;
                    }
                }
            }
        });
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut bytes = [0_u8; 2048];
            let mut pending = String::new();
            while let Ok(count) = reader.read(&mut bytes) {
                if count == 0 {
                    break;
                }
                pending.push_str(&String::from_utf8_lossy(&bytes[..count]));
                while let Some(end) = pending.find('\n') {
                    let line = pending.drain(..=end).collect::<String>();
                    if line.starts_with("[DetectorPerf]") {
                        crate::log_info!("[Screen Translate] {}", line.trim_end());
                    }
                }
                if pending.len() > 8192 {
                    pending.clear();
                }
            }
        });
        let mut client = Self {
            child,
            stdin: BufWriter::new(stdin),
            responses,
            _resources: resources,
            next_id: 2,
            terminal_drained: false,
        };
        stream::write_request(&mut client.stdin, 1, &Request::Hello(hello))?;
        match client.receive(1, Instant::now() + Duration::from_secs(180), cancelled)? {
            Event::Ready {
                nonce: echoed,
                version,
            } if echoed == nonce && version == client._resources.detector.version() => {
                warmup_progress.finish();
                Ok(client)
            }
            Event::Error { message } => bail!("incremental worker initialization: {message}"),
            _ => bail!("incremental worker handshake identity mismatch"),
        }
    }

    fn capture(
        &mut self,
        jpeg: &[u8],
        width: u32,
        height: u32,
        cancelled: &AtomicBool,
        on_event: &mut impl FnMut(Event) -> Result<()>,
    ) -> Result<()> {
        self.terminal_drained = false;
        if cancelled.load(Ordering::Acquire) {
            self.terminal_drained = true;
            bail!("incremental OCR cancelled before submission");
        }
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .context("capture identity exhausted")?;
        stream::write_request(&mut self.stdin, id, &Request::Capture(jpeg.to_vec()))?;
        let mut state = None;
        let mut count = 0;
        // Idle deadline advances only after a validated event. Total work is bounded separately.
        let mut total_deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let event = self.receive(
                id,
                (Instant::now() + Duration::from_secs(20)).min(total_deadline),
                cancelled,
            )?;
            match &event {
                Event::Geometry {
                    width: w,
                    height: h,
                    regions,
                    ..
                } if state.is_none() && *w == width && *h == height => {
                    count = regions.len();
                    total_deadline = Instant::now() + Duration::from_secs(120);
                    state = Some(CaptureReadings::new(id, regions.iter().map(|r| r.id))?);
                }
                Event::Reading { completion } => state
                    .as_mut()
                    .context("reading preceded geometry")?
                    .accept(completion.clone())?,
                Event::Finished { count: finished } if *finished == count => {
                    state
                        .take()
                        .context("capture completed without geometry")?
                        .finish()?;
                    self.terminal_drained = true;
                    on_event(event)?;
                    return Ok(());
                }
                Event::Error { message } => {
                    self.terminal_drained = true;
                    bail!("incremental OCR: {message}");
                }
                Event::Cancelled => bail!("unexpected incremental OCR cancellation"),
                _ => bail!("invalid incremental OCR event sequence"),
            }
            on_event(event)?;
        }
    }

    fn receive(&mut self, id: u64, deadline: Instant, cancelled: &AtomicBool) -> Result<Event> {
        loop {
            if cancelled.load(Ordering::Acquire) {
                let _ = stream::write_request(&mut self.stdin, id, &Request::Cancel);
                self.terminal_drained = drain_cancel(&self.responses, id, Duration::from_secs(2));
                bail!("incremental OCR cancelled");
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("incremental OCR timed out");
            }
            match self
                .responses
                .recv_timeout(remaining.min(Duration::from_millis(20)))
            {
                Ok(Ok((response_id, event))) if response_id == id => return Ok(event),
                Ok(Ok(_)) => bail!("incremental OCR response identity mismatch"),
                Ok(Err(error)) => bail!("incremental OCR protocol: {error}"),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    bail!("incremental OCR worker disconnected")
                }
            }
        }
    }
}

fn drain_cancel(
    responses: &Receiver<Result<(u64, Event), String>>,
    id: u64,
    timeout: Duration,
) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        match responses.recv_timeout(remaining) {
            Ok(Ok((
                response_id,
                Event::Cancelled | Event::Finished { .. } | Event::Error { .. },
            ))) if response_id == id => {
                return true;
            }
            Ok(Ok((response_id, Event::Reading { .. } | Event::Geometry { .. })))
                if response_id == id => {}
            _ => return false,
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.stop_within(Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_drain_reuses_only_matching_terminal_events() {
        let (tx, rx) = mpsc::channel();
        tx.send(Ok((
            7,
            Event::Error {
                message: "request rejected".into(),
            },
        )))
        .unwrap();
        assert!(drain_cancel(&rx, 7, Duration::from_millis(10)));
        tx.send(Ok((8, Event::Finished { count: 0 }))).unwrap();
        assert!(!drain_cancel(&rx, 7, Duration::from_millis(10)));
        assert!(!drain_cancel(&rx, 7, Duration::from_millis(1)));
    }
}
