use crate::tensor::{Device, Tensor};
use anyhow::{Context, Result};
use std::path::Path;

use crate::audio;
use crate::audio_encoder::AudioEncoder;
use crate::config::AsrConfig;
use crate::layers::compute_mrope_cos_sin;
use crate::mel::WhisperFeatureExtractor;
use crate::text_decoder::{DecoderState, TextDecoder};
use crate::tokenizer::{
    AsrTokenizer, ASR_TEXT_TOKEN_ID, AUDIO_END_TOKEN_ID, AUDIO_START_TOKEN_ID, ENDOFTEXT_TOKEN_ID,
    IM_END_TOKEN_ID, IM_START_TOKEN_ID,
};
use crate::weights;
#[path = "inference_generation.rs"]
mod generation;
use generation::{
    capitalize_first, parse_asr_output, parse_language_prefix, GenerationSession, ParsedGeneration,
    PromptPrefixCache, RopeCache,
};

const MEL_SAMPLE_RATE: u32 = 16000;
const DEFAULT_MAX_NEW_TOKENS: usize = 4096;
pub(crate) const DEFAULT_STREAMING_MAX_NEW_TOKENS: usize = 32;
const EXTRA_DECODE_POSITIONS: usize = 512;

/// ASR inference engine.
pub struct AsrInference {
    audio_encoder: AudioEncoder,
    text_decoder: TextDecoder,
    mel_extractor: WhisperFeatureExtractor,
    tokenizer: AsrTokenizer,
    config: AsrConfig,
    device: Device,
}

impl AsrInference {
    /// Load model from directory containing config.json, model.safetensors, tokenizer.json
    pub fn load(model_dir: &Path, device: Device) -> Result<Self> {
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

    fn base_prompt_token_ids(&self) -> Vec<i64> {
        vec![
            IM_START_TOKEN_ID,
            8948, // system
            198,
            IM_END_TOKEN_ID,
            198,
            IM_START_TOKEN_ID,
            872, // user
            198,
            AUDIO_START_TOKEN_ID,
        ]
    }

    fn suffix_prompt_token_ids(&self, language: Option<&str>) -> Result<Vec<i64>> {
        let mut tokens = vec![AUDIO_END_TOKEN_ID, IM_END_TOKEN_ID, 198];

        tokens.push(IM_START_TOKEN_ID);
        tokens.push(77091); // assistant
        tokens.push(198);

        if let Some(lang) = language {
            let prefix = format!("language {}", capitalize_first(lang));
            tokens.extend(self.tokenizer.encode(&prefix)?);
        }

        Ok(tokens)
    }

    pub fn decode_text_tokens(&self, ids: &[i64]) -> Result<String> {
        self.tokenizer.decode(ids)
    }

    pub(crate) fn encode_text_tokens(&self, text: &str) -> Result<Vec<i64>> {
        self.tokenizer.encode(text)
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

    pub(crate) fn encode_pcm16_samples(&self, samples: &[i16]) -> Result<Tensor> {
        let normalized: Vec<f32> = samples
            .iter()
            .map(|sample| *sample as f32 / 32768.0)
            .collect();
        self.encode_audio_samples(&normalized)
    }

    pub(crate) fn transcribe_audio_embeds(
        &self,
        audio_embeds: &Tensor,
        language: Option<&str>,
        sample_count: usize,
    ) -> Result<TranscribeResult> {
        let base_prefix = self.create_base_prefix_cache(language)?;
        let audio_prefix =
            self.extend_prefix_cache_with_audio(&base_prefix, audio_embeds, language)?;
        self.transcribe_from_audio_prefix_cache(
            &audio_prefix,
            language,
            sample_count,
            DEFAULT_MAX_NEW_TOKENS,
        )
    }

    pub(crate) fn create_base_prefix_cache(
        &self,
        language: Option<&str>,
    ) -> Result<PromptPrefixCache> {
        self.create_prompt_prefix_cache(language)
    }

    fn create_prompt_prefix_cache(&self, language: Option<&str>) -> Result<PromptPrefixCache> {
        let base_ids = self.base_prompt_token_ids();
        let max_total_positions = base_ids.len()
            + self.suffix_prompt_token_ids(language)?.len()
            + EXTRA_DECODE_POSITIONS;
        let rope = self.build_rope_cache(max_total_positions);
        let mut state = DecoderState::new(self.text_decoder.config().num_hidden_layers);
        let base_hidden = self.embed_text_tokens(&base_ids);
        let (cos, sin) = rope.slice(0, base_ids.len());
        self.text_decoder
            .prefill(&base_hidden, &cos, &sin, &mut state)
            .eval();
        Ok(PromptPrefixCache { state, rope })
    }

    pub(crate) fn extend_prefix_cache_with_audio(
        &self,
        prefix_cache: &PromptPrefixCache,
        audio_embeds: &Tensor,
        language: Option<&str>,
    ) -> Result<PromptPrefixCache> {
        self.extend_prompt_prefix_with_audio(
            prefix_cache,
            audio_embeds,
            self.suffix_prompt_token_ids(language)?.len(),
        )
    }

    fn extend_prompt_prefix_with_audio(
        &self,
        prefix_cache: &PromptPrefixCache,
        audio_embeds: &Tensor,
        suffix_token_count: usize,
    ) -> Result<PromptPrefixCache> {
        if audio_embeds.size()[0] == 0 {
            return Ok(prefix_cache.clone());
        }
        let mut cache = prefix_cache.clone();
        let additional_tokens = audio_embeds.size()[0] as usize;
        let next_position = cache.state.next_position();
        self.ensure_rope_capacity(
            &mut cache,
            next_position + additional_tokens,
            suffix_token_count + EXTRA_DECODE_POSITIONS,
        )?;
        let hidden_states = audio_embeds.unsqueeze(0);
        let (cos, sin) = cache.rope.slice(next_position, additional_tokens);
        self.text_decoder
            .prefill(&hidden_states, &cos, &sin, &mut cache.state)
            .eval();
        Ok(cache)
    }

    pub(crate) fn create_generation_prefix_cache(
        &self,
        audio_prefix_cache: &PromptPrefixCache,
        language: Option<&str>,
    ) -> Result<PromptPrefixCache> {
        let suffix_ids = self.suffix_prompt_token_ids(language)?;
        self.extend_prefix_cache_with_token_ids(
            audio_prefix_cache,
            &suffix_ids,
            EXTRA_DECODE_POSITIONS,
        )
    }

    pub(crate) fn extend_generation_prefix_with_token_ids(
        &self,
        generation_prefix_cache: &PromptPrefixCache,
        token_ids: &[i64],
    ) -> Result<PromptPrefixCache> {
        if token_ids.is_empty() {
            return Ok(generation_prefix_cache.clone());
        }
        self.extend_prefix_cache_with_token_ids(
            generation_prefix_cache,
            &token_ids,
            EXTRA_DECODE_POSITIONS,
        )
    }

    pub(crate) fn transcribe_from_audio_prefix_cache(
        &self,
        audio_prefix_cache: &PromptPrefixCache,
        language: Option<&str>,
        sample_count: usize,
        max_new_tokens: usize,
    ) -> Result<TranscribeResult> {
        let generation_prefix_cache =
            self.create_generation_prefix_cache(audio_prefix_cache, language)?;
        self.continue_transcription_from_generation_cache(
            &generation_prefix_cache,
            language.is_some(),
            sample_count,
            max_new_tokens,
        )
    }

    pub(crate) fn continue_transcription_from_generation_cache(
        &self,
        generation_prefix_cache: &PromptPrefixCache,
        language_forced: bool,
        sample_count: usize,
        max_new_tokens: usize,
    ) -> Result<TranscribeResult> {
        let duration_seconds = sample_count as f64 / MEL_SAMPLE_RATE as f64;
        let mut session = GenerationSession::from_prefilled_cache(self, generation_prefix_cache);
        let generated_ids = session.generate(max_new_tokens)?;
        tracing::info!("Generated {} tokens", generated_ids.len());
        let parsed = self.parse_generated_output(&generated_ids, language_forced)?;
        tracing::debug!("Raw output: {:?}", parsed.raw_output);

        Ok(TranscribeResult {
            text: parsed.transcription,
            text_token_ids: parsed.transcript_token_ids,
            raw_token_ids: parsed.transcript_generation_token_ids,
            raw_output_token_ids: generated_ids,
            language: parsed.language,
            raw_output: parsed.raw_output,
            duration_seconds,
        })
    }

    fn encode_audio_samples(&self, samples: &[f32]) -> Result<Tensor> {
        let mel = self.mel_extractor.extract(samples, self.device)?;
        let num_mel_frames = mel.size()[1] as usize;
        tracing::info!("Mel spectrogram: {} frames", num_mel_frames);

        let audio_embeds = self.audio_encoder.forward(&mel);
        audio_embeds.eval();
        tracing::info!("Audio encoder: {} tokens", audio_embeds.size()[0]);
        Ok(audio_embeds)
    }

    fn ensure_rope_capacity(
        &self,
        prefix_cache: &mut PromptPrefixCache,
        prefixed_token_count: usize,
        reserve_positions: usize,
    ) -> Result<()> {
        let required = prefixed_token_count + reserve_positions;
        if prefix_cache.rope.len() < required {
            prefix_cache.rope = self.build_rope_cache(required);
        }
        Ok(())
    }

    fn extend_prefix_cache_with_token_ids(
        &self,
        prefix_cache: &PromptPrefixCache,
        token_ids: &[i64],
        reserve_positions: usize,
    ) -> Result<PromptPrefixCache> {
        if token_ids.is_empty() {
            return Ok(prefix_cache.clone());
        }
        let mut cache = prefix_cache.clone();
        let next_position = cache.state.next_position();
        self.ensure_rope_capacity(
            &mut cache,
            next_position + token_ids.len(),
            reserve_positions,
        )?;
        let hidden_states = self.embed_text_tokens(token_ids);
        let (cos, sin) = cache.rope.slice(next_position, token_ids.len());
        self.text_decoder
            .prefill(&hidden_states, &cos, &sin, &mut cache.state)
            .eval();
        Ok(cache)
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
}
