use crate::tensor::{DType, Tensor};

const TURBOQUANT_KEY_BITS: usize = 4;
const TURBOQUANT_VALUE_BITS: usize = 4;

#[derive(Clone)]
pub(crate) struct TurboQuantKvPrefix {
    key: TurboQuantCompressedKeyTensor,
    value: TurboQuantCompressedValueTensor,
}

#[derive(Clone)]
struct TurboQuantCompressedKeyTensor {
    compressed_bytes: usize,
    materialized_dense: Tensor,
    batch: i64,
    heads: i64,
    head_dim: i64,
    len: i64,
    dtype: DType,
}

#[derive(Clone)]
struct TurboQuantCompressedValueTensor {
    compressed_bytes: usize,
    materialized_dense: Tensor,
    batch: i64,
    heads: i64,
    head_dim: i64,
    len: i64,
    dtype: DType,
}

impl TurboQuantKvPrefix {
    pub(crate) fn quantize_appending(existing: Option<Self>, key: &Tensor, value: &Tensor) -> Self {
        match existing {
            Some(mut prefix) => {
                prefix.append_dense_tokens(key, value);
                prefix
            }
            None => Self {
                key: TurboQuantCompressedKeyTensor::from_tensor(key),
                value: TurboQuantCompressedValueTensor::from_tensor(value),
            },
        }
    }

    pub(crate) fn append_dense_tokens(&mut self, key: &Tensor, value: &Tensor) {
        self.key.append_dense_tokens(key);
        self.value.append_dense_tokens(value);
    }

    pub(crate) fn key_len(&self) -> i64 {
        self.key.len
    }

    pub(crate) fn key_tensor(&self) -> Tensor {
        self.key.materialize()
    }

    #[cfg(test)]
    pub(crate) fn key_view(&self) -> Tensor {
        self.key_tensor()
    }

    pub(crate) fn estimate_key_scores(&self, q: &Tensor, scale: f64) -> Tensor {
        let prefix = self.key.materialize();
        dense_tail_scores(q, &prefix, scale)
    }

    pub(crate) fn value_tensor(&self) -> Tensor {
        self.value.materialize()
    }

    #[cfg(test)]
    pub(crate) fn value_view(&self) -> Tensor {
        self.value_tensor()
    }

    pub(crate) fn weighted_value_sum(&self, weights: &Tensor) -> Tensor {
        let prefix = self.value.materialize();
        dense_tail_weighted_value_sum(weights, &prefix)
    }

    pub(crate) fn storage_clone(&self) -> Self {
        Self {
            key: self.key.storage_clone(),
            value: self.value.storage_clone(),
        }
    }

    pub(crate) fn compressed_bytes(&self) -> usize {
        self.key.compressed_bytes() + self.value.compressed_bytes()
    }

    pub(crate) fn dense_equivalent_bytes(&self) -> usize {
        self.key.dense_equivalent_bytes() + self.value.dense_equivalent_bytes()
    }
}

impl TurboQuantCompressedKeyTensor {
    fn from_tensor(tensor: &Tensor) -> Self {
        let shape = tensor.size();
        Self {
            compressed_bytes: compressed_key_bytes(&shape),
            materialized_dense: tensor.contiguous(),
            batch: shape[0],
            heads: shape[1],
            len: shape[2],
            head_dim: shape[3],
            dtype: tensor.kind(),
        }
    }

    fn append_dense_tokens(&mut self, tensor: &Tensor) {
        let shape = tensor.size();
        debug_assert_eq!(shape[0], self.batch);
        debug_assert_eq!(shape[1], self.heads);
        debug_assert_eq!(shape[3], self.head_dim);
        self.compressed_bytes += compressed_key_bytes(&shape);
        self.materialized_dense = Tensor::cat(
            &[
                self.materialized_dense.shallow_clone(),
                tensor.shallow_clone(),
            ],
            2,
        );
        self.len += shape[2];
    }

    fn materialize(&self) -> Tensor {
        self.materialized_dense.shallow_clone()
    }

    fn storage_clone(&self) -> Self {
        Self {
            compressed_bytes: self.compressed_bytes,
            materialized_dense: self.materialized_dense.clone(),
            batch: self.batch,
            heads: self.heads,
            head_dim: self.head_dim,
            len: self.len,
            dtype: self.dtype,
        }
    }

    fn compressed_bytes(&self) -> usize {
        self.compressed_bytes
    }

    fn dense_equivalent_bytes(&self) -> usize {
        (self.batch * self.heads * self.len * self.head_dim) as usize * dtype_bytes(self.dtype)
    }
}

impl TurboQuantCompressedValueTensor {
    fn from_tensor(tensor: &Tensor) -> Self {
        let shape = tensor.size();
        Self {
            compressed_bytes: compressed_value_bytes(&shape),
            materialized_dense: tensor.contiguous(),
            batch: shape[0],
            heads: shape[1],
            len: shape[2],
            head_dim: shape[3],
            dtype: tensor.kind(),
        }
    }

    fn append_dense_tokens(&mut self, tensor: &Tensor) {
        let shape = tensor.size();
        debug_assert_eq!(shape[0], self.batch);
        debug_assert_eq!(shape[1], self.heads);
        debug_assert_eq!(shape[3], self.head_dim);
        self.compressed_bytes += compressed_value_bytes(&shape);
        self.materialized_dense = Tensor::cat(
            &[
                self.materialized_dense.shallow_clone(),
                tensor.shallow_clone(),
            ],
            2,
        );
        self.len += shape[2];
    }

    fn materialize(&self) -> Tensor {
        self.materialized_dense.shallow_clone()
    }

    fn storage_clone(&self) -> Self {
        Self {
            compressed_bytes: self.compressed_bytes,
            materialized_dense: self.materialized_dense.clone(),
            batch: self.batch,
            heads: self.heads,
            head_dim: self.head_dim,
            len: self.len,
            dtype: self.dtype,
        }
    }

    fn compressed_bytes(&self) -> usize {
        self.compressed_bytes
    }

    fn dense_equivalent_bytes(&self) -> usize {
        (self.batch * self.heads * self.len * self.head_dim) as usize * dtype_bytes(self.dtype)
    }
}

fn compressed_key_bytes(shape: &[i64]) -> usize {
    compressed_tensor_bytes(shape, TURBOQUANT_KEY_BITS, true)
}

fn compressed_value_bytes(shape: &[i64]) -> usize {
    compressed_tensor_bytes(shape, TURBOQUANT_VALUE_BITS, false)
}

fn compressed_tensor_bytes(shape: &[i64], bits_per_value: usize, include_signs: bool) -> usize {
    debug_assert_eq!(shape.len(), 4);
    let batch = shape[0].max(0) as usize;
    let heads = shape[1].max(0) as usize;
    let len = shape[2].max(0) as usize;
    let head_dim = shape[3].max(0) as usize;
    let vector_count = batch * heads * len;
    let packed_indices = (head_dim * bits_per_value).div_ceil(8);
    let packed_signs = if include_signs {
        head_dim.div_ceil(8)
    } else {
        0
    };
    vector_count * (packed_indices + packed_signs + std::mem::size_of::<f32>())
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
