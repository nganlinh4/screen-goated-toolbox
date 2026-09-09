use super::process::{LaunchResources, create_kill_on_close_job, spawn_worker, terminate_job};
use anyhow::{Context, Result, bail};
use sgt_screen_text_detector_protocol::{
    recognition::CaptureReadings,
    stream::{self, Event, Hello, Request},
};
use std::io::{BufReader, BufWriter, Read as _};
use std::os::windows::ffi::OsStrExt as _;
use std::process::{Child, ChildStdin};
use std::sync::{
    Arc, LazyLock, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};
use std::time::{Duration, Instant};

static CLIENT: LazyLock<Mutex<Option<Client>>> = LazyLock::new(|| Mutex::new(None));

pub(crate) fn prepare(cancelled: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        crate::log_info!("[Screen Translate] incremental preparation queued");
        let mut client = CLIENT.lock().unwrap_or_else(|e| e.into_inner());
        if client.is_none() {
            match Client::start(&cancelled) {
                Ok(value) => *client = Some(value),
                Err(error) => {
                    crate::log_info!("[Screen Translate] incremental preparation failed: {error:#}")
                }
            }
        }
    });
}

pub(crate) fn stop() {
    CLIENT.lock().unwrap_or_else(|e| e.into_inner()).take();
}

pub(crate) fn detect(
    jpeg: &[u8],
    width: u32,
    height: u32,
    cancelled: &AtomicBool,
    mut event: impl FnMut(Event) -> Result<()>,
) -> Result<()> {
    let mut slot = CLIENT.lock().unwrap_or_else(|e| e.into_inner());
    if slot.is_none() {
        *slot = Some(Client::start(cancelled)?);
    }
    let result = slot
        .as_mut()
        .expect("initialized client")
        .capture(jpeg, width, height, cancelled, &mut event);
    if result.is_err() && !slot.as_ref().is_some_and(|client| client.cancel_drained) {
        slot.take();
    }
    result
}

struct Client {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    responses: Receiver<Result<(u64, Event), String>>,
    job: std::os::windows::io::OwnedHandle,
    _resources: LaunchResources,
    next_id: u64,
    cancel_drained: bool,
}

impl Client {
    fn start(cancelled: &AtomicBool) -> Result<Self> {
        let resources = LaunchResources::ensure(cancelled)?;
        if resources.detector.version() != stream::WORKER_VERSION {
            bail!("incremental worker delivery mismatch");
        }
        let language = crate::APP
            .lock()
            .map(|app| app.config.ui_language.clone())
            .unwrap_or_else(|_| "en".to_string());
        let locale = crate::gui::locale::LocaleText::get(&language).screen_translate;
        let warmup_progress = super::client::WarmupProgress::start(
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
        let mut child = spawn_worker(&resources)?;
        let job = match create_kill_on_close_job(&child) {
            Ok(job) => job,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            terminate_job(&job);
            let _ = child.wait();
            bail!("worker pipes missing");
        };
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
            job,
            _resources: resources,
            next_id: 2,
            cancel_drained: false,
        };
        stream::write_request(&mut client.stdin, 1, &Request::Hello(hello))?;
        match client.receive(1, Instant::now() + Duration::from_secs(180), cancelled)? {
            Event::Ready {
                nonce: echoed,
                version,
            } if echoed == nonce && version == stream::WORKER_VERSION => {
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
        self.cancel_drained = false;
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
                    on_event(event)?;
                    return Ok(());
                }
                Event::Error { message } => bail!("incremental OCR: {message}"),
                Event::Cancelled => bail!("incremental OCR cancelled"),
                _ => bail!("invalid incremental OCR event sequence"),
            }
            on_event(event)?;
        }
    }

    fn receive(&mut self, id: u64, deadline: Instant, cancelled: &AtomicBool) -> Result<Event> {
        loop {
            if cancelled.load(Ordering::Acquire) {
                let _ = stream::write_request(&mut self.stdin, id, &Request::Cancel);
                self.cancel_drained = self.drain_cancel(id);
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

    fn drain_cancel(&self, id: u64) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return false;
            }
            match self.responses.recv_timeout(remaining) {
                Ok(Ok((response_id, Event::Cancelled | Event::Finished { .. })))
                    if response_id == id =>
                {
                    return true;
                }
                Ok(Ok((response_id, Event::Reading { .. } | Event::Geometry { .. })))
                    if response_id == id => {}
                _ => return false,
            }
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        terminate_job(&self.job);
        let _ = self.child.wait();
    }
}
