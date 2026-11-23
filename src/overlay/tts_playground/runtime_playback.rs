use std::sync::{LazyLock, Mutex};

use crate::api::tts::TTS_MANAGER;

use super::runtime::{PLAYBACK_TIMER, PlaybackTimer, cache_get};
use super::state;

static PLAYBACK_OWNER: LazyLock<Mutex<Option<u64>>> = LazyLock::new(|| Mutex::new(None));

fn replace_owned_playback<T>(start: impl FnOnce() -> (u64, T)) -> T {
    let mut owner = PLAYBACK_OWNER.lock().unwrap();
    if let Some(id) = owner.take() {
        TTS_MANAGER.stop_if_active(id);
    }
    let (id, result) = start();
    *owner = Some(id);
    result
}

pub(super) fn stop_owned_playback() {
    if let Some(id) = PLAYBACK_OWNER.lock().unwrap().take() {
        TTS_MANAGER.stop_if_active(id);
    }
}

pub(super) fn preview_profile(text: &str, profile: crate::api::tts::types::TtsRequestProfile) {
    replace_owned_playback(|| (TTS_MANAGER.speak_with_profile(text, 0, profile), ()));
}

pub(super) fn start_pcm_playback(
    pcm: Vec<i16>,
    sample_rate: u32,
    total_samples: usize,
    start_sample: usize,
) {
    let progress = replace_owned_playback(|| TTS_MANAGER.play_pcm(pcm, start_sample));
    *PLAYBACK_TIMER.lock().unwrap() = Some(PlaybackTimer {
        progress,
        start_sample,
        sample_rate,
        total_samples,
    });
}

/// Returns the position from frames consumed by the output device.
fn current_position_sec() -> f32 {
    let timer = PLAYBACK_TIMER.lock().unwrap();
    let Some(t) = timer.as_ref() else { return 0.0 };
    let frames = t.progress.played_frames();
    let elapsed_samples = ((frames as u128 * t.sample_rate as u128)
        / crate::api::tts::types::PLAYBACK_SAMPLE_RATE as u128)
        .min(usize::MAX as u128) as usize;
    let pos_sample = t.start_sample.saturating_add(elapsed_samples);
    (pos_sample.min(t.total_samples) as f32) / (t.sample_rate.max(1) as f32)
}

pub(super) fn play() {
    let Some(current) = state::with_state(|s| s.current.clone()) else {
        return;
    };
    let Some(audio) = cache_get(&current.id) else {
        return;
    };
    let resume_from_sec = state::with_state(|s| s.position_sec);
    let start_sample = ((resume_from_sec.max(0.0) * audio.sample_rate as f32) as usize)
        .min(audio.pcm_samples.len());
    state::with_state(|s| {
        s.is_playing = true;
        s.paused = false;
    });
    start_pcm_playback(
        audio.pcm_samples.clone(),
        audio.sample_rate,
        audio.pcm_samples.len(),
        start_sample,
    );
    state::sync_to_webview();
}

pub(super) fn pause() {
    let pos = current_position_sec();
    stop_owned_playback();
    state::with_state(|s| {
        s.is_playing = false;
        s.paused = true;
        s.position_sec = pos;
    });
    *PLAYBACK_TIMER.lock().unwrap() = None;
    state::sync_to_webview();
}

pub(super) fn stop() {
    stop_owned_playback();
    state::with_state(|s| {
        s.is_playing = false;
        s.paused = false;
        s.position_sec = 0.0;
    });
    *PLAYBACK_TIMER.lock().unwrap() = None;
    state::sync_to_webview();
}

pub(super) fn replay() {
    let Some(current) = state::with_state(|s| s.current.clone()) else {
        return;
    };
    let Some(audio) = cache_get(&current.id) else {
        return;
    };
    state::with_state(|s| {
        s.is_playing = true;
        s.paused = false;
        s.position_sec = 0.0;
    });
    let total = audio.pcm_samples.len();
    start_pcm_playback(audio.pcm_samples.clone(), audio.sample_rate, total, 0);
    state::sync_to_webview();
}

pub(super) fn seek(sec: f32) {
    let Some(current) = state::with_state(|s| s.current.clone()) else {
        return;
    };
    let Some(audio) = cache_get(&current.id) else {
        state::with_state(|s| s.position_sec = sec.max(0.0));
        state::sync_to_webview();
        return;
    };
    let was_playing = state::with_state(|s| s.is_playing);
    let clamped = sec.max(0.0).min(current.duration_sec);
    state::with_state(|s| {
        s.position_sec = clamped;
    });
    if was_playing {
        let start_sample =
            ((clamped * audio.sample_rate as f32) as usize).min(audio.pcm_samples.len());
        start_pcm_playback(
            audio.pcm_samples.clone(),
            audio.sample_rate,
            audio.pcm_samples.len(),
            start_sample,
        );
    } else {
        *PLAYBACK_TIMER.lock().unwrap() = None;
    }
    state::sync_to_webview();
}

pub(super) fn tick_position() {
    let pos = current_position_sec();
    let finished = PLAYBACK_TIMER
        .lock()
        .unwrap()
        .as_ref()
        .map(|timer| (timer.progress.is_finished(), timer.progress.completed()));
    let mut should_finish = false;
    state::with_state(|s| {
        if !s.is_playing {
            return;
        }
        s.position_sec = pos;
        if let Some((true, completed)) = finished {
            if completed && let Some(current) = &s.current {
                s.position_sec = current.duration_sec;
            }
            s.is_playing = false;
            s.paused = false;
            should_finish = true;
        }
    });
    if should_finish {
        *PLAYBACK_TIMER.lock().unwrap() = None;
    }
}
