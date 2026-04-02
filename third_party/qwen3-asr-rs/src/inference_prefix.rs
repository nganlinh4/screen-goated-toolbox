use super::{
    AsrInference, GenerationSession, PromptPrefixCache, TranscribeResult,
    DEFAULT_MAX_NEW_TOKENS, EXTRA_DECODE_POSITIONS, MEL_SAMPLE_RATE,
};
use crate::tensor::Tensor;
use anyhow::Result;

impl AsrInference {
    pub(crate) fn transcribe_audio_embeds(
        &self,
        audio_embeds: &Tensor,
        language: Option<&str>,
        sample_count: usize,
    ) -> Result<TranscribeResult> {
        tracing::info!(samples = sample_count, kv_mode = %super::kv_cache_mode_name(self.kv_cache_mode), "starting transcribe_audio_embeds");
        let suffix_ids = self.suffix_prompt_token_ids(language)?;
        let suffix_token_count = suffix_ids.len();
        let base_prefix = self.create_prompt_prefix_cache_with_suffix_len(suffix_token_count)?;
        tracing::info!(samples = sample_count, kv_mode = %super::kv_cache_mode_name(self.kv_cache_mode), "base prefix ready");
        let audio_prefix =
            self.extend_owned_prefix_cache_with_audio_with_suffix_len(
                base_prefix,
                audio_embeds,
                suffix_token_count,
            )?;
        tracing::info!(samples = sample_count, kv_mode = %super::kv_cache_mode_name(self.kv_cache_mode), "audio prefix ready");
        self.transcribe_from_owned_audio_prefix_cache(
            audio_prefix,
            language,
            sample_count,
            DEFAULT_MAX_NEW_TOKENS,
            suffix_ids,
        )
    }

    pub(crate) fn create_base_prefix_cache(
        &self,
        language: Option<&str>,
    ) -> Result<PromptPrefixCache> {
        self.create_prompt_prefix_cache_with_suffix_len(
            self.suffix_prompt_token_ids(language)?.len(),
        )
    }

    fn create_prompt_prefix_cache_with_suffix_len(
        &self,
        suffix_token_count: usize,
    ) -> Result<PromptPrefixCache> {
        let base_ids = self.base_prompt_token_ids();
        let max_total_positions = base_ids.len() + suffix_token_count + EXTRA_DECODE_POSITIONS;
        let rope = self.rope_cache(max_total_positions);
        let mut state = crate::text_decoder::DecoderState::new(
            self.text_decoder.config().num_hidden_layers,
            self.kv_cache_mode,
        );
        let base_hidden = self.embed_text_tokens(&base_ids);
        let (cos, sin) = rope.slice(0, base_ids.len());
        self.text_decoder
            .prefill_with_offload(&base_hidden, &cos, &sin, &mut state, false)
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

    pub(crate) fn extend_owned_prefix_cache_with_audio(
        &self,
        prefix_cache: PromptPrefixCache,
        audio_embeds: &Tensor,
        language: Option<&str>,
    ) -> Result<PromptPrefixCache> {
        self.extend_prompt_prefix_with_audio_owned(
            prefix_cache,
            audio_embeds,
            self.suffix_prompt_token_ids(language)?.len(),
        )
    }

    fn extend_owned_prefix_cache_with_audio_with_suffix_len(
        &self,
        prefix_cache: PromptPrefixCache,
        audio_embeds: &Tensor,
        suffix_token_count: usize,
    ) -> Result<PromptPrefixCache> {
        self.extend_prompt_prefix_with_audio_owned(prefix_cache, audio_embeds, suffix_token_count)
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
        let additional_tokens = audio_embeds.size()[0] as usize;
        let mut cache = prefix_cache.deep_copy_with_reserve(additional_tokens);
        self.extend_prompt_prefix_with_audio_in_place(
            &mut cache,
            audio_embeds,
            suffix_token_count,
        )?;
        Ok(cache)
    }

    fn extend_prompt_prefix_with_audio_owned(
        &self,
        prefix_cache: PromptPrefixCache,
        audio_embeds: &Tensor,
        suffix_token_count: usize,
    ) -> Result<PromptPrefixCache> {
        if audio_embeds.size()[0] == 0 {
            return Ok(prefix_cache);
        }
        let additional_tokens = audio_embeds.size()[0] as usize;
        let mut cache = PromptPrefixCache {
            state: prefix_cache.state.into_with_reserve(additional_tokens),
            rope: prefix_cache.rope,
        };
        self.extend_prompt_prefix_with_audio_in_place(
            &mut cache,
            audio_embeds,
            suffix_token_count,
        )?;
        Ok(cache)
    }

    fn extend_prompt_prefix_with_audio_in_place(
        &self,
        cache: &mut PromptPrefixCache,
        audio_embeds: &Tensor,
        suffix_token_count: usize,
    ) -> Result<()> {
        let additional_tokens = audio_embeds.size()[0] as usize;
        let next_position = cache.state.next_position();
        self.ensure_rope_capacity(
            cache,
            next_position + additional_tokens,
            suffix_token_count + EXTRA_DECODE_POSITIONS,
        )?;
        let hidden_states = audio_embeds.unsqueeze(0);
        let (cos, sin) = cache.rope.slice(next_position, additional_tokens);
        self.text_decoder
            .prefill_with_offload(&hidden_states, &cos, &sin, &mut cache.state, false)
            .eval();
        Ok(())
    }

    pub(crate) fn prepare_owned_generation_prefix_cache_with_suffix_ids(
        &self,
        audio_prefix_cache: PromptPrefixCache,
        suffix_ids: &[i64],
        extra_token_ids: &[i64],
    ) -> Result<PromptPrefixCache> {
        let total_additional_tokens = suffix_ids.len() + extra_token_ids.len();
        let mut cache = audio_prefix_cache.into_with_reserve(total_additional_tokens);
        if extra_token_ids.is_empty() {
            self.append_token_ids_to_prefix_cache_in_place(
                &mut cache,
                suffix_ids,
                EXTRA_DECODE_POSITIONS,
            )?;
        } else {
            let mut generation_token_ids = Vec::with_capacity(total_additional_tokens);
            generation_token_ids.extend_from_slice(suffix_ids);
            generation_token_ids.extend_from_slice(extra_token_ids);
            self.append_token_ids_to_prefix_cache_in_place(
                &mut cache,
                &generation_token_ids,
                EXTRA_DECODE_POSITIONS,
            )?;
        }
        let cache = cache.into_generation_ready(0);
        tracing::info!(
            kv_mode = %super::kv_cache_mode_name(self.kv_cache_mode),
            suffix_tokens = suffix_ids.len(),
            extra_tokens = extra_token_ids.len(),
            "generation-ready prefix cache prepared"
        );
        Ok(cache)
    }

    pub(crate) fn transcribe_from_owned_audio_prefix_cache(
        &self,
        audio_prefix_cache: PromptPrefixCache,
        language: Option<&str>,
        sample_count: usize,
        max_new_tokens: usize,
        suffix_ids: Vec<i64>,
    ) -> Result<TranscribeResult> {
        let generation_prefix_cache = self.prepare_owned_generation_prefix_cache_with_suffix_ids(
            audio_prefix_cache,
            &suffix_ids,
            &[],
        )?;
        self.transcribe_from_owned_generation_prefix_cache(
            generation_prefix_cache,
            language.is_some(),
            sample_count,
            max_new_tokens,
        )
    }

    fn transcribe_from_owned_generation_prefix_cache(
        &self,
        generation_prefix_cache: PromptPrefixCache,
        language_forced: bool,
        sample_count: usize,
        max_new_tokens: usize,
    ) -> Result<TranscribeResult> {
        let duration_seconds = sample_count as f64 / MEL_SAMPLE_RATE as f64;
        let mut session =
            GenerationSession::from_owned_prefilled_cache(self, generation_prefix_cache);
        let effective_max_new_tokens = super::effective_max_new_tokens(max_new_tokens);
        tracing::info!(
            samples = sample_count,
            kv_mode = %super::kv_cache_mode_name(self.kv_cache_mode),
            max_new_tokens,
            effective_max_new_tokens,
            "starting generation session"
        );
        let generated_ids = session.generate(effective_max_new_tokens)?;
        session.finalize_kv_offload();
        let kv_cache_bytes = session.total_cache_bytes();
        let kv_cache_dense_bytes = session.dense_equivalent_cache_bytes();
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
            kv_cache_bytes,
            kv_cache_dense_bytes,
        })
    }

    pub(crate) fn continue_transcription_from_generation_cache(
        &self,
        generation_prefix_cache: PromptPrefixCache,
        language_forced: bool,
        sample_count: usize,
        max_new_tokens: usize,
    ) -> Result<TranscribeResult> {
        self.transcribe_from_owned_generation_prefix_cache(
            generation_prefix_cache,
            language_forced,
            sample_count,
            max_new_tokens,
        )
    }

    fn ensure_rope_capacity(
        &self,
        prefix_cache: &mut PromptPrefixCache,
        prefixed_token_count: usize,
        reserve_positions: usize,
    ) -> Result<()> {
        let required = prefixed_token_count + reserve_positions;
        if prefix_cache.rope.len() < required {
            prefix_cache.rope = self.rope_cache(required);
        }
        Ok(())
    }

    fn append_token_ids_to_prefix_cache_in_place(
        &self,
        prefix_cache: &mut PromptPrefixCache,
        token_ids: &[i64],
        reserve_positions: usize,
    ) -> Result<()> {
        if token_ids.is_empty() {
            return Ok(());
        }
        let next_position = prefix_cache.state.next_position();
        self.ensure_rope_capacity(
            prefix_cache,
            next_position + token_ids.len(),
            reserve_positions,
        )?;
        let hidden_states = self.embed_text_tokens(token_ids);
        let (cos, sin) = prefix_cache.rope.slice(next_position, token_ids.len());
        self.text_decoder
            .prefill_with_offload(&hidden_states, &cos, &sin, &mut prefix_cache.state, false)
            .eval();
        Ok(())
    }

    // ---- Legacy streaming API (re-encodes all audio every chunk) ----

    pub(crate) fn create_generation_prefix_cache(
        &self,
        audio_prefix_cache: &PromptPrefixCache,
        language: Option<&str>,
    ) -> Result<PromptPrefixCache> {
        let suffix_ids = self.suffix_prompt_token_ids(language)?;
        let additional_tokens = suffix_ids.len();
        let mut cache = audio_prefix_cache.deep_copy_with_reserve(additional_tokens);
        self.append_token_ids_to_prefix_cache_in_place(
            &mut cache,
            &suffix_ids,
            EXTRA_DECODE_POSITIONS,
        )?;
        Ok(cache)
    }

    pub(crate) fn extend_generation_prefix_with_token_ids(
        &self,
        generation_prefix_cache: &PromptPrefixCache,
        token_ids: &[i64],
    ) -> Result<PromptPrefixCache> {
        if token_ids.is_empty() {
            return Ok(generation_prefix_cache.deep_copy_with_reserve(0));
        }
        let mut cache = generation_prefix_cache.deep_copy_with_reserve(token_ids.len());
        self.append_token_ids_to_prefix_cache_in_place(
            &mut cache,
            token_ids,
            EXTRA_DECODE_POSITIONS,
        )?;
        Ok(cache)
    }

    pub(crate) fn continue_transcription_from_ref_generation_cache(
        &self,
        generation_prefix_cache: &PromptPrefixCache,
        language_forced: bool,
        sample_count: usize,
        max_new_tokens: usize,
    ) -> Result<TranscribeResult> {
        let owned = generation_prefix_cache.deep_copy_with_reserve(max_new_tokens);
        self.transcribe_from_owned_generation_prefix_cache(
            owned,
            language_forced,
            sample_count,
            max_new_tokens,
        )
    }

    pub(crate) fn encode_text_tokens(&self, text: &str) -> Result<Vec<i64>> {
        self.tokenizer.encode(text)
    }
}
