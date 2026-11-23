pub(crate) mod audio;
pub(crate) mod client;
#[cfg(test)]
mod fork_parity_tests;
#[path = "../../../../src/api/gemini_live/mod.rs"]
pub(crate) mod gemini_live;
pub(crate) mod groq;
#[path = "../../../../src/api/ollama.rs"]
pub(crate) mod ollama;
#[path = "../../../../src/api/provider_credentials.rs"]
pub(crate) mod provider_credentials;
pub(crate) mod realtime_audio;
#[path = "../../../../src/api/reasoning_policy.rs"]
mod reasoning_policy;
#[path = "../../../../src/api/taalas.rs"]
pub(crate) mod taalas;
#[path = "../../../../src/api/tts/mod.rs"]
pub(crate) mod tts;

pub(crate) use reasoning_policy::{
    apply_ordinary_openai_reasoning_policy, apply_ordinary_openrouter_reasoning_policy,
    gemini_thinking_config,
};

pub(crate) const WIPE_SIGNAL: &str = "\x00WIPE\x00";
