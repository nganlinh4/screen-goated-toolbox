# Qwen3-ASR-RS Development Guide

## Project Overview

Pure Rust implementation of Qwen3-ASR (automatic speech recognition) with two backends:
- **libtorch** (via `tch` crate) — cross-platform, optional CUDA
- **MLX** (Apple Silicon) — Metal GPU via `mlx-c` submodule

Loads safetensors weights directly and re-implements the full neural network forward pass.

Git DCO: `Signed-off-by: Michael Yuan <michael@secondstate.io>`

## Build Commands

```bash
# macOS (MLX backend — recommended for Apple Silicon)
git submodule update --init --recursive
cargo build --release --no-default-features --features mlx,build-ffmpeg

# Linux (libtorch backend)
export LIBTORCH=$(pwd)/libtorch
export LIBTORCH_BYPASS_VERSION_CHECK=1
cargo build --release --features build-ffmpeg
```

Features are mutually exclusive: `tch-backend` (default) XOR `mlx`. Enforced at compile time in `lib.rs`.

## Architecture

Pipeline: **Audio → FFmpeg decode → 16kHz mono → Mel spectrogram → Audio encoder → Text decoder → Transcription**

### Model Architecture (Qwen3-ASR-0.6B defaults)

**Audio Encoder** (Whisper-style, `audio_encoder.rs`):
- 3x Conv2d downsampling (stride 2, 8x total reduction)
- Sinusoidal positional embeddings (computed at runtime, not stored in weights)
- 18 transformer layers (d_model=896, 14 attention heads)
- Output projection: LayerNorm → Linear → GELU → Linear (896 → 1024)
- Chunk-based: splits mel into n_window*2 (100) frame segments, processes ALL as a single batch

**Text Decoder** (Qwen3, `text_decoder.rs`):
- 28 transformer layers (hidden_size=1024, head_dim=128)
- GQA: 16 Q heads, 8 KV heads
- QK-normalization via per-head RMSNorm
- MRoPE (Multimodal Rotary Position Embeddings), section sizes [24, 20, 20]
- SwiGLU MLP (gate_proj → silu, up_proj, down_proj)
- Tied word embeddings (embed_tokens = lm_head by default)

### Key File Map

| File | Purpose |
|------|---------|
| `tensor.rs` | Unified Tensor abstraction over tch/MLX (largest file, ~1200 lines) |
| `layers.rs` | All transformer building blocks (norms, linear, attention, MLP, MRoPE) |
| `inference.rs` | End-to-end ASR pipeline orchestration |
| `audio_encoder.rs` | Whisper-style encoder with chunk-based Conv2d processing |
| `text_decoder.rs` | Qwen3 decoder with KV cache |
| `mel.rs` | Whisper-style 128-bin log-mel spectrogram (Slaney scale) |
| `audio.rs` | FFmpeg audio loading + WAV/rubato fallback |
| `weights.rs` | Safetensors loading (single-file + sharded, bf16/f16 → f32) |
| `config.rs` | Model configuration deserialization |
| `tokenizer.rs` | HuggingFace tokenizer wrapper |
| `backend/mlx/` | MLX C FFI bindings, safe array wrappers, ops, signal processing |

## Critical Implementation Details

### Mel Spectrogram (`mel.rs`)
- Uses **Slaney** mel scale (NOT HTK): linear below 1000 Hz, logarithmic above
- STFT requires center padding via `reflection_pad1d` (n_fft//2 on each side)
- Must remove last STFT frame: `magnitudes[..., :-1]`
- Pad audio samples to next multiple of hop_length
- Log-mel normalization: `log10().clamp(max-8).add(4)/4`
- Do NOT use tch's `stft_center` — it errors when `align_to_window` is set

### Audio Encoder Chunking (`audio_encoder.rs`)
- Chunk mel into n_window*2 (100) frame segments
- **Process ALL chunks as a single batch** through Conv2d (not one-at-a-time)
- Tail chunks zero-padded to chunk_size before Conv2d, then valid tokens extracted
- Block-diagonal windowed attention mask for chunk boundaries
- Output token count per chunk: apply `(len-1)//2 + 1` three times (one per Conv2d)

### MRoPE Position IDs (CRITICAL)
- All 3 MRoPE dimensions use **identical** sequential positions [0, 1, 2, ..., N-1]
- NO special handling for audio tokens — all dims are the same
- Wrong position IDs cause completely wrong transcription output
- Section sizes [24, 20, 20] = 64 total (half of head_dim=128), mapped contiguously by default
- cos/sin are doubled: `cos[j] == cos[j + half_dim]` for standard RoPE pattern

### Inference Loop (`inference.rs`)
- Prompt format: `<|im_start|>system\n<|im_end|>\n<|im_start|>user\n<|audio_start|>[AUDIO_PAD × N]<|audio_end|><|im_end|>\n<|im_start|>assistant\n`
- Audio embeddings injected via `slice_scatter` at AUDIO_PAD positions
- Prefill: full causal mask, populates KV cache for all layers
- Decode: single-token steps, **no mask needed** (causal mask for seq_len=1 is all-zeros)
- MRoPE cos/sin precomputed for all positions, indexed per step via `narrow`
- EOS tokens: ENDOFTEXT (151643) or IM_END (151645)

### Special Token IDs (`tokenizer.rs`)
| Token | ID |
|-------|-----|
| IM_START | 151644 |
| IM_END | 151645 |
| ENDOFTEXT | 151643 |
| AUDIO_START | 151669 |
| AUDIO_END | 151670 |
| AUDIO_PAD | 151676 |
| ASR_TEXT | 151704 |

## MLX Backend Notes

### Lazy Evaluation Model
MLX uses lazy evaluation: operations build a computation graph that executes only when `eval()` is called or data is read. Call `eval()` at outer loop boundaries, not per-layer.

Current eval placement:
- After audio encoder forward (materialize before decode phase)
- After prefill logits (materialize before decode loop)
- `int64_value()` / `item_i64()` implicitly call eval (forces graph execution for argmax)

### Fused Operations (Applied)
These replace multiple discrete ops with single Metal kernels:

1. **Fused RmsNorm** (`mlx_fast_rms_norm`): Replaces cast → square → mean → add_eps → rsqrt → multiply → cast back. Used by `Tensor::rms_norm()`.

2. **Fused SDPA** (`mlx_fast_scaled_dot_product_attention`): Replaces manual QK^T/scale + mask + softmax + V matmul. Natively handles GQA (different Q/KV head counts, no `repeat_kv` needed).
   - `mask_mode` values: `"causal"`, `"array"`, or `""` (empty string). **NOT** `"none"` — that causes a runtime error.
   - Pass `mask_mode = "array"` with an additive float mask tensor, or `""` with null mask.

3. **Fused LayerNorm** (`mlx_fast_layer_norm`): Used by the audio encoder's LayerNorm.

4. **Fused RoPE** (`mlx_fast_rope`): Available in `ops.rs` but not yet integrated into the attention layer because MRoPE uses cos/sin tensors rather than offset-based API. Integration would require changing the attention interface. Since all 3 MRoPE dims are identical, standard fast_rope with a single offset would work.

### Channels-Last Layout
MLX uses NHWC (channels-last), PyTorch uses NCHW. The `Tensor::conv2d()` method handles the transpose automatically:
- Input: `permute([0, 2, 3, 1])` — NCHW → NHWC
- Weight: `permute([0, 2, 3, 1])` — [C_out, C_in, kH, kW] → [C_out, kH, kW, C_in]
- Output: `permute([0, 3, 1, 2])` — NHWC → NCHW

Conv2d weights could be pre-transposed at load time for a minor speedup (only affects audio encoder, runs once).

### Weight Loading
- `MlxArray::clone()` uses `mlx_array_set()` for O(1) shallow cloning (reference counting, no data copy)
- Safetensors loaded via `mlx_load_safetensors` FFI call, returns map of name → MlxArray

### Avoiding CPU Round-Trips
- Never call `to_vec_f32()` or `eval()` mid-graph unless explicitly reading data
- Argmax uses `Tensor::argmax()` on GPU, only the scalar token ID transfers to CPU (4 bytes)
- Any interruption of lazy evaluation breaks Metal kernel fusion

### Pre-transposed Weights
- `Linear` stores `weight_t = weight.tr()` at load time to avoid per-forward transpose
- `TextDecoder` stores `lm_head_weight_t` similarly
- On MLX this is mostly cosmetic (lazy eval fuses the transpose anyway), but cleaner

## tch Backend Notes

- `stft_center` with `center=true` errors when `align_to_window` is set (even to false) — use manual `reflection_pad1d` + regular `stft`
- `Tensor::eval()` is a no-op (tch uses eager execution)
- Weight loading does bf16/f16 → f32 conversion via custom `bf16_bytes_to_f32()` / `half_to_float()`

### Cross-Backend API Pitfall: Scale Convention
**CRITICAL**: When adding shared `Tensor` methods that serve both tch and MLX backends, pay close attention to parameter semantics.

MLX's `fast_sdpa` takes `scale` as a **multiplier** (`1/sqrt(hd)`), computing `softmax(Q @ K^T * scale) @ V`. The tch manual SDPA must also **multiply** by this scale value. A past bug had the tch SDPA dividing by `scale = 1/sqrt(hd)`, effectively computing `Q*K^T * sqrt(hd)` — the inverse of the correct scaling. This produced wildly wrong softmax distributions and garbage transcription.

Rule: when wrapping MLX fused ops behind a shared `Tensor` API, the tch fallback must use the **same parameter convention** as the MLX kernel. Always verify both backends produce identical numerical results for the same inputs.

## Testing

```bash
# Basic transcription
./target/release/asr ./Qwen3-ASR-0.6B test_audio/sample1.wav

# Expected output:
# Language: English
# Text: Thank you for your contribution to the most recent issue of Computer...

# Force language
./target/release/asr ./Qwen3-ASR-0.6B test_audio/sample3.wav chinese

# Debug logging
RUST_LOG=debug ./target/release/asr ./Qwen3-ASR-0.6B test_audio/sample1.wav
```

Test audio files:
- `speech_en.wav` — "Hello world. This is a test of automatic speech recognition."
- `test_audio/sample1.wav` — "Thank you for your contribution..." (31 tokens, good for benchmarking)
- `test_audio/sample2.wav` — "The quick brown fox jumps over the lazy dog."
- `test_audio/sample3.wav` — Chinese: "你好，这是宽森语音合成系统的持续集成测试。"

## Performance Notes

Benchmarked on Apple Silicon (MLX, 0.6B model, sample1.wav → 31 tokens):
- First run includes Metal shader compilation (~3.0s)
- Warm runs: ~2.3s
- Decode loop is the bottleneck (28 layers × 31 steps × 7+ matmuls per layer)

## Future Optimization Opportunities

1. **Fused RoPE integration**: Use `mlx_fast_rope` with offset parameter instead of precomputed cos/sin tensors. Since all 3 MRoPE dims are identical, this would work with a single offset. Requires changing the attention layer interface.

2. **Pre-allocated KV cache**: Currently each decode step allocates new tensors via `Tensor::cat([past, new])`. Pre-allocating max-size buffers and writing into them would eliminate repeated allocation/copy.

3. **Conv2d weight pre-transpose**: Store weights in MLX NHWC format at load time to avoid per-forward transpose (minor, audio encoder only).

4. **BFloat16 inference**: Run the model in BF16 instead of F32 to halve memory bandwidth (currently all weights converted to F32 at load time).
