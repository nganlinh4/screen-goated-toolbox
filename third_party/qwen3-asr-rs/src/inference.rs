use crate::tensor::{Device, Tensor};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use crate::audio;
use crate::audio_encoder::AudioEncoder;
use crate::config::AsrConfig;
use crate::layers::compute_mrope_cos_sin;
use crate::mel::WhisperFeatureExtractor;
pub use crate::text_decoder::KvCacheMode;
use crate::text_decoder::TextDecoder;
use crate::tokenizer::{
    AsrTokenizer, ASR_TEXT_TOKEN_ID, AUDIO_END_TOKEN_ID, AUDIO_START_TOKEN_ID, ENDOFTEXT_TOKEN_ID,
    IM_END_TOKEN_ID, IM_START_TOKEN_ID,
};
use crate::weights;
#[path = "inference_generation.rs"]
mod generation;
#[path = "inference_prefix.rs"]
mod prefix;
use generation::{
    capitalize_first, parse_asr_output, parse_language_prefix, GenerationSession, ParsedGeneration,
    RopeCache,
};
pub(crate) use generation::PromptPrefixCache;

const MEL_SAMPLE_RATE: u32 = 16000;
const DEFAULT_MAX_NEW_TOKENS: usize = 4096;
pub(crate) const DEFAULT_STREAMING_MAX_NEW_TOKENS: usize = 32;
const EXTRA_DECODE_POSITIONS: usize = 512;
const DEBUG_MAX_NEW_TOKENS_ENV: &str = "SGT_QWEN3_DEBUG_MAX_NEW_TOKENS";
pub const KV_CACHE_MODE_DENSE_APPEND: &str = "dense_append";
pub const KV_CACHE_MODE_EXPERIMENTAL_TURBOQUANT: &str = "experimental_turboquant";
pub const KV_CACHE_MODE_LEGACY_PAGED_INT8: &str = "paged_int8";

pub fn kv_cache_mode_name(mode: KvCacheMode) -> &'static str {
    match mode {
        KvCacheMode::DenseAppend => KV_CACHE_MODE_DENSE_APPEND,
        KvCacheMode::ExperimentalTurboQuant => KV_CACHE_MODE_EXPERIMENTAL_TURBOQUANT,
    }
}

pub fn supported_kv_cache_mode_names() -> [&'static str; 2] {
    [KV_CACHE_MODE_DENSE_APPEND, KV_CACHE_MODE_EXPERIMENTAL_TURBOQUANT]
}

pub fn kv_cache_mode_from_name(name: &str) -> Option<KvCacheMode> {
    match name {
        KV_CACHE_MODE_DENSE_APPEND => Some(KvCacheMode::DenseAppend),
        KV_CACHE_MODE_EXPERIMENTAL_TURBOQUANT | KV_CACHE_MODE_LEGACY_PAGED_INT8 => {
            Some(KvCacheMode::ExperimentalTurboQuant)
        }
        _ => None,
    }
}

fn effective_max_new_tokens(default_max_new_tokens: usize) -> usize {
    std::env::var(DEBUG_MAX_NEW_TOKENS_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(default_max_new_tokens))
        .unwrap_or(default_max_new_tokens)
}

pub fn kv_cache_mode_from_env() -> KvCacheMode {
    std::env::var("SGT_QWEN3_RUNTIME_KV_MODE")
        .ok()
        .and_then(|value| kv_cache_mode_from_name(value.trim()))
        .unwrap_or(KvCacheMode::DenseAppend)
}

/// ASR inference engine.
pub struct AsrInference {
    audio_encoder: AudioEncoder,
    text_decoder: TextDecoder,
    mel_extractor: WhisperFeatureExtractor,
    tokenizer: AsrTokenizer,
    config: AsrConfig,
    device: Device,
    kv_cache_mode: KvCacheMode,
    base_prompt_ids: Vec<i64>,
    suffix_prompt_cache: Mutex<HashMap<Option<String>, Vec<i64>>>,
    rope_cache: Mutex<Option<RopeCache>>,
}

impl AsrInference {
    /// Load model from directory containing config.json, model.safetensors, tokenizer.json
    pub fn load(model_dir: &Path, device: Device) -> Result<Self> {
        Self::load_with_kv_mode(model_dir, device, kv_cache_mode_from_env())
    }

    pub fn load_with_kv_mode(
        model_dir: &Path,
        device: Device,
        kv_cache_mode: KvCacheMode,
    ) -> Result<Self> {
        tracing::info!("Loading model from {:?}", model_dir);

        // Load config
        let config = AsrConfig::from_file(&model_dir.join("config.json"))
            .context("Failed to load config")?;

        // Load weights (supports both single-file and sharded safetensors)
        let all_weights =
            weights::load_model_weights(model_dir, device).context("Failed to load weights")?;

        tracing::info!("Loaded {} weight tensors", all_weights.len());

        // Load audio encoder
        tracing::info!("Loading audio encoder...");
        let audio_encoder = AudioEncoder::load(
            &all_weights,
            "thinker.audio_tower",
            &config.thinker_config.audio_config,
            device,
        )
        .context("Failed to load audio encoder")?;

        // Load text decoder
        tracing::info!("Loading text decoder...");
        let text_decoder = TextDecoder::load(
            &all_weights,
            "thinker.model",
            &config.thinker_config.text_config,
        )
        .context("Failed to load text decoder")?;

        // Load tokenizer
        tracing::info!("Loading tokenizer...");
        let tokenizer = AsrTokenizer::from_dir(model_dir).context("Failed to load tokenizer")?;

        // Create mel feature extractor
        let mel_extractor = WhisperFeatureExtractor::new(
            400, // n_fft
            160, // hop_length
            config.thinker_config.audio_config.num_mel_bins,
            MEL_SAMPLE_RATE,
            device,
        );

        tracing::info!("Model loaded successfully");

        Ok(Self {
            audio_encoder,
            text_decoder,
            mel_extractor,
            tokenizer,
            config,
            device,
            kv_cache_mode,
            base_prompt_ids: vec![
                IM_START_TOKEN_ID,
                8948, // system
                198,
                IM_END_TOKEN_ID,
                198,
                IM_START_TOKEN_ID,
                872, // user
                198,
                AUDIO_START_TOKEN_ID,
            ],
            suffix_prompt_cache: Mutex::new(HashMap::new()),
            rope_cache: Mutex::new(None),
        })
    }

    /// Transcribe an audio file.
    pub fn transcribe(&self, audio_path: &str, language: Option<&str>) -> Result<TranscribeResult> {
        // Step 1: Load and preprocess audio
        tracing::info!("Loading audio from {}", audio_path);
        let samples = audio::load_audio(audio_path, MEL_SAMPLE_RATE)?;
        self.transcribe_samples(&samples, language)
    }

    /// Transcribe pre-decoded mono PCM audio at 16kHz.
    pub fn transcribe_samples(
        &self,
        samples: &[f32],
        language: Option<&str>,
    ) -> Result<TranscribeResult> {
        let audio_embeds = self.encode_audio_samples(samples)?;
        self.transcribe_audio_embeds(&audio_embeds, language, samples.len())
    }

    /// Transcribe 16-bit mono PCM audio at 16kHz.
    pub fn transcribe_pcm16(
        &self,
        samples: &[i16],
        language: Option<&str>,
    ) -> Result<TranscribeResult> {
        let audio_embeds = self.encode_pcm16_samples(samples)?;
        self.transcribe_audio_embeds(&audio_embeds, language, samples.len())
    }

    fn base_prompt_token_ids(&self) -> &[i64] {
        &self.base_prompt_ids
    }

    pub(crate) fn suffix_prompt_token_ids(&self, language: Option<&str>) -> Result<Vec<i64>> {
        let cache_key = language.map(ToOwned::to_owned);
        if let Some(cached) = self
            .suffix_prompt_cache
            .lock()
            .expect("suffix prompt cache mutex poisoned")
            .get(&cache_key)
            .cloned()
        {
            return Ok(cached);
        }

        let mut tokens = vec![AUDIO_END_TOKEN_ID, IM_END_TOKEN_ID, 198];

        tokens.push(IM_START_TOKEN_ID);
        tokens.push(77091); // assistant
        tokens.push(198);

        if let Some(lang) = language {
            let prefix = format!("language {}", capitalize_first(lang));
            tokens.extend(self.tokenizer.encode(&prefix)?);
        }

        self.suffix_prompt_cache
            .lock()
            .expect("suffix prompt cache mutex poisoned")
            .insert(cache_key, tokens.clone());
        Ok(tokens)
    }

    pub fn decode_text_tokens(&self, ids: &[i64]) -> Result<String> {
        self.tokenizer.decode(ids)
    }

    pub(crate) fn parse_streaming_raw_output(
        &self,
        raw: &str,
        forced_language: Option<&str>,
    ) -> (String, String) {
        if let Some(language) = forced_language.filter(|value| !value.trim().is_empty()) {
            return (language.to_string(), raw.trim().to_string());
        }
        parse_asr_output(raw, false)
    }

    fn embed_text_tokens(&self, ids: &[i64]) -> Tensor {
        let input_tensor = Tensor::from_slice_i64(ids).to_device(self.device);
        self.text_decoder.embed(&input_tensor).unsqueeze(0)
    }

    pub(crate) fn encode_audio_samples(&self, samples: &[f32]) -> Result<Tensor> {
        let mel_features = self.mel_extractor.extract(samples, self.device)?;
        Ok(self.audio_encoder.forward(&mel_features))
    }

    pub(crate) fn encode_pcm16_samples(&self, samples: &[i16]) -> Result<Tensor> {
        let normalized: Vec<f32> = samples
            .iter()
            .map(|sample| *sample as f32 / 32768.0)
            .collect();
        self.encode_audio_samples(&normalized)
    }

    fn parse_generated_output(
        &self,
        generated_ids: &[i64],
        language_forced: bool,
    ) -> Result<ParsedGeneration> {
        let raw_output = self.tokenizer.decode(generated_ids)?;
        if language_forced {
            let transcription = raw_output.trim().to_string();
            return Ok(ParsedGeneration {
                language: "forced".to_string(),
                raw_output,
                transcription,
                transcript_token_ids: generated_ids.to_vec(),
                transcript_generation_token_ids: generated_ids.to_vec(),
            });
        }

        if let Some(asr_text_pos) = generated_ids.iter().position(|&id| id == ASR_TEXT_TOKEN_ID) {
            let language =
                parse_language_prefix(&self.tokenizer.decode(&generated_ids[..asr_text_pos])?);
            let transcript_generation_token_ids = generated_ids[asr_text_pos + 1..].to_vec();
            let transcript_token_ids = transcript_generation_token_ids.clone();
            let transcription = self
                .tokenizer
                .decode(&transcript_token_ids)?
                .trim()
                .to_string();
            return Ok(ParsedGeneration {
                language,
                raw_output,
                transcription,
                transcript_token_ids,
                transcript_generation_token_ids,
            });
        }

        let (language, transcription) = parse_asr_output(&raw_output, false);
        let transcript_token_ids = self.tokenizer.encode(&transcription)?;
        let transcript_generation_token_ids = transcript_token_ids.clone();
        Ok(ParsedGeneration {
            language,
            raw_output,
            transcription,
            transcript_token_ids,
            transcript_generation_token_ids,
        })
    }

    fn rope_cache(&self, max_total_positions: usize) -> RopeCache {
        let mut cache = self.rope_cache.lock().expect("rope cache mutex poisoned");
        let needs_refresh = cache
            .as_ref()
            .map_or(true, |rope| rope.len() < max_total_positions);
        if needs_refresh {
            *cache = Some(self.build_rope_cache(max_total_positions));
        }
        cache
            .as_ref()
            .expect("rope cache must exist after refresh")
            .clone()
    }

    fn build_rope_cache(&self, max_total_positions: usize) -> RopeCache {
        let text_config = &self.config.thinker_config.text_config;
        let all_positions: Vec<i64> = (0..max_total_positions as i64).collect();
        let all_pos_ids: [Vec<i64>; 3] =
            [all_positions.clone(), all_positions.clone(), all_positions];
        let (cos, sin) = compute_mrope_cos_sin(
            &all_pos_ids,
            text_config.head_dim,
            text_config.rope_theta,
            &text_config.mrope_section(),
            text_config.mrope_interleaved(),
            self.device,
        );
        RopeCache { cos, sin }
    }
}

/// Result of ASR transcription.
pub struct TranscribeResult {
    pub text: String,
    pub text_token_ids: Vec<i64>,
    pub raw_token_ids: Vec<i64>,
    pub raw_output_token_ids: Vec<i64>,
    pub language: String,
    pub raw_output: String,
    pub duration_seconds: f64,
    pub kv_cache_bytes: usize,
    pub kv_cache_dense_bytes: usize,
}
