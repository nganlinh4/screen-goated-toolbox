use super::{AtomicBool, AudioEvent, Ordering, TtsManager, mpsc, next_request_id_for_internal_use};
use std::sync::Arc;

pub(crate) trait RealtimeAudioSink: Send + Sync {
    fn play(&self, bytes: &[u8]);
    fn stop(&self);
}

impl RealtimeAudioSink for crate::api::tts::player::audio_player::AudioPlayer {
    fn play(&self, bytes: &[u8]) {
        self.play_native_stream(bytes);
    }

    fn stop(&self) {
        crate::api::tts::player::audio_player::AudioPlayer::stop(self);
    }
}

impl TtsManager {
    pub(crate) fn admit_direct_realtime_audio(
        &self,
        sink: &Arc<dyn RealtimeAudioSink>,
        bytes: &[u8],
        session_stopped: &AtomicBool,
        visible: &AtomicBool,
        epoch: u64,
    ) -> bool {
        let _operation = self.playback_operations.lock().unwrap();
        if session_stopped.load(Ordering::SeqCst)
            || !visible.load(Ordering::SeqCst)
            || self.realtime_epoch.load(Ordering::SeqCst) != epoch
        {
            return false;
        }
        let mut registered = self.realtime_direct.lock().unwrap();
        if let Some(previous) = registered.as_ref().and_then(std::sync::Weak::upgrade)
            && !Arc::ptr_eq(&previous, sink)
        {
            previous.stop();
        }
        *registered = Some(Arc::downgrade(sink));
        sink.play(bytes);
        true
    }

    pub(crate) fn release_direct_realtime(&self, sink: &Arc<dyn RealtimeAudioSink>) {
        let _operation = self.playback_operations.lock().unwrap();
        let mut registered = self.realtime_direct.lock().unwrap();
        if registered
            .as_ref()
            .and_then(std::sync::Weak::upgrade)
            .is_some_and(|current| Arc::ptr_eq(&current, sink))
        {
            registered.take();
            sink.stop();
        }
    }

    pub(super) fn stop_direct_realtime(&self) {
        if let Some(sink) = self
            .realtime_direct
            .lock()
            .unwrap()
            .take()
            .and_then(|sink| sink.upgrade())
        {
            sink.stop();
        }
    }

    pub(crate) fn realtime_text_backlog(&self) -> usize {
        self.realtime_text_requests.lock().unwrap().len()
    }

    pub(crate) fn finish_realtime_text(&self, request_id: u64) {
        self.realtime_text_requests
            .lock()
            .unwrap()
            .remove(&request_id);
    }

    pub(super) fn halt_playback_device(&self) {
        let player = self.playback_device.lock().unwrap().upgrade();
        if let Some(player) = player {
            player.stop();
        }
    }
    /// Serializes a cancellation check and one playback admission with stop.
    pub(in crate::api::tts) fn admit_playback(
        &self,
        cancelled: impl FnOnce() -> bool,
        effect: impl FnOnce(),
    ) -> bool {
        let _operation = self.playback_operations.lock().unwrap();
        if cancelled() {
            return false;
        }
        effect();
        true
    }

    pub(crate) fn realtime_epoch(&self) -> u64 {
        self.realtime_epoch.load(Ordering::SeqCst)
    }

    /// Fence old session handoffs without waiting for an audio device operation.
    pub(crate) fn invalidate_realtime_epoch(&self) {
        self.realtime_epoch.fetch_add(1, Ordering::SeqCst);
    }

    /// Stop the realtime session's speech without affecting other owners.
    #[cfg(test)]
    pub(crate) fn stop_realtime(&self) {
        let removed = self.retire_realtime();
        self.notify_retired_playback(removed);
    }

    pub(crate) fn retire_realtime(&self) -> Vec<(isize, u64)> {
        let _operation = self.playback_operations.lock().unwrap();
        self.realtime_epoch.fetch_add(1, Ordering::SeqCst);
        self.retire_realtime_locked()
    }

    /// Retire only the realtime owner observed before an asynchronous handoff.
    pub(crate) fn retire_realtime_if_epoch(&self, expected: u64) -> Option<Vec<(isize, u64)>> {
        let _operation = self.playback_operations.lock().unwrap();
        if self
            .realtime_epoch
            .compare_exchange(
                expected,
                expected.wrapping_add(1),
                Ordering::SeqCst,
                Ordering::SeqCst,
            )
            .is_err()
        {
            return None;
        }
        Some(self.retire_realtime_locked())
    }

    fn retire_realtime_locked(&self) -> Vec<(isize, u64)> {
        self.realtime_text_requests.lock().unwrap().clear();
        self.stop_direct_realtime();
        let mut removed = Vec::new();
        let mut active_cancelled = false;
        {
            let mut queue = self.playback_queue.lock().unwrap();
            queue.retain(|item| {
                if item.4 {
                    item.5.store(true, Ordering::SeqCst);
                    removed.push((item.1, item.2));
                    false
                } else {
                    true
                }
            });
            if let Some((_, true, cancel)) = self.active_playback.lock().unwrap().as_ref() {
                cancel.store(true, Ordering::SeqCst);
                active_cancelled = true;
            }
        }
        if active_cancelled {
            self.halt_playback_device();
        }
        self.work_queue.lock().unwrap().retain(|(request, _)| {
            if !request.artifact && request.req.is_realtime {
                request.cancel.store(true, Ordering::SeqCst);
                false
            } else {
                true
            }
        });
        self.playback_signal.notify_all();
        removed
    }

    pub(crate) fn notify_retired_playback(&self, removed: Vec<(isize, u64)>) {
        for (hwnd, id) in removed {
            super::utils::clear_tts_state(hwnd, id);
        }
    }

    pub(super) fn clear_queued_playback(&self) {
        let removed: Vec<_> = self.playback_queue.lock().unwrap().drain(..).collect();
        for (_, hwnd, id, _, _, cancel, _) in removed {
            cancel.store(true, Ordering::SeqCst);
            self.pending_progress.lock().unwrap().remove(&id);
            super::utils::clear_tts_state(hwnd, id);
        }
    }

    /// Queue audio produced by a live session under the same identity namespace
    /// as synthesized speech.
    pub(crate) fn enqueue_audio(
        &self,
        rx: mpsc::Receiver<AudioEvent>,
        hwnd: isize,
        is_realtime: bool,
    ) -> u64 {
        let _operation = self.playback_operations.lock().unwrap();
        let id = next_request_id_for_internal_use();
        let generation = self.interrupt_generation.load(Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        self.playback_queue.lock().unwrap().push_back((
            rx,
            hwnd,
            id,
            generation,
            is_realtime,
            cancel,
            None,
        ));
        self.playback_signal.notify_one();
        id
    }

    pub(crate) fn enqueue_realtime_audio(
        &self,
        rx: mpsc::Receiver<AudioEvent>,
        hwnd: isize,
        session_stopped: &AtomicBool,
        visible: &AtomicBool,
        epoch: u64,
    ) -> Option<u64> {
        let _operation = self.playback_operations.lock().unwrap();
        if session_stopped.load(Ordering::SeqCst)
            || !visible.load(Ordering::SeqCst)
            || self.realtime_epoch.load(Ordering::SeqCst) != epoch
        {
            return None;
        }
        let id = next_request_id_for_internal_use();
        let generation = self.interrupt_generation.load(Ordering::SeqCst);
        let cancel = Arc::new(AtomicBool::new(false));
        self.playback_queue
            .lock()
            .unwrap()
            .push_back((rx, hwnd, id, generation, true, cancel, None));
        self.playback_signal.notify_one();
        Some(id)
    }

    /// Cancel only the named playback, whether it is active or still queued.
    pub fn stop_if_active(&self, request_id: u64) {
        if let Some(removed) = self.retire_if_active(request_id) {
            self.notify_retired_playback(vec![removed]);
        }
    }

    pub(crate) fn retire_if_active(&self, request_id: u64) -> Option<(isize, u64)> {
        if request_id == 0 {
            return None;
        }
        let _operation = self.playback_operations.lock().unwrap();
        let mut removed_hwnd = None;
        let mut found = false;
        let mut active_cancelled = false;
        {
            let mut queue = self.playback_queue.lock().unwrap();
            queue.retain(|item| {
                if item.2 == request_id {
                    found = true;
                    item.5.store(true, Ordering::SeqCst);
                    removed_hwnd = Some(item.1);
                    false
                } else {
                    true
                }
            });
            if let Some((_, _, cancel)) = self
                .active_playback
                .lock()
                .unwrap()
                .as_ref()
                .filter(|(id, _, _)| *id == request_id)
            {
                found = true;
                cancel.store(true, Ordering::SeqCst);
                active_cancelled = true;
            }
        }
        if active_cancelled {
            self.halt_playback_device();
        }
        if found {
            self.remove_queued_request(request_id);
            self.pending_progress.lock().unwrap().remove(&request_id);
            self.finish_realtime_text(request_id);
        }
        self.playback_signal.notify_all();
        removed_hwnd.map(|hwnd| (hwnd, request_id))
    }

    pub fn is_speaking(&self, request_id: u64) -> bool {
        if request_id == 0 {
            return false;
        }
        let queue = self.playback_queue.lock().unwrap();
        queue
            .iter()
            .any(|item| item.2 == request_id && !item.5.load(Ordering::SeqCst))
            || self
                .active_playback
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|(id, _, cancel)| *id == request_id && !cancel.load(Ordering::SeqCst))
    }
}
