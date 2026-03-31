# Qwen3 TurboQuant Research

This note captures the current high-signal references for a native Qwen3 TurboQuant runtime.

## Current status

- The vendored Rust sidecar in `third_party/qwen3-asr-rs` is the reference backend and oracle.
- The future native runtime is a separate Windows DLL seam at `src/api/realtime_audio/qwen3/runtime.rs`.
- The best next step is decoder-first acceleration, not a full-model rewrite in one move.

## Local clones

These were cloned locally for offline reading:

- `Qwen3-ASR`: `/tmp/qwen_streaming_research/Qwen3-ASR`
- `vLLM`: `/tmp/qwen_streaming_research/vllm`
- `TensorRT-LLM`: `/tmp/turboquant_research/TensorRT-LLM`
- `FlashInfer`: `/tmp/turboquant_research/flashinfer`
- `Marlin`: `/tmp/turboquant_research/marlin`

## What each repo is for

### 1. Official Qwen streaming semantics

Source:
- `/tmp/qwen_streaming_research/Qwen3-ASR/qwen_asr/inference/qwen3_asr.py`

Why it matters:
- This is the authoritative streaming state machine we should match semantically.
- It defines `init_streaming_state`, `streaming_transcribe`, and `finish_streaming_transcribe`.
- It proves that official streaming is still accumulated-audio plus rollback-prefix prompting.

Important lines:
- `streaming_transcribe`: rebuilds `prompt = state.prompt_raw + prefix`
- `_raw_decoded = prefix + gen_text`
- first `unfixed_chunk_num` chunks use no prefix
- later chunks roll back `unfixed_token_num` tokens

### 2. Official Qwen vLLM decode configuration

Source:
- `/tmp/qwen_streaming_research/Qwen3-ASR/examples/example_qwen3_asr_vllm_streaming.py`
- `/tmp/qwen_streaming_research/Qwen3-ASR/qwen_asr/inference/qwen3_asr.py`

Why it matters:
- Streaming uses greedy decode semantics with a small token budget.
- The official example sets `max_new_tokens=32` for streaming.
- The vLLM backend is constructed with `SamplingParams(temperature=0.0, max_tokens=max_new_tokens)`.

This is the reference for decode policy, not kernel design.

### 3. TensorRT-LLM: quantized decoder and KV cache design

Source:
- `/tmp/turboquant_research/TensorRT-LLM/cpp/tensorrt_llm/common/attentionOp.h`
- `/tmp/turboquant_research/TensorRT-LLM/cpp/tensorrt_llm/runtime/gptJsonConfig.cpp`
- `/tmp/turboquant_research/TensorRT-LLM/tensorrt_llm/llmapi/llm_args.py`

Why it matters:
- This is the strongest reference for how modern CUDA inference runtimes represent quantized KV cache and attention settings.
- `attentionOp.h` explicitly handles KV cache element sizes for int8, fp8, and fp4.
- `gptJsonConfig.cpp` parses both `quant_algo` and `kv_cache_quant_algo`.
- `llm_args.py` shows the runtime-facing configuration layer for `fp8` and `nvfp4` KV cache.

Key takeaway:
- Quantized KV cache is a first-class runtime concern, not a side detail.
- A native TurboQuant runtime should expose quant mode and KV-cache mode separately.

### 4. FlashInfer: paged attention and decode kernels

Source:
- `/tmp/turboquant_research/flashinfer/csrc/batch_attention.cu`
- `/tmp/turboquant_research/flashinfer/csrc/batch_decode_mla_binding.cu`
- `/tmp/turboquant_research/flashinfer/csrc/trtllm_fmha_v2_binding.cu`

Why it matters:
- This is the best compact reference for modern paged decode attention.
- It is especially valuable for page-table layout, planning, and launch structure.
- It also shows how FlashInfer interoperates with TensorRT-LLM-style attention bindings.

Key takeaway:
- If we build a native session runtime, paged KV layout should be designed early.
- Even if we start with continuous KV cache, the runtime ABI should not block a paged layout later.

### 5. Marlin: low-bit GEMM kernel design

Source:
- `/tmp/turboquant_research/marlin/README.md`
- `/tmp/turboquant_research/marlin/marlin/marlin_cuda_kernel.cu`

Why it matters:
- This is a focused reference for weight-only low-bit decoder linear layers.
- It is small enough to actually study line by line.
- The kernel demonstrates:
  - offline weight/scales reshuffling
  - async global-to-shared movement
  - fused dequant plus tensor-core MMA
  - decoder-oriented throughput targets

Key takeaway:
- For a first TurboQuant milestone, decoder linear layers are the right place to start.
- Marlin is a stronger implementation reference than generic quantization papers.

## What is not worth copying directly

- The full Qwen Python/vLLM stack is too broad to port literally.
- TensorRT-LLM is too large to mirror architecture-for-architecture.
- FlashInfer is best used as a kernel and layout reference, not as an application framework.

## Recommended implementation order

### Milestone 1: native runtime shell

Use:
- `src/api/realtime_audio/qwen3/runtime.rs`

Goal:
- replace the placeholder DLL contract with a real session ABI
- create and destroy native sessions
- keep the current sidecar as the oracle

### Milestone 2: decoder-only acceleration

Target first:
- text decoder projections and generation path
- do not quantize the audio encoder yet

Repo references:
- `third_party/qwen3-asr-rs/src/text_decoder.rs`
- `third_party/qwen3-asr-rs/src/layers.rs`
- `third_party/qwen3-asr-rs/src/inference.rs`
- `Marlin`

Goal:
- weight-only low-bit decoder linear layers
- keep output parity against the sidecar oracle

### Milestone 3: KV cache policy

Repo references:
- `TensorRT-LLM attentionOp.h`
- `FlashInfer batch_attention.cu`

Goal:
- add an internal KV cache abstraction that can later support:
  - fp16 or bf16
  - fp8
  - maybe fp4

### Milestone 4: attention kernel replacement

Repo references:
- `FlashInfer`
- `TensorRT-LLM`

Goal:
- replace the slowest attention/decode path after linear layers are already accelerated

## Strongest current conclusion

There is no single public "TurboQuant" codebase to copy.

The best practical synthesis for this repo is:

1. Qwen repo for streaming semantics
2. vLLM example for decode policy
3. TensorRT-LLM for quantized decoder and KV-cache runtime structure
4. FlashInfer for paged attention execution model
5. Marlin for compact low-bit GEMM kernel design

That combination is a realistic foundation for an SGT-owned native CUDA runtime.
