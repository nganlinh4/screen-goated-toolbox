use crate::tensor::{DType, Tensor};

pub(super) const RECENT_DENSE_TAIL_TOKENS: i64 = 32;
pub(super) const APPEND_OFFLOAD_TRIGGER_TOKENS: i64 = RECENT_DENSE_TAIL_TOKENS * 2;

pub(super) fn quantized_tokens_to_move(
    dense_len: i64,
    retained_dense_tail_tokens: i64,
) -> i64 {
    let dense_len = dense_len.max(0);
    let retained_dense_tail_tokens = retained_dense_tail_tokens.max(0);
    dense_len.saturating_sub(dense_len.min(retained_dense_tail_tokens))
}

pub(super) fn copy_prefix_into_reserved_buffer(buffer: &mut Tensor, src: &Tensor, len: i64) {
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

pub(super) fn copy_append_into_reserved_buffer(buffer: &mut Tensor, src: &Tensor, start: i64) {
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

pub(super) fn dense_tail_scores(q: &Tensor, key: &Tensor, scale: f64) -> Tensor {
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

pub(super) fn dense_tail_weighted_value_sum(weights: &Tensor, value: &Tensor) -> Tensor {
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

pub(super) fn dtype_bytes(dtype: DType) -> usize {
    match dtype {
        DType::Float32 => 4,
        DType::Float16 | DType::BFloat16 => 2,
        DType::Int8 | DType::Bool => 1,
        DType::Int32 => 4,
        DType::Int64 => 8,
    }
}
