use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

pub(super) const ROTATE_AT: Duration = Duration::from_secs(9 * 60);
const SPEECH_RMS: f32 = 0.015;
const TRAILING_AUDIO: Duration = Duration::from_millis(180);
const END_SILENCE: Duration = Duration::from_millis(420);
const MAX_VOCABULARY: usize = 1_000;

static VOCABULARY: LazyLock<Mutex<Vec<String>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static VOCABULARY_VERSION: AtomicU64 = AtomicU64::new(0);

pub(super) fn compute_i16_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| (s as f64 / 32768.0).powi(2)).sum();
    (sum_sq / samples.len() as f64).sqrt() as f32
}

pub(super) fn samples_to_ms(samples: usize) -> usize {
    samples.saturating_mul(1_000) / 16_000
}

pub(super) fn uses_periodic_silence_cycle(uses_interim_transcripts: bool) -> bool {
    !uses_interim_transcripts
}

pub(crate) fn vocabulary_snapshot() -> (u64, Vec<String>) {
    (
        VOCABULARY_VERSION.load(Ordering::SeqCst),
        VOCABULARY.lock().unwrap().clone(),
    )
}

pub(crate) fn set_vocabulary(lines: &str) {
    let mut normalized = Vec::new();
    for value in lines
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if !normalized.iter().any(|known| known == value) {
            normalized.push(value.to_string());
        }
        if normalized.len() == MAX_VOCABULARY {
            break;
        }
    }
    let mut current = VOCABULARY.lock().unwrap();
    if *current != normalized {
        *current = normalized;
        VOCABULARY_VERSION.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub(super) struct HybridVad {
    turn_active: bool,
    last_speech_at: Option<Instant>,
    end_sent: bool,
}

impl HybridVad {
    pub(super) fn is_safe_gap(&self) -> bool {
        !self.turn_active
    }

    pub(super) fn observe(&mut self, rms: f32, now: Instant) -> bool {
        if rms >= SPEECH_RMS {
            self.turn_active = true;
            self.last_speech_at = Some(now);
            self.end_sent = false;
            return false;
        }
        self.poll_end(now)
    }

    pub(super) fn poll_end(&mut self, now: Instant) -> bool {
        if !self.turn_active || self.end_sent {
            return false;
        }
        let silent_for = self.last_speech_at.map(|at| now.duration_since(at));
        if silent_for.is_some_and(|duration| duration <= TRAILING_AUDIO) {
            return false;
        }
        if silent_for.is_some_and(|duration| duration >= END_SILENCE) {
            self.turn_active = false;
            self.last_speech_at = None;
            self.end_sent = true;
            return true;
        }
        false
    }

    pub(super) fn reset_connection(&mut self) {
        self.turn_active = false;
        self.last_speech_at = None;
        self.end_sent = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_constants_match_parity_fixture() {
        let value: serde_json::Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/parity-fixtures/gemini-transcribe-lifecycle/contract.json"
        )))
        .unwrap();
        assert_eq!(value["hybridVad"]["speechRms"], SPEECH_RMS);
        assert_eq!(
            value["hybridVad"]["trailingAudioMs"],
            TRAILING_AUDIO.as_millis() as u64
        );
        assert_eq!(
            value["hybridVad"]["endSilenceMs"],
            END_SILENCE.as_millis() as u64
        );
        assert_eq!(value["session"]["rotateAtMs"], ROTATE_AT.as_millis() as u64);
        assert_eq!(value["customVocabulary"]["maxEntries"], MAX_VOCABULARY);
    }

    #[test]
    fn vad_sends_one_end_per_speech_turn() {
        let start = Instant::now();
        let mut vad = HybridVad::default();
        assert!(!vad.observe(SPEECH_RMS, start));
        assert!(!vad.observe(0.0, start + END_SILENCE - Duration::from_millis(1)));
        assert!(vad.observe(0.0, start + END_SILENCE));
        assert!(!vad.observe(0.0, start + END_SILENCE + Duration::from_secs(1)));
        assert!(!vad.observe(SPEECH_RMS, start + Duration::from_secs(2)));
        assert!(vad.observe(0.0, start + Duration::from_secs(2) + END_SILENCE));
    }
}
