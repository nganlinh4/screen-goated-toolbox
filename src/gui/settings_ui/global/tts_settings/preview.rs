use crate::gui::locale::LocaleText;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static LAST_TTS_PREVIEW_IDX: AtomicUsize = AtomicUsize::new(9999);

/// Speak a randomized preview line for the given speaker, avoiding repeating the
/// previously used line back-to-back. Falls back to a fixed English line when no
/// localized preview texts are configured.
pub(super) fn speak_settings_preview(text: &LocaleText, speaker_name: &str) {
    if !text.tts_preview_texts.is_empty() {
        let s = RandomState::new();
        let mut hasher = s.build_hasher();
        hasher.write_usize(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos() as usize,
        );
        let rand_val = hasher.finish();
        let len = text.tts_preview_texts.len();
        let mut idx = (rand_val as usize) % len;

        let last = LAST_TTS_PREVIEW_IDX.load(Ordering::Relaxed);
        if idx == last {
            idx = (idx + 1) % len;
        }
        LAST_TTS_PREVIEW_IDX.store(idx, Ordering::Relaxed);

        let preview_text = text.tts_preview_texts[idx].replace("{}", speaker_name);
        crate::api::tts::TTS_MANAGER.speak_interrupt(&preview_text, 0);
    } else {
        let preview_text = format!("Hello, I am {}. This is a voice preview.", speaker_name);
        crate::api::tts::TTS_MANAGER.speak_interrupt(&preview_text, 0);
    }
}
