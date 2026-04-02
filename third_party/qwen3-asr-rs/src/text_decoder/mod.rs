mod cache;

use crate::config::TextDecoderConfig;
use crate::layers::{RmsNorm, TextDecoderLayer};
use crate::tensor::{DType, Tensor};
use crate::weights::get_weight;
use anyhow::Result;
use std::collections::HashMap;

pub use cache::{create_causal_mask, DecoderState, KvCache, KvCacheEntry, KvCacheMode};

/// Qwen3 Text Decoder model.
pub struct TextDecoder {
    embed_tokens: Tensor,
    layers: Vec<TextDecoderLayer>,
    norm: RmsNorm,
    lm_head_weight_t: Tensor,
    config: TextDecoderConfig,
}

impl TextDecoder {
    pub fn load(
        weights: &HashMap<String, Tensor>,
        prefix: &str,
        config: &TextDecoderConfig,
    ) -> Result<Self> {
        let embed_tokens = get_weight(weights, &format!("{}.embed_tokens", prefix), "weight")?;

        let mut layers = Vec::new();
        for i in 0..config.num_hidden_layers {
            layers.push(TextDecoderLayer::load(
                weights,
                &format!("{}.layers.{}", prefix, i),
                config.num_attention_heads,
                config.num_key_value_heads,
                config.head_dim,
                config.rms_norm_eps,
            )?);
        }

        let norm = RmsNorm::load(weights, &format!("{}.norm", prefix), config.rms_norm_eps)?;
        let lm_head_key = format!("{}", prefix.replace(".model", ".lm_head"));
        let lm_head_weight = if config.tie_word_embeddings {
            embed_tokens.shallow_clone()
        } else {
            get_weight(weights, &lm_head_key, "weight")?
        };

        Ok(Self {
            embed_tokens,
            layers,
            norm,
            lm_head_weight_t: lm_head_weight.tr().contiguous(),
            config: config.clone(),
        })
    }

    pub fn embed(&self, input_ids: &Tensor) -> Tensor {
        Tensor::embedding(&self.embed_tokens, input_ids)
    }

    pub fn forward(
        &self,
        hidden_states: &Tensor,
        cos: &Tensor,
        sin: &Tensor,
        kv_cache: &mut KvCache,
        mask: Option<&Tensor>,
    ) -> Tensor {
        let mut hidden = hidden_states.shallow_clone();
        let kv_cache_mode = kv_cache.mode();

        for (i, layer) in self.layers.iter().enumerate() {
            hidden = layer.forward(&hidden, cos, sin, kv_cache.get_mut(i), kv_cache_mode, mask);
        }

        let hidden = self.norm.forward(&hidden);
        let hidden = if hidden.kind() == self.lm_head_weight_t.kind() {
            hidden
        } else {
            hidden.to_dtype(self.lm_head_weight_t.kind())
        };
        hidden.matmul(&self.lm_head_weight_t).to_dtype(DType::Float32)
    }

    pub fn prefill(
        &self,
        hidden_states: &Tensor,
        cos: &Tensor,
        sin: &Tensor,
        state: &mut DecoderState,
    ) -> Tensor {
        self.prefill_with_offload(hidden_states, cos, sin, state, false)
    }

    pub fn prefill_with_offload(
        &self,
        hidden_states: &Tensor,
        cos: &Tensor,
        sin: &Tensor,
        state: &mut DecoderState,
        finalize_offload: bool,
    ) -> Tensor {
        let seq_len = hidden_states.size()[1];
        let mask = create_causal_mask(seq_len, state.cached_seq_len(), hidden_states.device());
        let logits = self.forward(hidden_states, cos, sin, &mut state.kv_cache, Some(&mask));
        if finalize_offload && seq_len > 1 {
            state.kv_cache.finalize_prefill_offload();
        }
        let next_logits = logits.narrow(1, seq_len - 1, 1).squeeze_dim(1);
        state.last_logits = Some(next_logits.shallow_clone());
        state.advance_by(seq_len as usize);
        logits
    }

    pub fn decode_embedded(
        &self,
        hidden_states: &Tensor,
        cos: &Tensor,
        sin: &Tensor,
        state: &mut DecoderState,
    ) -> Tensor {
        let logits = self.forward(hidden_states, cos, sin, &mut state.kv_cache, None);
        let seq_len = hidden_states.size()[1];
        let next_logits = logits.narrow(1, seq_len - 1, 1).squeeze_dim(1);
        state.last_logits = Some(next_logits.shallow_clone());
        state.advance_by(hidden_states.size()[1] as usize);
        logits
    }

    pub fn config(&self) -> &TextDecoderConfig {
        &self.config
    }
}
