//! Canonical AI text/translation provider identifiers.
//!
//! Provider routing was open-coded as `provider == "google"` string comparisons
//! scattered across the translate/refine/vision/transcription dispatchers, with the
//! same wire strings re-spelled at every site (and easy to typo into a silent
//! route-to-default). [`Provider`] is the single source of truth: parse the wire
//! string once at a dispatch boundary with [`Provider::from_wire`] and match the
//! enum, so a misspelled variant is a compile error.

/// An AI text/translation provider, parsed from the wire string used in config/IPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Google,
    GeminiLive,
    GoogleGtx,
    Cerebras,
    OpenRouter,
    Groq,
    Taalas,
    Ollama,
}

impl Provider {
    /// Parse the wire string. Returns `None` for unknown values; callers map that to
    /// their own default provider, preserving the prior `else`-branch behavior.
    pub fn from_wire(s: &str) -> Option<Self> {
        Some(match s {
            "google" => Self::Google,
            "gemini-live" => Self::GeminiLive,
            "google-gtx" => Self::GoogleGtx,
            "cerebras" => Self::Cerebras,
            "openrouter" => Self::OpenRouter,
            "groq" => Self::Groq,
            "taalas" => Self::Taalas,
            "ollama" => Self::Ollama,
            _ => return None,
        })
    }
}
