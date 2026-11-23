use std::sync::atomic::Ordering;

use super::AudioPlayer;

impl AudioPlayer {
    pub(in crate::api::tts::player) fn flush_stretcher(
        &self,
        cancelled: &impl Fn() -> bool,
    ) -> bool {
        if cancelled() {
            self.stop();
            return false;
        }
        let Ok(mut stretcher) = self.wsola.lock() else {
            return false;
        };
        let tail = stretcher.finish();
        let volume = crate::overlay::realtime_webview::state::CURRENT_TTS_VOLUME
            .load(Ordering::Relaxed) as f32
            / 100.0;
        let Ok(mut buffer) = self.shared_buffer.lock() else {
            return false;
        };
        if cancelled() {
            buffer.clear();
            self.reset_generation.fetch_add(1, Ordering::SeqCst);
            self.deck_ready.notify_all();
            return false;
        }
        buffer.extend(tail.into_iter().flat_map(|sample| {
            let scaled = (sample as f32 * volume).clamp(-32768.0, 32767.0) as i16;
            [scaled, scaled]
        }));
        self.deck_ready.notify_all();
        true
    }
}
