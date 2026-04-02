use crate::tensor::{DType, Device, Tensor};
use crate::text_decoder::{KvCacheEntry, KvCacheMode};
use crate::weights::{get_weight, get_weight_opt};
use anyhow::Result;
use std::collections::HashMap;

fn cast_for_compute(tensor: &Tensor, dtype: DType) -> Tensor {
    if tensor.kind() == dtype {
        tensor.shallow_clone()
    } else {
        tensor.to_dtype(dtype)
    }
}

// ============================================================================
// LayerNorm (with bias, used in audio encoder)
// ============================================================================

pub struct LayerNorm {
    pub weight: Tensor,
    pub bias: Tensor,
    pub eps: f64,
}

impl LayerNorm {
    pub fn load(weights: &HashMap<String, Tensor>, prefix: &str, eps: f64) -> Result<Self> {
        Ok(Self {
            weight: get_weight(weights, prefix, "weight")?,
            bias: get_weight(weights, prefix, "bias")?,
            eps,
        })
    }

    pub fn forward(&self, x: &Tensor) -> Tensor {
        let ndim = x.dim();
        let dtype = x.kind();
        let weight = cast_for_compute(&self.weight, dtype);
        let bias = cast_for_compute(&self.bias, dtype);
        x.layer_norm(&[x.size()[ndim - 1]], Some(&weight), Some(&bias), self.eps)
    }
}

// ============================================================================
// RMSNorm (used in text decoder)
// ============================================================================

pub struct RmsNorm {
    pub weight: Tensor,
    pub eps: f64,
}

impl RmsNorm {
    pub fn load(weights: &HashMap<String, Tensor>, prefix: &str, eps: f64) -> Result<Self> {
        Ok(Self {
            weight: get_weight(weights, prefix, "weight")?,
            eps,
        })
    }

    pub fn forward(&self, x: &Tensor) -> Tensor {
        x.rms_norm(&self.weight, self.eps)
    }
}

// ============================================================================
// Linear layer
// ============================================================================

pub struct Linear {
    pub weight_t: Tensor, // Pre-transposed weight for matmul
    pub bias: Option<Tensor>,
}

impl Linear {
    pub fn load(weights: &HashMap<String, Tensor>, prefix: &str) -> Result<Self> {
        let weight = get_weight(weights, prefix, "weight")?;
        Ok(Self {
            weight_t: weight.tr().contiguous(), // Pre-transpose at load time for faster matmul
            bias: get_weight_opt(weights, prefix, "bias"),
        })
    }

    pub fn forward(&self, x: &Tensor) -> Tensor {
        let compute_dtype = self.weight_t.kind();
        let x = cast_for_compute(x, compute_dtype);
        let out = x.matmul(&self.weight_t);
        match &self.bias {
            Some(b) => out + cast_for_compute(b, compute_dtype),
            None => out,
        }
    }
}

// ============================================================================
// Conv2d layer
// ============================================================================

pub struct Conv2d {
    pub weight: Tensor,
    pub bias: Option<Tensor>,
    pub stride: [i64; 2],
    pub padding: [i64; 2],
}

impl Conv2d {
    pub fn load(
        weights: &HashMap<String, Tensor>,
        prefix: &str,
        stride: [i64; 2],
        padding: [i64; 2],
    ) -> Result<Self> {
        Ok(Self {
            weight: get_weight(weights, prefix, "weight")?,
            bias: get_weight_opt(weights, prefix, "bias"),
            stride,
            padding,
        })
    }

    pub fn forward(&self, x: &Tensor) -> Tensor {
        let compute_dtype = self.weight.kind();
        let x = cast_for_compute(x, compute_dtype);
        let bias = self
            .bias
            .as_ref()
            .map(|bias| cast_for_compute(bias, compute_dtype));
        x.conv2d(
            &self.weight,
            bias.as_ref(),
            &self.stride,
            &self.padding,
            &[1, 1], // dilation
            1,       // groups
        )
    }
}

// ============================================================================
// Audio encoder self-attention (bidirectional, with bias)
// ============================================================================

pub struct AudioAttention {
    pub q_proj: Linear,
    pub k_proj: Linear,
    pub v_proj: Linear,
    pub out_proj: Linear,
    pub num_heads: usize,
    pub head_dim: usize,
}

impl AudioAttention {
    pub fn load(
        weights: &HashMap<String, Tensor>,
        prefix: &str,
        num_heads: usize,
        d_model: usize,
    ) -> Result<Self> {
        let head_dim = d_model / num_heads;
        Ok(Self {
            q_proj: Linear::load(weights, &format!("{}.q_proj", prefix))?,
            k_proj: Linear::load(weights, &format!("{}.k_proj", prefix))?,
            v_proj: Linear::load(weights, &format!("{}.v_proj", prefix))?,
            out_proj: Linear::load(weights, &format!("{}.out_proj", prefix))?,
            num_heads,
            head_dim,
        })
    }

    pub fn forward(&self, x: &Tensor, mask: Option<&Tensor>) -> Tensor {
        let (bsz, seq_len, _) = x.size3();
        let nh = self.num_heads as i64;
        let hd = self.head_dim as i64;

        let q = self
            .q_proj
            .forward(x)
            .reshape(&[bsz, seq_len, nh, hd])
            .permute(&[0, 2, 1, 3]);
        let k = self
            .k_proj
            .forward(x)
            .reshape(&[bsz, seq_len, nh, hd])
            .permute(&[0, 2, 1, 3]);
        let v = self
            .v_proj
            .forward(x)
            .reshape(&[bsz, seq_len, nh, hd])
            .permute(&[0, 2, 1, 3]);

        let scale = 1.0 / (hd as f64).sqrt();
        let out = Tensor::scaled_dot_product_attention(&q, &k, &v, scale, mask);
        let out = out.permute(&[0, 2, 1, 3]).reshape(&[bsz, seq_len, nh * hd]);
        self.out_proj.forward(&out)
    }
}

// ============================================================================
// Audio encoder FFN
// ============================================================================

pub struct AudioFfn {
    pub fc1: Linear,
    pub fc2: Linear,
}

impl AudioFfn {
    pub fn load(weights: &HashMap<String, Tensor>, prefix: &str) -> Result<Self> {
        Ok(Self {
            fc1: Linear::load(weights, &format!("{}.fc1", prefix))?,
            fc2: Linear::load(weights, &format!("{}.fc2", prefix))?,
        })
    }

    pub fn forward(&self, x: &Tensor) -> Tensor {
        let x = self.fc1.forward(x).gelu();
        self.fc2.forward(&x)
    }
}

// ============================================================================
// Audio encoder layer
// ============================================================================

pub struct AudioEncoderLayer {
    pub self_attn_layer_norm: LayerNorm,
    pub self_attn: AudioAttention,
    pub final_layer_norm: LayerNorm,
    pub ffn: AudioFfn,
}

impl AudioEncoderLayer {
    pub fn load(
        weights: &HashMap<String, Tensor>,
        prefix: &str,
        num_heads: usize,
        d_model: usize,
    ) -> Result<Self> {
        Ok(Self {
            self_attn_layer_norm: LayerNorm::load(
                weights,
                &format!("{}.self_attn_layer_norm", prefix),
                1e-5,
            )?,
            self_attn: AudioAttention::load(
                weights,
                &format!("{}.self_attn", prefix),
                num_heads,
                d_model,
            )?,
            final_layer_norm: LayerNorm::load(
                weights,
                &format!("{}.final_layer_norm", prefix),
                1e-5,
            )?,
            ffn: AudioFfn::load(weights, prefix)?,
        })
    }

    pub fn forward(&self, x: &Tensor, mask: Option<&Tensor>) -> Tensor {
        // Pre-norm + self-attention + residual
        let residual = x;
        let x = self.self_attn_layer_norm.forward(x);
        let x = self.self_attn.forward(&x, mask);
        let x = &x + residual;

        // Pre-norm + FFN + residual
        let residual = x.shallow_clone();
        let h = self.final_layer_norm.forward(&x);
        let h = self.ffn.forward(&h);
        h + residual
    }
}

// ============================================================================
// Text decoder attention (GQA with QK-norm and MRoPE)
// ============================================================================

pub struct TextAttention {
    /// Fused QKV weight: [hidden, (q_dim + kv_dim + kv_dim)] transposed
    qkv_weight_t: Tensor,
    q_dim: i64,
    kv_dim: i64,
    pub o_proj: Linear,
    pub q_norm: RmsNorm,
    pub k_norm: RmsNorm,
    pub num_q_heads: usize,
    pub num_kv_heads: usize,
    pub head_dim: usize,
}

struct AttentionProjection {
    q: Tensor,
    k: Tensor,
    v: Tensor,
}

impl TextAttention {
    pub fn load(
        weights: &HashMap<String, Tensor>,
        prefix: &str,
        num_q_heads: usize,
        num_kv_heads: usize,
        head_dim: usize,
        rms_norm_eps: f64,
    ) -> Result<Self> {
        let q_proj = Linear::load(weights, &format!("{}.q_proj", prefix))?;
        let k_proj = Linear::load(weights, &format!("{}.k_proj", prefix))?;
        let v_proj = Linear::load(weights, &format!("{}.v_proj", prefix))?;
        let q_dim = q_proj.weight_t.size()[1];
        let kv_dim = k_proj.weight_t.size()[1];
        let qkv_weight_t = Tensor::cat(&[q_proj.weight_t, k_proj.weight_t, v_proj.weight_t], 1);
        Ok(Self {
            qkv_weight_t,
            q_dim,
            kv_dim,
            o_proj: Linear::load(weights, &format!("{}.o_proj", prefix))?,
            q_norm: RmsNorm::load(weights, &format!("{}.q_norm", prefix), rms_norm_eps)?,
            k_norm: RmsNorm::load(weights, &format!("{}.k_norm", prefix), rms_norm_eps)?,
            num_q_heads,
            num_kv_heads,
            head_dim,
        })
    }

    /// Forward pass with KV cache support.
    pub fn forward(
        &self,
        x: &Tensor,
        cos: &Tensor,
        sin: &Tensor,
        kv_cache: &mut Option<KvCacheEntry>,
        kv_cache_mode: KvCacheMode,
        mask: Option<&Tensor>,
    ) -> Tensor {
        let projection = self.project_qkv(x, cos, sin);
        let scale = 1.0 / (self.head_dim as f64).sqrt();
        self.merge_kv_cache(projection, kv_cache, kv_cache_mode, scale, mask)
    }

    fn apply_head_norm(&self, x: &Tensor, norm: &RmsNorm) -> Tensor {
        norm.forward(x)
    }

    fn project_qkv(&self, x: &Tensor, cos: &Tensor, sin: &Tensor) -> AttentionProjection {
        let (bsz, seq_len, _) = x.size3();
        let nqh = self.num_q_heads as i64;
        let nkvh = self.num_kv_heads as i64;
        let hd = self.head_dim as i64;

        // Single fused QKV matmul instead of 3 separate projections
        let compute_dtype = self.qkv_weight_t.kind();
        let x_cast = cast_for_compute(x, compute_dtype);
        let qkv = x_cast.matmul(&self.qkv_weight_t);

        let q = qkv.narrow(-1, 0, self.q_dim)
            .reshape(&[bsz, seq_len, nqh, hd])
            .transpose(1, 2);
        let k = qkv.narrow(-1, self.q_dim, self.kv_dim)
            .reshape(&[bsz, seq_len, nkvh, hd])
            .transpose(1, 2);
        let v = qkv.narrow(-1, self.q_dim + self.kv_dim, self.kv_dim)
            .reshape(&[bsz, seq_len, nkvh, hd])
            .transpose(1, 2);

        let q = self.apply_head_norm(&q, &self.q_norm).apply_rope(cos, sin);
        let k = self.apply_head_norm(&k, &self.k_norm).apply_rope(cos, sin);

        AttentionProjection { q, k, v }
    }

    fn merge_kv_cache(
        &self,
        projection: AttentionProjection,
        kv_cache: &mut Option<KvCacheEntry>,
        kv_cache_mode: KvCacheMode,
        scale: f64,
        mask: Option<&Tensor>,
    ) -> Tensor {
        let AttentionProjection { q, k, v } = projection;
        let output = match kv_cache {
            Some(cache) => {
                cache.append(&k, &v);
                cache
                    .attend_with_quantized_prefix(&q, scale, mask)
                    .expect("text attention cache must produce attention output after merge")
            }
            None => {
                let cache = kv_cache.get_or_insert_with(|| KvCacheEntry::from_tokens(k, v, kv_cache_mode));
                cache
                    .attend_with_quantized_prefix(&q, scale, mask)
                    .expect("text attention cache must produce attention output after merge")
            }
        };
        let (bsz, seq_len, _) = (q.size()[0], q.size()[2], q.size()[1] * q.size()[3]);
        self.o_proj.forward(&output.transpose(1, 2).reshape(&[
            bsz,
            seq_len,
            self.num_q_heads as i64 * self.head_dim as i64,
        ]))
    }
}

// ============================================================================
// Text decoder MLP (SwiGLU)
// ============================================================================

pub struct TextMlp {
    /// Fused gate+up weight: [hidden, intermediate*2] (transposed for matmul)
    gate_up_weight_t: Tensor,
    gate_up_bias: Option<(Tensor, Tensor)>,
    intermediate_size: i64,
    pub down_proj: Linear,
}

impl TextMlp {
    pub fn load(weights: &HashMap<String, Tensor>, prefix: &str) -> Result<Self> {
        let gate_proj = Linear::load(weights, &format!("{}.gate_proj", prefix))?;
        let up_proj = Linear::load(weights, &format!("{}.up_proj", prefix))?;
        let intermediate_size = gate_proj.weight_t.size()[1];
        // Fuse gate_proj and up_proj weights: concat along output dim
        // gate_proj.weight_t: [hidden, intermediate], up_proj.weight_t: [hidden, intermediate]
        // fused: [hidden, intermediate*2]
        let gate_up_weight_t = Tensor::cat(&[gate_proj.weight_t, up_proj.weight_t], 1);
        let gate_up_bias = match (gate_proj.bias, up_proj.bias) {
            (Some(gb), Some(ub)) => Some((gb, ub)),
            _ => None,
        };
        Ok(Self {
            gate_up_weight_t,
            gate_up_bias,
            intermediate_size,
            down_proj: Linear::load(weights, &format!("{}.down_proj", prefix))?,
        })
    }

    pub fn forward(&self, x: &Tensor) -> Tensor {
        let compute_dtype = self.gate_up_weight_t.kind();
        let x = cast_for_compute(x, compute_dtype);
        let fused = x.matmul(&self.gate_up_weight_t);
        if let Some((ref gb, ref ub)) = self.gate_up_bias {
            let gate_part = fused.narrow(-1, 0, self.intermediate_size) + cast_for_compute(gb, compute_dtype);
            let up_part = fused.narrow(-1, self.intermediate_size, self.intermediate_size) + cast_for_compute(ub, compute_dtype);
            return self.down_proj.forward(&(gate_part.silu() * up_part));
        }
        let gate = fused.narrow(-1, 0, self.intermediate_size).silu();
        let up = fused.narrow(-1, self.intermediate_size, self.intermediate_size);
        self.down_proj.forward(&(gate * up))
    }
}

// ============================================================================
// Text decoder layer
// ============================================================================

pub struct TextDecoderLayer {
    pub input_layernorm: RmsNorm,
    pub self_attn: TextAttention,
    pub post_attention_layernorm: RmsNorm,
    pub mlp: TextMlp,
}

impl TextDecoderLayer {
    pub fn load(
        weights: &HashMap<String, Tensor>,
        prefix: &str,
        num_q_heads: usize,
        num_kv_heads: usize,
        head_dim: usize,
        rms_norm_eps: f64,
    ) -> Result<Self> {
        Ok(Self {
            input_layernorm: RmsNorm::load(
                weights,
                &format!("{}.input_layernorm", prefix),
                rms_norm_eps,
            )?,
            self_attn: TextAttention::load(
                weights,
                &format!("{}.self_attn", prefix),
                num_q_heads,
                num_kv_heads,
                head_dim,
                rms_norm_eps,
            )?,
            post_attention_layernorm: RmsNorm::load(
                weights,
                &format!("{}.post_attention_layernorm", prefix),
                rms_norm_eps,
            )?,
            mlp: TextMlp::load(weights, &format!("{}.mlp", prefix))?,
        })
    }

    pub fn forward(
        &self,
        x: &Tensor,
        cos: &Tensor,
        sin: &Tensor,
        kv_cache: &mut Option<KvCacheEntry>,
        kv_cache_mode: KvCacheMode,
        mask: Option<&Tensor>,
    ) -> Tensor {
        // Pre-norm + self-attention + residual
        let residual = x;
        let h = self.input_layernorm.forward(x);
        let h = self
            .self_attn
            .forward(&h, cos, sin, kv_cache, kv_cache_mode, mask);
        let x = &h + residual;

        // Pre-norm + MLP + residual
        let residual = x.shallow_clone();
        let h = self.post_attention_layernorm.forward(&x);
        let h = self.mlp.forward(&h);
        h + residual
    }
}

// ============================================================================
// MRoPE (Multimodal Rotary Position Embedding) utilities
// ============================================================================

/// Compute MRoPE cos/sin tensors for the given 3D position IDs.
pub fn compute_mrope_cos_sin(
    position_ids: &[Vec<i64>; 3],
    head_dim: usize,
    rope_theta: f64,
    mrope_section: &[usize],
    interleaved: bool,
    device: Device,
) -> (Tensor, Tensor) {
    let half_dim = head_dim / 2;
    let seq_len = position_ids[0].len();

    // Compute inverse frequencies
    let inv_freq: Vec<f64> = (0..half_dim)
        .map(|i| 1.0 / rope_theta.powf(2.0 * i as f64 / head_dim as f64))
        .collect();

    // Build dim_map: for each freq index (0..half_dim), which MRoPE dim to use (0,1,2)
    let dim_map = if interleaved {
        build_interleaved_dim_map(mrope_section, half_dim)
    } else {
        build_contiguous_dim_map(mrope_section, half_dim)
    };

    // Compute cos/sin for each position
    let mut cos_vals = vec![0.0f32; seq_len * head_dim];
    let mut sin_vals = vec![0.0f32; seq_len * head_dim];

    for t in 0..seq_len {
        for j in 0..half_dim {
            let dim = dim_map[j];
            let pos = position_ids[dim][t] as f64;
            let angle = pos * inv_freq[j];
            let c = angle.cos() as f32;
            let s = angle.sin() as f32;
            // First half
            cos_vals[t * head_dim + j] = c;
            sin_vals[t * head_dim + j] = s;
            // Second half (identical, standard RoPE doubling)
            cos_vals[t * head_dim + j + half_dim] = c;
            sin_vals[t * head_dim + j + half_dim] = s;
        }
    }

    let cos = Tensor::from_slice_f32(&cos_vals)
        .reshape(&[seq_len as i64, head_dim as i64])
        .to_device(device);
    let sin = Tensor::from_slice_f32(&sin_vals)
        .reshape(&[seq_len as i64, head_dim as i64])
        .to_device(device);

    (cos, sin)
}

fn build_contiguous_dim_map(sections: &[usize], total: usize) -> Vec<usize> {
    let mut map = Vec::with_capacity(total);
    for (dim, &size) in sections.iter().enumerate() {
        for _ in 0..size {
            if map.len() >= total {
                break;
            }
            map.push(dim);
        }
    }
    while map.len() < total {
        map.push(sections.len() - 1);
    }
    map
}

fn build_interleaved_dim_map(sections: &[usize], total: usize) -> Vec<usize> {
    let n_dims = sections.len();
    let mut map = Vec::with_capacity(total);
    let mut counts = vec![0usize; n_dims];

    while map.len() < total {
        let prev_len = map.len();
        for dim in 0..n_dims {
            if map.len() >= total {
                break;
            }
            if counts[dim] < sections[dim] {
                map.push(dim);
                counts[dim] += 1;
            }
        }
        if map.len() == prev_len {
            break;
        }
    }

    map
}
