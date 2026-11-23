/// Output audio sample rate from Gemini (24kHz)
pub const SOURCE_SAMPLE_RATE: u32 = 24000;

/// Playback sample rate (48kHz - most devices support this)
#[cfg(not(feature = "recorder-worker"))]
pub const PLAYBACK_SAMPLE_RATE: u32 = 48000;

/// Events passed from socket workers to the player thread
pub enum AudioEvent {
    Data(Vec<u8>),
    Error(String),
    End,
}

/// Fully collected TTS audio for artifact-style callers.
#[derive(Clone)]
pub struct TtsCollectedAudio {
    pub pcm_samples: Vec<i16>,
    pub wav_data: Vec<u8>,
    pub sample_rate: u32,
    pub duration_ms: u64,
}

/// Request paired with its generation ID (to handle interrupts)
#[derive(Clone)]
pub struct QueuedRequest {
    pub req: TtsRequest,
    pub generation: u64,
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub artifact: bool,
}

impl QueuedRequest {
    pub fn is_cancelled(&self, manager: &super::manager::TtsManager) -> bool {
        use std::sync::atomic::Ordering;
        self.cancel.load(Ordering::SeqCst)
            || manager.shutdown.load(Ordering::SeqCst)
            || (!self.artifact
                && self.generation < manager.interrupt_generation.load(Ordering::SeqCst))
    }
}

/// Per-request TTS settings for sandboxed callers such as TTS Playground.
#[derive(Clone)]
pub struct TtsRequestProfile {
    pub method: crate::config::TtsMethod,
    pub gemini_model: String,
    pub gemini_voice: String,
    pub gemini_speed: String,
    pub gemini_instruction: String,
    pub gemini_language_conditions: Vec<crate::config::TtsLanguageCondition>,
    #[cfg(feature = "recorder-worker")]
    pub gemini_parallel_requests: usize,
    pub google_speed: String,
    #[cfg(feature = "recorder-worker")]
    pub edge_voice: String,
    pub edge_settings: crate::config::EdgeTtsSettings,
    /// Optional ISO 639-3 language hint for batched callers such as subtitle narration.
    pub language_code_override: Option<String>,
}

impl From<&crate::config::TtsPlaygroundSettings> for TtsRequestProfile {
    fn from(settings: &crate::config::TtsPlaygroundSettings) -> Self {
        Self {
            method: settings.method.clone(),
            gemini_model: settings.gemini_model.clone(),
            gemini_voice: settings.gemini_voice.clone(),
            gemini_speed: settings.gemini_speed.clone(),
            gemini_instruction: settings.gemini_instruction.clone(),
            gemini_language_conditions: settings.gemini_language_conditions.clone(),
            #[cfg(feature = "recorder-worker")]
            gemini_parallel_requests: 2,
            google_speed: settings.google_speed.clone(),
            #[cfg(feature = "recorder-worker")]
            edge_voice: settings.edge_voice.clone(),
            edge_settings: settings.edge_settings.clone(),
            language_code_override: None,
        }
    }
}

/// TTS request with unique ID for cancellation
#[derive(Clone)]
pub struct TtsRequest {
    pub _id: u64,
    pub text: String,
    pub hwnd: isize,       // Window handle to update state when audio starts
    pub is_realtime: bool, // True if this is from realtime translation (uses REALTIME_TTS_SPEED)
    pub profile: Option<TtsRequestProfile>,
}
