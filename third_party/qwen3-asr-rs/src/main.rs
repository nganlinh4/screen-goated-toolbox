use anyhow::{Context, Result};
use std::path::Path;

use qwen3_asr_rs::cuda_runtime::{
    force_cuda_requested, maybe_reexec_with_cuda_preload, preload_cuda_runtime,
};
use qwen3_asr_rs::tensor::Device;
use qwen3_asr_rs::inference::{
    AsrInference, KvCacheMode, kv_cache_mode_from_name, kv_cache_mode_name,
};

fn resolve_kv_cache_mode() -> (KvCacheMode, Option<String>) {
    let requested = std::env::var("SGT_QWEN3_RUNTIME_KV_MODE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let mode = requested
        .as_deref()
        .and_then(kv_cache_mode_from_name)
        .unwrap_or_else(|| {
            if let Some(requested) = requested.as_deref() {
                tracing::warn!(
                    "Unrecognized SGT_QWEN3_RUNTIME_KV_MODE='{}'; defaulting to dense_append",
                    requested
                );
            }
            KvCacheMode::DenseAppend
        });
    (mode, requested)
}

fn main() -> Result<()> {
    if let Err(err) = maybe_reexec_with_cuda_preload() {
        eprintln!("warning: failed to re-exec with Linux CUDA preload: {err}");
    }
    preload_cuda_runtime();

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!("Qwen3 ASR - Automatic Speech Recognition");
        eprintln!();
        eprintln!("Usage: asr <model_path> <audio_file> [language]");
        eprintln!();
        eprintln!("Arguments:");
        eprintln!("  model_path   Path to the Qwen3-ASR model directory");
        eprintln!("  audio_file   Path to the input audio file (any format supported by ffmpeg)");
        eprintln!("  language     Optional: force language (e.g., chinese, english, japanese)");
        eprintln!();
        eprintln!("The audio file will be automatically converted to mono 16kHz f32 for the model.");
        eprintln!();
        eprintln!("Environment variables:");
        #[cfg(feature = "tch-backend")]
        eprintln!("  LIBTORCH     Path to libtorch installation");
        eprintln!("  RUST_LOG     Set logging level (e.g., info, debug, trace)");
        std::process::exit(1);
    }

    let model_path = &args[1];
    let audio_file = &args[2];
    let language = args.get(3).map(|s| s.as_str());

    // Verify paths exist
    let model_dir = Path::new(model_path);
    if !model_dir.exists() {
        anyhow::bail!("Model directory not found: {}", model_path);
    }
    if !Path::new(audio_file).exists() {
        anyhow::bail!("Audio file not found: {}", audio_file);
    }

    // Select device
    #[cfg(feature = "tch-backend")]
    let device = if force_cuda_requested() {
        tracing::warn!("Forcing CUDA device selection via environment override");
        Device::Gpu(0)
    } else if tch::Cuda::is_available() {
        tracing::info!("Using CUDA device");
        Device::Gpu(0)
    } else {
        tracing::info!("Using CPU device");
        Device::Cpu
    };

    #[cfg(feature = "mlx")]
    let device = {
        qwen3_asr_rs::backend::mlx::stream::init_mlx(true);
        tracing::info!("Using MLX Metal GPU");
        Device::Gpu(0)
    };

    // Load model
    let (kv_cache_mode, requested_kv_cache_mode) = resolve_kv_cache_mode();
    if let Some(requested_kv_cache_mode) = requested_kv_cache_mode.as_deref() {
        let canonical_kv_cache_mode = kv_cache_mode_name(kv_cache_mode);
        if requested_kv_cache_mode == canonical_kv_cache_mode {
            tracing::info!("Using KV cache mode: {}", canonical_kv_cache_mode);
        } else {
            tracing::info!(
                "Using KV cache mode: {} (requested: {})",
                canonical_kv_cache_mode,
                requested_kv_cache_mode
            );
        }
    } else {
        tracing::info!("Using KV cache mode: {}", kv_cache_mode_name(kv_cache_mode));
    }
    let model = AsrInference::load_with_kv_mode(model_dir, device, kv_cache_mode)
        .context("Failed to load model")?;

    // Run transcription
    tracing::info!("Transcribing: {}", audio_file);
    let result = model
        .transcribe(audio_file, language)
        .context("Transcription failed")?;

    // Output result
    println!("Language: {}", result.language);
    println!("Text: {}", result.text);

    Ok(())
}
