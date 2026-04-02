use crate::tensor::{DType, Device, Tensor};
use crate::turboquant_kv::TurboQuantKvPrefix;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KvCacheMode {
    DenseAppend,
    ExperimentalTurboQuant,
}

#[derive(Clone)]
pub struct KvCache {
    mode: KvCacheMode,
    pub layers: Vec<Option<KvCacheEntry>>,
}

#[derive(Clone)]
pub struct KvCacheEntry {
    mode: KvCacheMode,
    quantized_prefix: Option<TurboQuantKvPrefix>,
    key: Tensor,
    value: Tensor,
    dense_len: i64,
    dense_capacity: i64,
}

impl KvCacheEntry {
    pub fn from_tokens(key: Tensor, value: Tensor, mode: KvCacheMode) -> Self {
        let dense_len = key.size()[2];
        Self {
            mode,
            quantized_prefix: None,
            key,
            value,
            dense_len,
            dense_capacity: dense_len.max(1),
        }
    }

    pub fn len(&self) -> i64 { self.prefix_len() + self.dense_len }
    pub(crate) fn dense_key_view(&self) -> Tensor { self.key.narrow(2, 0, self.dense_len) }
    pub(crate) fn dense_value_view(&self) -> Tensor { self.value.narrow(2, 0, self.dense_len) }

    pub fn attend_with_quantized_prefix(
        &self,
        q: &Tensor,
        scale: f64,
        mask: Option<&Tensor>,
    ) -> Option<Tensor> {
        let prefix = self.quantized_prefix.as_ref();
        let prefix_len = prefix.map(TurboQuantKvPrefix::key_len).unwrap_or(0);
        if prefix_len == 0 {
            if self.dense_len == 0 {
                return None;
            }
            return Some(self.dense_attention_output(q, scale, mask));
        }
        let prefix = prefix.expect("non-zero prefix length must have compressed prefix");
        if q.device() == Device::Cpu {
            return Some(self.cpu_attention_output_with_materialized_prefix(prefix, q, scale, mask));
        }

        let prefix_logits = prefix.estimate_key_scores(q, scale);
        let mut logits = if self.dense_len > 0 {
            let dense_logits = dense_tail_scores(q, &self.dense_key_view(), scale);
            Tensor::cat(&[prefix_logits, dense_logits], 3)
        } else {
            prefix_logits
        };
        if let Some(mask) = mask {
            logits = logits + mask;
        }

        let weights = logits.softmax(-1);
        let prefix_weights = weights.narrow(3, 0, prefix_len);
        let mut output = prefix.weighted_value_sum(&prefix_weights);
        if self.dense_len > 0 {
            let dense_weights = weights.narrow(3, prefix_len, self.dense_len);
            let dense_output = dense_tail_weighted_value_sum(&dense_weights, &self.dense_value_view());
            let output_kind = output.kind();
            output = if output_kind == dense_output.kind() {
                output + dense_output
            } else {
                output + dense_output.to_dtype(output_kind)
            };
        }

        let output_kind = output.kind();
        Some(if output_kind == weights.kind() {
            output
        } else {
            output.to_dtype(weights.kind())
        })
    }

    fn cpu_attention_output_with_materialized_prefix(
        &self,
        prefix: &TurboQuantKvPrefix,
        q: &Tensor,
        scale: f64,
        mask: Option<&Tensor>,
    ) -> Tensor {
        let prefix_key = prefix.key_tensor();
        let prefix_value = prefix.value_tensor();
        let key = if self.dense_len > 0 {
            Tensor::cat(&[prefix_key, self.dense_key_view()], 2)
        } else {
            prefix_key
        };
        let value = if self.dense_len > 0 {
            Tensor::cat(&[prefix_value, self.dense_value_view()], 2)
        } else {
            prefix_value
        };
        Tensor::scaled_dot_product_attention(q, &key, &value, scale, mask)
    }
    fn dense_attention_output(&self, q: &Tensor, scale: f64, mask: Option<&Tensor>) -> Tensor {
        Tensor::scaled_dot_product_attention(q, &self.dense_key_view(), &self.dense_value_view(), scale, mask)
    }

    pub fn compressed_prefix_bytes(&self) -> usize {
        self.quantized_prefix.as_ref().map(TurboQuantKvPrefix::compressed_bytes).unwrap_or(0)
    }

    pub fn dense_prefix_equivalent_bytes(&self) -> usize {
        self.quantized_prefix
            .as_ref()
            .map(TurboQuantKvPrefix::dense_equivalent_bytes)
            .unwrap_or(0)
    }

    pub fn dense_tail_bytes(&self) -> usize {
        let shape = self.key.size();
        (shape[0] * shape[1] * self.dense_len * shape[3]) as usize * dtype_bytes(self.key.kind()) * 2
    }

    pub fn total_cache_bytes(&self) -> usize {
        self.compressed_prefix_bytes() + self.dense_tail_bytes()
    }

    #[cfg(test)]
    pub(crate) fn quantized_prefix_len(&self) -> i64 { self.prefix_len() }
    #[cfg(test)]
    pub(crate) fn dense_capacity(&self) -> i64 { self.dense_capacity }
    #[cfg(test)]
    pub(crate) fn key_view(&self) -> Tensor { self.merge_prefix_and_dense(false) }
    #[cfg(test)]
    pub(crate) fn value_view(&self) -> Tensor { self.merge_prefix_and_dense(true) }

    pub fn ensure_capacity(&mut self, required: i64) {
        if required <= self.dense_capacity {
            return;
        }

        let new_capacity = self.dense_capacity.max(1).max(required).max(self.dense_capacity * 2);
        let key_shape = self.key.size();
        let key = Tensor::zeros(
            &[key_shape[0], key_shape[1], new_capacity, key_shape[3]],
            self.key.kind(),
            self.key.device(),
        );
        let mut key = key;
        copy_prefix_into_reserved_buffer(&mut key, &self.key.narrow(2, 0, self.dense_len), self.dense_len);
        self.key = key;

        let value_shape = self.value.size();
        let value = Tensor::zeros(
            &[value_shape[0], value_shape[1], new_capacity, value_shape[3]],
            self.value.kind(),
            self.value.device(),
        );
        let mut value = value;
        copy_prefix_into_reserved_buffer(
            &mut value,
            &self.value.narrow(2, 0, self.dense_len),
            self.dense_len,
        );
        self.value = value;
        self.dense_capacity = new_capacity;
    }

    pub fn append(&mut self, key: &Tensor, value: &Tensor) {
        let appended_tokens = key.size()[2];
        let required = self.dense_len + appended_tokens;
        self.ensure_capacity(required);
        copy_append_into_reserved_buffer(&mut self.key, key, self.dense_len);
        copy_append_into_reserved_buffer(&mut self.value, value, self.dense_len);
        self.dense_len = required;
        if self.mode == KvCacheMode::ExperimentalTurboQuant
            && (appended_tokens > 1 || self.quantized_prefix.is_none())
        {
            return;
        }
        if self.mode == KvCacheMode::ExperimentalTurboQuant
            && self.dense_len <= APPEND_OFFLOAD_TRIGGER_TOKENS
        {
            return;
        }
        self.offload_quantized_prefix();
    }

    pub fn finalize_prefill_offload(&mut self) { self.offload_quantized_prefix(); }

    fn deep_copy_with_capacity(&self, required_capacity: i64) -> Self {
        self.copy_dense_window_with_prefix(
            0,
            self.dense_len,
            self.dense_capacity.max(required_capacity).max(self.dense_len),
            self.quantized_prefix.as_ref().map(TurboQuantKvPrefix::storage_clone),
        )
    }

    fn deep_copy_with_dense_reserve(&self, additional_dense_tokens: i64) -> Self { self.deep_copy_with_capacity(self.dense_len + additional_dense_tokens) }

    fn into_with_dense_reserve(mut self, additional_dense_tokens: i64) -> Self {
        self.ensure_capacity(self.dense_len + additional_dense_tokens);
        self
    }

    fn deep_copy_generation_ready(&self, additional_dense_tokens: i64) -> Self {
        // Deferred quantization: don't quantize at handoff time.
        // Incremental offload during generation will handle compression.
        self.deep_copy_with_dense_reserve(additional_dense_tokens)
    }

    fn into_generation_ready(self, additional_dense_tokens: i64) -> Self {
        // Deferred quantization: don't quantize at handoff time.
        // Incremental offload during generation will handle compression.
        self.into_with_dense_reserve(additional_dense_tokens)
    }

    fn prefix_len(&self) -> i64 {
        self.quantized_prefix.as_ref().map(TurboQuantKvPrefix::key_len).unwrap_or(0)
    }

    #[cfg(test)]
    fn merge_prefix_and_dense(&self, use_value: bool) -> Tensor {
        let dense = if use_value { self.value.narrow(2, 0, self.dense_len) } else { self.key.narrow(2, 0, self.dense_len) };
        if let Some(prefix) = &self.quantized_prefix {
            let prefix = if use_value { prefix.value_view() } else { prefix.key_view() };
            if self.dense_len == 0 { prefix } else { Tensor::cat(&[prefix, dense], 2) }
        } else {
            dense
        }
    }

    fn copy_dense_window_with_prefix(&self, start: i64, len: i64, capacity: i64, quantized_prefix: Option<TurboQuantKvPrefix>) -> Self {
        if len == 0 { return self.empty_dense_window_with_prefix(capacity, quantized_prefix); }
        let capacity = capacity.max(len);
        let key_shape = self.key.size();
        let value_shape = self.value.size();
        let key = Tensor::zeros(
            &[key_shape[0], key_shape[1], capacity, key_shape[3]],
            self.key.kind(),
            self.key.device(),
        );
        let value = Tensor::zeros(
            &[value_shape[0], value_shape[1], capacity, value_shape[3]],
            self.value.kind(),
            self.value.device(),
        );
        let mut key = key;
        if len > 0 {
            copy_prefix_into_reserved_buffer(&mut key, &self.key.narrow(2, start, len), len)
        }
        let mut value = value;
        if len > 0 {
            copy_prefix_into_reserved_buffer(&mut value, &self.value.narrow(2, start, len), len)
        }
        Self {
            mode: self.mode,
            quantized_prefix,
            key,
            value,
            dense_len: len,
            dense_capacity: capacity,
        }
    }

    fn offload_quantized_prefix(&mut self) {
        if self.mode != KvCacheMode::ExperimentalTurboQuant {
            return;
        }

        let tokens_to_move = quantized_tokens_to_move(self.dense_len);
        let remaining_len = self.dense_len - tokens_to_move;
        if tokens_to_move <= 0 {
            return;
        }

        let mut quantized_prefix = self.quantized_prefix.take();
        let prefix_key = self.key.narrow(2, 0, tokens_to_move);
        let prefix_value = self.value.narrow(2, 0, tokens_to_move);
        if let Some(prefix) = quantized_prefix.as_mut() {
            prefix.append_dense_tokens(&prefix_key, &prefix_value);
        } else {
            quantized_prefix =
                Some(TurboQuantKvPrefix::quantize_appending(None, &prefix_key, &prefix_value));
        }

        let new_capacity = remaining_len;
        *self = self.copy_dense_window_with_prefix(
            tokens_to_move,
            remaining_len,
            new_capacity,
            quantized_prefix,
        );
    }

    fn empty_dense_window_with_prefix(&self, capacity: i64, quantized_prefix: Option<TurboQuantKvPrefix>) -> Self {
        let key_shape = self.key.size(); let value_shape = self.value.size();
        Self { mode: self.mode, quantized_prefix, key: Tensor::zeros(&[key_shape[0], key_shape[1], capacity, key_shape[3]], self.key.kind(), self.key.device()), value: Tensor::zeros(&[value_shape[0], value_shape[1], capacity, value_shape[3]], self.value.kind(), self.value.device()), dense_len: 0, dense_capacity: capacity }
    }
}

impl KvCache {
    pub fn new(num_layers: usize, mode: KvCacheMode) -> Self {
        Self { mode, layers: (0..num_layers).map(|_| None).collect() }
    }

    pub fn get_mut(&mut self, layer: usize) -> &mut Option<KvCacheEntry> { &mut self.layers[layer] }
    pub fn mode(&self) -> KvCacheMode { self.mode }
    pub fn seq_len(&self) -> i64 { self.layers[0].as_ref().map(KvCacheEntry::len).unwrap_or(0) }

    pub fn total_cache_bytes(&self) -> usize { self.layers.iter().filter_map(|entry| entry.as_ref()).map(KvCacheEntry::total_cache_bytes).sum() }

    pub fn dense_equivalent_cache_bytes(&self) -> usize {
        self.layers
            .iter()
            .filter_map(|entry| entry.as_ref())
            .map(|entry| entry.dense_prefix_equivalent_bytes() + entry.dense_tail_bytes())
            .sum()
    }

    pub fn deep_copy_with_reserve(&self, additional_tokens: usize) -> Self {
        let additional_tokens = additional_tokens as i64;
        Self { mode: self.mode, layers: self.layers.iter().map(|entry| entry.as_ref().map(|entry| entry.deep_copy_with_dense_reserve(additional_tokens))).collect() }
    }

    pub fn into_with_reserve(self, additional_tokens: usize) -> Self {
        let additional_tokens = additional_tokens as i64;
        Self {
            mode: self.mode,
            layers: self
                .layers
                .into_iter()
                .map(|entry| entry.map(|entry| entry.into_with_dense_reserve(additional_tokens)))
                .collect(),
        }
    }

    pub fn deep_copy_generation_ready(&self, additional_tokens: usize) -> Self {
        let additional_tokens = additional_tokens as i64;
        Self { mode: self.mode, layers: self.layers.iter().map(|entry| entry.as_ref().map(|entry| entry.deep_copy_generation_ready(additional_tokens))).collect() }
    }

    pub fn into_generation_ready(self, additional_tokens: usize) -> Self {
        let additional_tokens = additional_tokens as i64;
        Self {
            mode: self.mode,
            layers: self
                .layers
                .into_iter()
                .map(|entry| entry.map(|entry| entry.into_generation_ready(additional_tokens)))
                .collect(),
        }
    }

    pub fn finalize_prefill_offload(&mut self) {
        if self.mode != KvCacheMode::ExperimentalTurboQuant {
            return;
        }
        for entry in &mut self.layers {
            if let Some(entry) = entry.as_mut() {
                entry.finalize_prefill_offload();
            }
        }
    }
}

#[derive(Clone)]
pub struct DecoderState {
    pub(crate) kv_cache: KvCache,
    next_position: usize,
    pub(crate) last_logits: Option<Tensor>,
}

impl DecoderState {
    pub fn new(num_layers: usize, kv_cache_mode: KvCacheMode) -> Self {
        Self { kv_cache: KvCache::new(num_layers, kv_cache_mode), next_position: 0, last_logits: None }
    }

    pub fn next_position(&self) -> usize { self.next_position }
    pub fn cached_seq_len(&self) -> i64 { self.kv_cache.seq_len() }
    pub fn total_cache_bytes(&self) -> usize { self.kv_cache.total_cache_bytes() }
    pub fn dense_equivalent_cache_bytes(&self) -> usize { self.kv_cache.dense_equivalent_cache_bytes() }

    pub fn deep_copy_with_reserve(&self, additional_tokens: usize) -> Self {
        Self {
            kv_cache: self.kv_cache.deep_copy_with_reserve(additional_tokens),
            next_position: self.next_position,
            last_logits: self.last_logits.as_ref().map(Tensor::copy),
        }
    }

    pub fn into_with_reserve(mut self, additional_tokens: usize) -> Self {
        self.kv_cache = self.kv_cache.into_with_reserve(additional_tokens);
        self
    }

    pub fn deep_copy_generation_ready(&self, additional_tokens: usize) -> Self {
        Self {
            kv_cache: self.kv_cache.deep_copy_generation_ready(additional_tokens),
            next_position: self.next_position,
            last_logits: self.last_logits.as_ref().map(Tensor::copy),
        }
    }

    pub fn into_generation_ready(mut self, additional_tokens: usize) -> Self {
        self.kv_cache = self.kv_cache.into_generation_ready(additional_tokens);
        self
    }

    pub(crate) fn advance_by(&mut self, tokens: usize) { self.next_position += tokens; }
}

pub fn create_causal_mask(seq_len: i64, past_len: i64, device: Device) -> Tensor {
    Tensor::full(&[seq_len, past_len + seq_len], f64::NEG_INFINITY, DType::Float32, device)
        .triu(past_len + 1)
        .unsqueeze(0)
        .unsqueeze(0)
}

const RECENT_DENSE_TAIL_TOKENS: i64 = 32;
const APPEND_OFFLOAD_TRIGGER_TOKENS: i64 = RECENT_DENSE_TAIL_TOKENS * 2;

fn quantized_tokens_to_move(dense_len: i64) -> i64 {
    let dense_len = dense_len.max(0);
    dense_len.saturating_sub(dense_len.min(RECENT_DENSE_TAIL_TOKENS))
}

fn copy_prefix_into_reserved_buffer(buffer: &mut Tensor, src: &Tensor, len: i64) {
    #[cfg(feature = "tch-backend")]
    {
        let mut dst = buffer.narrow(2, 0, len);
        dst.inner.copy_(&src.inner);
    }
    #[cfg(feature = "mlx")]
    {
        *buffer = buffer.slice_scatter(src, 2, 0, len, 1)
    }
}

fn copy_append_into_reserved_buffer(buffer: &mut Tensor, src: &Tensor, start: i64) {
    #[cfg(feature = "tch-backend")]
    {
        let mut dst = buffer.narrow(2, start, src.size()[2]);
        dst.inner.copy_(&src.inner);
    }
    #[cfg(feature = "mlx")]
    {
        *buffer = buffer.slice_scatter(src, 2, start, start + src.size()[2], 1)
    }
}

fn dense_tail_scores(q: &Tensor, key: &Tensor, scale: f64) -> Tensor {
    let q_heads = q.size()[1];
    let kv_heads = key.size()[1];
    if q_heads == kv_heads {
        return q.matmul(&key.transpose(-2, -1)) * scale;
    }

    let n_rep = q_heads / kv_heads;
    let mut grouped_scores = Vec::with_capacity(kv_heads as usize);
    for kv_head_idx in 0..kv_heads {
        let q_group = q.narrow(1, kv_head_idx * n_rep, n_rep);
        let key_group = key.narrow(1, kv_head_idx, 1).transpose(-2, -1);
        grouped_scores.push(q_group.matmul(&key_group) * scale);
    }
    Tensor::cat(&grouped_scores, 1)
}

fn dense_tail_weighted_value_sum(weights: &Tensor, value: &Tensor) -> Tensor {
    let q_heads = weights.size()[1];
    let kv_heads = value.size()[1];
    if q_heads == kv_heads {
        return weights.matmul(value);
    }

    let n_rep = q_heads / kv_heads;
    let mut grouped_outputs = Vec::with_capacity(kv_heads as usize);
    for kv_head_idx in 0..kv_heads {
        let weight_group = weights.narrow(1, kv_head_idx * n_rep, n_rep);
        let value_group = value.narrow(1, kv_head_idx, 1);
        let value_group = if value_group.kind() == weights.kind() {
            value_group
        } else {
            value_group.to_dtype(weights.kind())
        };
        grouped_outputs.push(weight_group.matmul(&value_group));
    }
    Tensor::cat(&grouped_outputs, 1)
}

fn dtype_bytes(dtype: DType) -> usize {
    match dtype {
        DType::Float32 => 4,
        DType::Float16 | DType::BFloat16 => 2,
        DType::Int8 | DType::Bool => 1,
        DType::Int32 => 4,
        DType::Int64 => 8,
    }
}
