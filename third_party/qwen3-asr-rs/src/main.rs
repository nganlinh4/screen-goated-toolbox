use anyhow::{Context, Result};
use std::path::Path;

use qwen3_asr_rs::tensor::Device;
use qwen3_asr_rs::inference::AsrInference;

fn force_cuda_requested() -> bool {
    std::env::var("SGT_QWEN3_FORCE_CUDA")
        .ok()
        .or_else(|| std::env::var("QWEN3_FORCE_CUDA").ok())
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

#[cfg(target_os = "windows")]
fn preload_cuda_runtime() {
    unsafe extern "system" {
        fn LoadLibraryA(lp_lib_file_name: *const u8) -> *mut core::ffi::c_void;
    }

    for dll in [
        b"c10_cuda.dll\0".as_slice(),
        b"torch_cuda.dll\0".as_slice(),
        b"cudart64_12.dll\0".as_slice(),
    ] {
        let _ = unsafe { LoadLibraryA(dll.as_ptr()) };
    }
}

#[cfg(not(target_os = "windows"))]
fn preload_cuda_runtime() {}

fn main() -> Result<()> {
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
    let model = AsrInference::load(model_dir, device).context("Failed to load model")?;

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
