use super::*;
use crate::api::realtime_audio::capture::{CaptureStream, CaptureWorker};

pub(crate) struct S2sCapture {
    stream: Arc<Mutex<CaptureSlot<CaptureStream>>>,
    _watcher: CaptureWorker,
    buffer: Arc<Mutex<Vec<i16>>>,
    per_app: bool,
    stop: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    session_id: u64,
    engine_revision: u64,
    mode: S2sMode,
    epoch: u64,
    seen_hidden: bool,
}

impl S2sCapture {
    pub(super) fn open(
        selected_pid: Option<u32>,
        stop: Arc<AtomicBool>,
        pause: Arc<AtomicBool>,
        session_id: u64,
        engine_revision: u64,
        mode: S2sMode,
    ) -> Result<Self> {
        let stream = Arc::new(Mutex::new(CaptureSlot::default()));
        let watcher_stop = stop.clone();
        let watcher = watch_capture(stream.clone(), move || {
            watcher_stop.load(Ordering::SeqCst)
                || is_stale_session(session_id)
                || !crate::overlay::realtime_webview::state::engine_is_current(engine_revision)
                || AUDIO_SOURCE_CHANGE.load(Ordering::SeqCst)
                || TRANSCRIPTION_MODEL_CHANGE.load(Ordering::SeqCst)
                || LANGUAGE_CHANGE.load(Ordering::SeqCst)
        })?;
        let mut owner = Self {
            stream,
            _watcher: watcher,
            buffer: Arc::new(Mutex::new(Vec::new())),
            per_app: selected_pid.is_some(),
            stop,
            pause,
            session_id,
            engine_revision,
            mode,
            epoch: TTS_MANAGER.realtime_epoch(),
            seen_hidden: !crate::overlay::realtime_webview::state::TRANS_VISIBLE
                .load(Ordering::SeqCst),
        };
        owner.open_stream(selected_pid)?;
        Ok(owner)
    }

    fn open_stream(&mut self, selected_pid: Option<u32>) -> Result<()> {
        let stop = self.stop.clone();
        let pause = self.pause.clone();
        let session_id = self.session_id;
        let engine_revision = self.engine_revision;
        let mode = self.mode;
        replace_capture(&self.stream, &mut self.buffer, |buffer| {
            let owner = Some(
                crate::api::realtime_audio::capture::RealtimeCaptureOwner::new(
                    session_id,
                    engine_revision,
                    stop.clone(),
                ),
            );
            if let Some(pid) = selected_pid {
                let capture =
                    start_per_app_capture(pid, buffer.clone(), stop.clone(), pause.clone(), owner)?;
                Ok(spawn_s2s_per_app_audio_pid_refresh(
                    pid,
                    capture,
                    buffer,
                    stop,
                    pause,
                    session_id,
                    engine_revision,
                    mode,
                )?
                .into())
            } else {
                start_mic_capture_resilient(buffer, stop, pause, owner)
            }
        })
    }

    pub(crate) fn needs_reopen(&mut self) -> bool {
        let visible = crate::overlay::realtime_webview::state::TRANS_VISIBLE.load(Ordering::SeqCst);
        if !visible {
            self.seen_hidden = true;
        }
        capture_needs_reopen(
            self.epoch,
            TTS_MANAGER.realtime_epoch(),
            self.seen_hidden,
            visible,
        )
    }

    pub(crate) fn reopen(&mut self) -> Result<()> {
        let epoch = TTS_MANAGER.realtime_epoch();
        let selected_pid = if self.per_app {
            let pid = SELECTED_APP_PID.load(Ordering::SeqCst);
            anyhow::ensure!(pid != 0, "S2S audio capture target is unavailable");
            Some(pid)
        } else {
            None
        };
        self.open_stream(selected_pid)?;
        self.epoch = epoch;
        self.seen_hidden = false;
        Ok(())
    }

    pub(crate) fn buffer(&self) -> &Arc<Mutex<Vec<i16>>> {
        &self.buffer
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(crate) fn playback_epoch(&self) -> Option<u64> {
        (!self.seen_hidden
            && crate::overlay::realtime_webview::state::TRANS_VISIBLE.load(Ordering::SeqCst)
            && self.epoch == TTS_MANAGER.realtime_epoch())
        .then_some(self.epoch)
    }
}

fn capture_needs_reopen(epoch: u64, current: u64, seen_hidden: bool, visible: bool) -> bool {
    visible && (seen_hidden || epoch != current)
}

struct CaptureSlot<T> {
    stream: Option<T>,
    retired: bool,
}

impl<T> Default for CaptureSlot<T> {
    fn default() -> Self {
        Self {
            stream: None,
            retired: false,
        }
    }
}

fn watch_capture<T: Send + 'static>(
    slot: Arc<Mutex<CaptureSlot<T>>>,
    should_retire: impl Fn() -> bool + Send + 'static,
) -> Result<CaptureWorker> {
    CaptureWorker::spawn("sgt-s2s-capture-owner", move |watcher_stop| {
        while !watcher_stop.load(Ordering::Acquire) && !should_retire() {
            std::thread::sleep(Duration::from_millis(20));
        }
        let stream = {
            let mut slot = slot.lock().unwrap_or_else(|error| error.into_inner());
            slot.retired = true;
            slot.stream.take()
        };
        drop(stream);
    })
}

fn replace_capture<T>(
    slot: &Mutex<CaptureSlot<T>>,
    buffer: &mut Arc<Mutex<Vec<i16>>>,
    open: impl FnOnce(Arc<Mutex<Vec<i16>>>) -> Result<T>,
) -> Result<()> {
    let old = {
        let mut slot = slot.lock().unwrap_or_else(|error| error.into_inner());
        anyhow::ensure!(!slot.retired, "S2S capture has been retired");
        slot.stream.take()
    };
    drop(old);
    *buffer = Arc::new(Mutex::new(Vec::new()));
    let new = open(buffer.clone())?;
    let mut slot = slot.lock().unwrap_or_else(|error| error.into_inner());
    anyhow::ensure!(!slot.retired, "S2S capture has been retired");
    slot.stream = Some(new);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DropNotice(std::sync::mpsc::Sender<()>);

    impl Drop for DropNotice {
        fn drop(&mut self) {
            let _ = self.0.send(());
        }
    }

    #[test]
    fn stop_releases_capture_while_network_work_is_blocked() {
        let (dropped_tx, dropped_rx) = std::sync::mpsc::channel();
        let slot = Arc::new(Mutex::new(CaptureSlot {
            stream: Some(DropNotice(dropped_tx)),
            retired: false,
        }));
        let stopped = Arc::new(AtomicBool::new(false));
        let watch_stop = stopped.clone();
        let watcher =
            watch_capture(slot.clone(), move || watch_stop.load(Ordering::SeqCst)).unwrap();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let network = std::thread::spawn(move || {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
        });
        entered_rx.recv().unwrap();
        stopped.store(true, Ordering::SeqCst);
        dropped_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("capture was not released");
        assert!(slot.lock().unwrap().retired);
        release_tx.send(()).unwrap();
        network.join().unwrap();
        drop(watcher);
    }

    #[test]
    fn late_capture_open_cannot_reinstall_after_retirement() {
        let slot = Arc::new(Mutex::new(CaptureSlot::<DropNotice>::default()));
        let stopped = Arc::new(AtomicBool::new(false));
        let watch_stop = stopped.clone();
        let watcher =
            watch_capture(slot.clone(), move || watch_stop.load(Ordering::SeqCst)).unwrap();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let (dropped_tx, dropped_rx) = std::sync::mpsc::channel();
        let opening_slot = slot.clone();
        let opener = std::thread::spawn(move || {
            let mut buffer = Arc::new(Mutex::new(Vec::new()));
            replace_capture(&opening_slot, &mut buffer, |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(DropNotice(dropped_tx))
            })
        });
        entered_rx.recv().unwrap();
        stopped.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !slot.lock().unwrap().retired && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(slot.lock().unwrap().retired);
        release_tx.send(()).unwrap();
        assert!(opener.join().unwrap().is_err());
        dropped_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(slot.lock().unwrap().stream.is_none());
        drop(watcher);
    }

    #[test]
    fn replacement_isolates_late_old_writes_and_failed_open_has_no_fallback() {
        let stream = Mutex::new(CaptureSlot {
            stream: Some(()),
            retired: false,
        });
        let mut buffer = Arc::new(Mutex::new(vec![1]));
        let old = buffer.clone();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let producer = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            old.lock().unwrap().push(2);
        });
        ready_rx.recv().unwrap();
        replace_capture(&stream, &mut buffer, |_| Ok(())).unwrap();
        release_tx.send(()).unwrap();
        producer.join().unwrap();
        assert!(buffer.lock().unwrap().is_empty());
        assert!(
            replace_capture(&stream, &mut buffer, |_| -> Result<()> {
                anyhow::bail!("failed")
            })
            .is_err()
        );
        assert!(stream.lock().unwrap().stream.is_none());
        assert!(buffer.lock().unwrap().is_empty());
    }

    #[test]
    fn reopen_detects_hidden_start_and_coalesced_visibility_changes() {
        assert!(!capture_needs_reopen(4, 4, false, true));
        assert!(!capture_needs_reopen(4, 5, true, false));
        assert!(capture_needs_reopen(4, 5, false, true));
        assert!(capture_needs_reopen(5, 5, true, true));
    }
}
