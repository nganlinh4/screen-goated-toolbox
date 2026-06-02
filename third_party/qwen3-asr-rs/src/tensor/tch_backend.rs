// ===== tch backend implementation =====

use super::{DType, Device, StftConfig, Tensor};

#[cfg(feature = "tch-backend")]
impl Tensor {
    pub fn from_tch(t: tch::Tensor) -> Self {
        Tensor { inner: t }
    }

    pub fn as_tch(&self) -> &tch::Tensor {
        &self.inner
    }

    pub fn into_tch(self) -> tch::Tensor {
        self.inner
    }

    // -- Creation --

    pub fn from_slice_f32(data: &[f32]) -> Self {
        Tensor::from_tch(tch::Tensor::from_slice(data))
    }

    pub fn from_slice_i64(data: &[i64]) -> Self {
        Tensor::from_tch(tch::Tensor::from_slice(data))
    }

    pub fn from_slice_i8(data: &[i8]) -> Self {
        Tensor::from_tch(tch::Tensor::from_slice(data))
    }

    pub fn zeros(shape: &[i64], dtype: DType, device: Device) -> Self {
        let opts = (tch::Kind::from(dtype), tch::Device::from(device));
        Tensor::from_tch(tch::Tensor::zeros(shape, opts))
    }

    pub fn ones(shape: &[i64], dtype: DType, device: Device) -> Self {
        let opts = (tch::Kind::from(dtype), tch::Device::from(device));
        Tensor::from_tch(tch::Tensor::ones(shape, opts))
    }

    pub fn full(shape: &[i64], val: f64, dtype: DType, device: Device) -> Self {
        let opts = (tch::Kind::from(dtype), tch::Device::from(device));
        Tensor::from_tch(tch::Tensor::full(shape, val, opts))
    }

    pub fn arange(start: i64, end: i64, device: Device) -> Self {
        let t =
            tch::Tensor::arange(end - start, (tch::Kind::Int64, tch::Device::from(device))) + start;
        Tensor::from_tch(t)
    }

    pub fn arange_f(start: f64, end: f64, step: f64, dtype: DType, device: Device) -> Self {
        let t = tch::Tensor::arange_start_step(
            start,
            end,
            step,
            (tch::Kind::from(dtype), tch::Device::from(device)),
        );
        Tensor::from_tch(t)
    }

    pub fn cat(tensors: &[Tensor], dim: i64) -> Self {
        let inner: Vec<&tch::Tensor> = tensors.iter().map(|t| &t.inner).collect();
        Tensor::from_tch(tch::Tensor::cat(&inner, dim))
    }

    pub fn stack(tensors: &[Tensor], dim: i64) -> Self {
        let inner: Vec<&tch::Tensor> = tensors.iter().map(|t| &t.inner).collect();
        Tensor::from_tch(tch::Tensor::stack(&inner, dim))
    }

    pub fn embedding(weight: &Tensor, indices: &Tensor) -> Self {
        Tensor::from_tch(tch::Tensor::embedding(
            &weight.inner,
            &indices.inner,
            -1,
            false,
            false,
        ))
    }

    pub fn hann_window(size: i64, device: Device) -> Self {
        Tensor::from_tch(tch::Tensor::hann_window(
            size,
            (tch::Kind::Float, tch::Device::from(device)),
        ))
    }

    // -- Shape --

    pub fn size(&self) -> Vec<i64> {
        self.inner.size()
    }

    pub fn size3(&self) -> (i64, i64, i64) {
        self.inner.size3().unwrap()
    }

    pub fn size4(&self) -> (i64, i64, i64, i64) {
        self.inner.size4().unwrap()
    }

    pub fn dim(&self) -> usize {
        self.inner.dim()
    }

    pub fn view(&self, shape: &[i64]) -> Self {
        Tensor::from_tch(self.inner.view(shape))
    }

    pub fn reshape(&self, shape: &[i64]) -> Self {
        Tensor::from_tch(self.inner.reshape(shape))
    }

    pub fn narrow(&self, dim: i64, start: i64, len: i64) -> Self {
        Tensor::from_tch(self.inner.narrow(dim, start, len))
    }

    pub fn unsqueeze(&self, dim: i64) -> Self {
        Tensor::from_tch(self.inner.unsqueeze(dim))
    }

    pub fn squeeze_dim(&self, dim: i64) -> Self {
        Tensor::from_tch(self.inner.squeeze_dim(dim))
    }

    pub fn transpose(&self, dim0: i64, dim1: i64) -> Self {
        Tensor::from_tch(self.inner.transpose(dim0, dim1))
    }

    pub fn permute(&self, dims: &[i64]) -> Self {
        Tensor::from_tch(self.inner.permute(dims))
    }

    pub fn expand(&self, size: &[i64], implicit: bool) -> Self {
        Tensor::from_tch(self.inner.expand(size, implicit))
    }

    pub fn contiguous(&self) -> Self {
        Tensor::from_tch(self.inner.contiguous())
    }

    pub fn tr(&self) -> Self {
        Tensor::from_tch(self.inner.tr())
    }

    pub fn get(&self, index: i64) -> Self {
        Tensor::from_tch(self.inner.get(index))
    }

    pub fn select(&self, dim: i64, index: i64) -> Self {
        Tensor::from_tch(self.inner.select(dim, index))
    }

    // -- Arithmetic --

    pub fn matmul(&self, other: &Tensor) -> Self {
        Tensor::from_tch(self.inner.matmul(&other.inner))
    }

    /// Fused scaled dot-product attention (FlashAttention2 on CUDA).
    /// Q/K/V: [batch, heads, seq, head_dim]. Mask is additive float.
    pub fn scaled_dot_product_attention(
        query: &Tensor,
        key: &Tensor,
        value: &Tensor,
        scale: f64,
        mask: Option<&Tensor>,
    ) -> Tensor {
        let q_heads = query.size()[1];
        let kv_heads = key.size()[1];
        let enable_gqa = q_heads != kv_heads;
        // When an explicit causal mask is passed for prefill, use is_causal=true
        // instead — this lets SDPA use FlashAttention's built-in causal masking
        // and avoids dtype mismatch (mask is f32 but query may be bf16).
        let seq_len = query.size()[2];
        let kv_len = key.size()[2];
        let is_prefill_causal = mask.is_some() && seq_len > 1 && seq_len == kv_len;
        let (use_mask, is_causal) = if is_prefill_causal {
            (None, true) // Let SDPA handle causal masking internally
        } else {
            (mask, false)
        };
        // Cast mask to query dtype if needed (SDPA requires matching dtypes)
        let mask_cast = use_mask.map(|m| {
            if m.kind() == query.kind() {
                m.shallow_clone()
            } else {
                m.to_dtype(query.kind())
            }
        });
        // Ensure Q/K/V have matching dtypes (SDPA requirement)
        let q_dtype = query.kind();
        let key = if key.kind() == q_dtype {
            key.shallow_clone()
        } else {
            key.to_dtype(q_dtype)
        };
        let value = if value.kind() == q_dtype {
            value.shallow_clone()
        } else {
            value.to_dtype(q_dtype)
        };
        Tensor::from_tch(tch::Tensor::scaled_dot_product_attention(
            &query.inner,
            &key.inner,
            &value.inner,
            mask_cast.as_ref().map(|m| &m.inner),
            0.0,
            is_causal,
            Some(scale),
            enable_gqa,
        ))
    }

    pub fn pow_scalar(&self, exp: f64) -> Self {
        Tensor::from_tch(self.inner.pow_tensor_scalar(exp))
    }

    pub fn neg(&self) -> Self {
        Tensor::from_tch(self.inner.neg())
    }

    pub fn clamp_min(&self, min: f64) -> Self {
        Tensor::from_tch(self.inner.clamp_min(min))
    }

    pub fn maximum(&self, other: &Tensor) -> Self {
        Tensor::from_tch(self.inner.maximum(&other.inner))
    }

    // -- Math --

    pub fn abs(&self) -> Self {
        Tensor::from_tch(self.inner.abs())
    }

    pub fn square(&self) -> Self {
        Tensor::from_tch(self.inner.square())
    }

    pub fn sqrt(&self) -> Self {
        Tensor::from_tch(self.inner.sqrt())
    }

    pub fn rsqrt(&self) -> Self {
        let s = self.inner.sqrt();
        Tensor::from_tch(s.reciprocal())
    }

    pub fn log10(&self) -> Self {
        Tensor::from_tch(self.inner.log10())
    }

    pub fn sin(&self) -> Self {
        Tensor::from_tch(self.inner.sin())
    }

    pub fn cos(&self) -> Self {
        Tensor::from_tch(self.inner.cos())
    }

    pub fn exp(&self) -> Self {
        Tensor::from_tch(self.inner.exp())
    }

    // -- Activations --

    pub fn softmax(&self, dim: i64) -> Self {
        Tensor::from_tch(self.inner.softmax(dim, tch::Kind::Float))
    }

    pub fn gelu(&self) -> Self {
        Tensor::from_tch(self.inner.gelu("none"))
    }

    pub fn silu(&self) -> Self {
        Tensor::from_tch(self.inner.silu())
    }

    // -- Reduction --

    pub fn mean_dim(&self, dims: &[i64], keepdim: bool) -> Self {
        Tensor::from_tch(self.inner.mean_dim(dims, keepdim, tch::Kind::Float))
    }

    pub fn max(&self) -> Self {
        Tensor::from_tch(self.inner.max())
    }

    // -- Indexing --

    pub fn argmax(&self, dim: i64, keepdim: bool) -> Self {
        Tensor::from_tch(self.inner.argmax(dim, keepdim))
    }

    pub fn triu(&self, diagonal: i64) -> Self {
        Tensor::from_tch(self.inner.triu(diagonal))
    }

    pub fn slice_scatter(&self, src: &Tensor, dim: i64, start: i64, end: i64, step: i64) -> Self {
        Tensor::from_tch(
            self.inner
                .slice_scatter(&src.inner, dim, Some(start), Some(end), step),
        )
    }

    pub fn fill_(&mut self, val: f64) {
        let _ = self.inner.fill_(val);
    }

    // -- Normalization --

    pub fn rms_norm(&self, weight: &Tensor, eps: f64) -> Self {
        let dtype = self.kind();
        let x = self.to_dtype(DType::Float32);
        let weight = weight.to_dtype(DType::Float32);
        let variance = (&x.inner * &x.inner).mean_dim([-1i64].as_slice(), true, tch::Kind::Float);
        let x_normed = &x.inner * (variance + eps).rsqrt();
        (Tensor::from_tch(x_normed) * &weight).to_dtype(dtype)
    }

    pub fn layer_norm(
        &self,
        normalized_shape: &[i64],
        weight: Option<&Tensor>,
        bias: Option<&Tensor>,
        eps: f64,
    ) -> Self {
        Tensor::from_tch(self.inner.layer_norm(
            normalized_shape,
            weight.map(|w| &w.inner),
            bias.map(|b| &b.inner),
            eps,
            true,
        ))
    }

    /// Apply rotary position embeddings using cos/sin tensors.
    pub fn apply_rope(&self, cos: &Tensor, sin: &Tensor) -> Tensor {
        let cos = cos.unsqueeze(0).unsqueeze(0);
        let sin = sin.unsqueeze(0).unsqueeze(0);
        let half = self.size().last().unwrap() / 2;
        let x1 = self.narrow(-1, 0, half);
        let x2 = self.narrow(-1, half, half);
        let x_rotated = Tensor::cat(&[(-&x2), x1], -1);
        self * &cos + x_rotated * &sin
    }

    // -- Convolution --

    pub fn conv2d(
        &self,
        weight: &Tensor,
        bias: Option<&Tensor>,
        stride: &[i64],
        padding: &[i64],
        dilation: &[i64],
        groups: i64,
    ) -> Self {
        let bias_inner = bias.map(|b| &b.inner);
        Tensor::from_tch(self.inner.conv2d(
            &weight.inner,
            bias_inner,
            stride,
            padding,
            dilation,
            groups,
        ))
    }

    // -- Signal --

    pub fn reflection_pad1d(&self, pad: &[i64]) -> Self {
        Tensor::from_tch(self.inner.reflection_pad1d(pad))
    }

    pub fn stft(&self, config: StftConfig<'_>) -> Self {
        Tensor::from_tch(self.inner.stft(
            config.n_fft,
            Some(config.hop_length),
            Some(config.win_length),
            Some(&config.window.inner),
            config.normalized,
            config.onesided,
            config.return_complex,
            false,
        ))
    }

    // -- Type / Device --

    pub fn to_dtype(&self, dtype: DType) -> Self {
        Tensor::from_tch(self.inner.to_kind(tch::Kind::from(dtype)))
    }

    pub fn to_device(&self, device: Device) -> Self {
        Tensor::from_tch(self.inner.to_device(tch::Device::from(device)))
    }

    pub fn kind(&self) -> DType {
        DType::from(self.inner.kind())
    }

    pub fn device(&self) -> Device {
        Device::from(self.inner.device())
    }

    pub fn shallow_clone(&self) -> Self {
        Tensor::from_tch(self.inner.shallow_clone())
    }

    pub fn copy(&self) -> Self {
        Tensor::from_tch(self.inner.copy())
    }

    /// Evaluate tensor (no-op for tch which uses eager execution).
    pub fn eval(&self) {}

    // -- Data extraction --

    pub fn int64_value(&self, indices: &[i64]) -> i64 {
        self.inner.int64_value(indices)
    }

    pub fn f64_value(&self, indices: &[i64]) -> f64 {
        self.inner.double_value(indices)
    }

    pub fn to_vec_f32(&self) -> Vec<f32> {
        let flat = self.inner.view(-1);
        let numel = flat.numel();
        let mut result = vec![0.0f32; numel];
        flat.to_kind(tch::Kind::Float).copy_data(&mut result, numel);
        result
    }
}
