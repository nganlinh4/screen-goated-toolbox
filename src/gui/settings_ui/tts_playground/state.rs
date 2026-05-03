use crate::config::TtsMethod;
use std::collections::VecDeque;
use std::sync::mpsc::Receiver;
use std::time::Instant;

pub(super) const MAX_RECENT_ARTIFACTS: usize = 5;

#[derive(Clone)]
pub(super) struct TtsPlaygroundArtifact {
    pub id: u64,
    pub text: String,
    pub method: TtsMethod,
    pub voice_label: String,
    pub pcm_samples: Vec<i16>,
    pub wav_data: Vec<u8>,
    pub sample_rate: u32,
    pub duration_ms: u64,
    pub latency_ms: u128,
    pub created_label: String,
}

impl TtsPlaygroundArtifact {
    pub fn duration_sec(&self) -> f32 {
        self.duration_ms as f32 / 1000.0
    }

    pub fn sample_for_sec(&self, sec: f32) -> usize {
        ((sec.max(0.0) * self.sample_rate as f32) as usize).min(self.pcm_samples.len())
    }

    pub fn size_label(&self) -> String {
        let bytes = self.wav_data.len() as f64;
        if bytes >= 1024.0 * 1024.0 {
            format!("{:.1} MB", bytes / 1024.0 / 1024.0)
        } else {
            format!("{:.0} KB", bytes / 1024.0)
        }
    }
}

pub(super) type ArtifactResult = Result<TtsPlaygroundArtifact, String>;
pub(super) type ExportResult = Result<String, String>;

pub struct TtsPlaygroundUiState {
    pub(super) current: Option<TtsPlaygroundArtifact>,
    pub(super) recent: VecDeque<TtsPlaygroundArtifact>,
    pub(super) job_rx: Option<Receiver<ArtifactResult>>,
    pub(super) is_generating: bool,
    pub(super) export_rx: Option<Receiver<ExportResult>>,
    pub(super) is_exporting: bool,
    pub(super) status: String,
    pub(super) error: Option<String>,
    pub(super) player: PlayerState,
}

impl TtsPlaygroundUiState {
    pub fn new() -> Self {
        let recent: VecDeque<_> = super::library::load_recent().into_iter().collect();
        let current = recent.front().cloned();
        Self {
            current,
            recent,
            job_rx: None,
            is_generating: false,
            export_rx: None,
            is_exporting: false,
            status: String::new(),
            error: None,
            player: PlayerState::default(),
        }
    }

    pub(super) fn push_recent(&mut self, artifact: TtsPlaygroundArtifact) {
        self.recent.retain(|item| item.id != artifact.id);
        self.recent.push_front(artifact);
        while self.recent.len() > MAX_RECENT_ARTIFACTS {
            self.recent.pop_back();
        }
        super::library::save_recent(&self.recent);
    }

    pub(super) fn delete_recent(&mut self, id: u64) {
        self.recent.retain(|item| item.id != id);
        if self.current.as_ref().map(|item| item.id) == Some(id) {
            self.current = self.recent.front().cloned();
            self.player.reset();
        }
        super::library::save_recent(&self.recent);
    }
}

impl Default for TtsPlaygroundUiState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
pub(super) struct PlayerState {
    pub playing: bool,
    pub paused_sample: usize,
    pub start_sample: usize,
    pub started_at: Option<Instant>,
}

impl PlayerState {
    pub fn current_sample(&self, sample_rate: u32, total_samples: usize) -> usize {
        if self.playing {
            let elapsed = self
                .started_at
                .map(|start| start.elapsed().as_secs_f32())
                .unwrap_or(0.0);
            let sample = self
                .start_sample
                .saturating_add((elapsed * sample_rate as f32) as usize);
            sample.min(total_samples)
        } else {
            self.paused_sample.min(total_samples)
        }
    }

    pub fn reset(&mut self) {
        self.playing = false;
        self.paused_sample = 0;
        self.start_sample = 0;
        self.started_at = None;
    }
}
