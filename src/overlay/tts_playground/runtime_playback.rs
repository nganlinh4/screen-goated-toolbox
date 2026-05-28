use std::time::Instant;

use crate::api::tts::TTS_MANAGER;

use super::runtime::{PLAYBACK_TIMER, PlaybackTimer, cache_get};
use super::state;

pub(super) fn start_pcm_playback(
    pcm: Vec<i16>,
    sample_rate: u32,
    total_samples: usize,
    start_sample: usize,
) {
    TTS_MANAGER.play_pcm_interrupt(pcm, start_sample);
    *PLAYBACK_TIMER.lock().unwrap() = Some(PlaybackTimer {
        start: Instant::now(),
        start_sample,
        sample_rate,
        total_samples,
    });
}

/// Returns the current playback position in seconds based on wall-clock since
/// the last `start_pcm_playback` call. Caller is responsible for clamping.
fn current_position_sec() -> f32 {
    let timer = PLAYBACK_TIMER.lock().unwrap();
    let Some(t) = timer.as_ref() else { return 0.0 };
    let elapsed = t.start.elapsed().as_secs_f32();
    let pos_sample = t
        .start_sample
        .saturating_add((elapsed * t.sample_rate as f32) as usize);
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
    TTS_MANAGER.stop();
    state::with_state(|s| {
        s.is_playing = false;
        s.paused = true;
        s.position_sec = pos;
    });
    *PLAYBACK_TIMER.lock().unwrap() = None;
    state::sync_to_webview();
}

pub(super) fn stop() {
    TTS_MANAGER.stop();
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
    let mut should_finish = false;
    state::with_state(|s| {
        if !s.is_playing {
            return;
        }
        s.position_sec = pos;
        if let Some(current) = &s.current {
            if pos >= current.duration_sec {
                s.position_sec = current.duration_sec;
                s.is_playing = false;
                s.paused = false;
                should_finish = true;
            }
        }
    });
    if should_finish {
        *PLAYBACK_TIMER.lock().unwrap() = None;
    }
}
