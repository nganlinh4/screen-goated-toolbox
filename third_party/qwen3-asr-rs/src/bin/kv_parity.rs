use anyhow::{Context, Result};
use clap::Parser;
use qwen3_asr_rs::audio;
use qwen3_asr_rs::cuda_runtime::{
    force_cuda_requested, maybe_reexec_with_cuda_preload, preload_cuda_runtime,
};
use qwen3_asr_rs::inference::AsrInference;
use qwen3_asr_rs::text_decoder::KvCacheMode;
use qwen3_asr_rs::streaming::{StreamingConfig, StreamingState, StreamingTranscript};
use qwen3_asr_rs::tensor::Device;
use serde::Serialize;
use std::path::Path;
const SAMPLE_RATE: usize = 16_000;
#[derive(Parser, Debug)]
struct Args {
    model_path: String,
    audio_file: String,
    #[arg(long)]
    language: Option<String>,
    #[arg(long, default_value_t = 2000)]
    chunk_size_ms: u32,
    #[arg(long, default_value_t = 2)]
    unfixed_chunk_num: usize,
    #[arg(long, default_value_t = 5)]
    unfixed_token_num: usize,
    #[arg(long, default_value_t = false)]
    fail_on_divergence: bool,
    #[arg(long, default_value_t = false)]
    ignore_whitespace: bool,
    #[arg(long, default_value_t = 0.0)]
    min_final_kv_reduction_ratio: f64,
    #[arg(long, default_value_t = 0)] max_streaming_divergences: usize,
    #[arg(long, default_value_t = usize::MAX)] max_worst_streaming_divergence_field_count: usize,
    #[arg(long, default_value_t = 1.0)] min_streaming_match_ratio: f64,
    #[arg(long, default_value_t = usize::MAX)]
    max_language_divergences: usize,
    #[arg(long, default_value_t = usize::MAX)]
    max_fixed_text_divergences: usize,
    #[arg(long, default_value_t = usize::MAX)]
    max_draft_text_divergences: usize,
    #[arg(long, default_value_t = usize::MAX)]
    max_text_divergences: usize,
    #[arg(long, default_value_t = 0.0)]
    min_peak_kv_reduction_ratio: f64,
}
#[derive(Serialize)]
struct ParityReport {
    audio_samples: usize,
    chunk_samples: usize,
    summary: ParitySummary,
    offline_dense: TranscriptResult,
    offline_compressed: TranscriptResult,
    streaming_steps: Vec<StepPair>,
    final_dense: StepTranscript,
    final_compressed: StepTranscript,
    offline_divergence: Option<Divergence>,
    first_divergence: Option<Divergence>,
}
#[derive(Serialize)]
struct ParitySummary {
    offline_match: bool,
    final_match: bool,
    streaming_match: bool,
    streaming_match_count: usize,
    streaming_match_ratio: f64,
    streaming_divergence_count: usize,
    language_divergence_count: usize,
    fixed_text_divergence_count: usize,
    draft_text_divergence_count: usize,
    text_divergence_count: usize,
    first_divergence_chunk_id: Option<usize>,
    first_divergence_audio_samples: Option<usize>,
    worst_streaming_divergence_chunk_id: Option<usize>,
    worst_streaming_divergence_audio_samples: Option<usize>,
    worst_streaming_divergence_field_count: usize,
    dense_final_kv_bytes: usize,
    compressed_final_kv_bytes: usize,
    final_kv_reduction_ratio: f64,
    dense_peak_kv_bytes: usize,
    compressed_peak_kv_bytes: usize,
    peak_kv_reduction_ratio: f64,
    min_streaming_kv_reduction_ratio: f64,
}
#[derive(Serialize)]
struct TranscriptResult {
    language: String,
    text: String,
    raw_output: String,
    kv_cache_bytes: usize,
    kv_cache_dense_bytes: usize,
}
#[derive(Serialize)]
struct StepPair {
    chunk_id: usize,
    audio_samples: usize,
    dense: StepTranscript,
    compressed: StepTranscript,
}
#[derive(Serialize, Clone)]
struct StepTranscript {
    language: String,
    fixed_text: String,
    draft_text: String,
    text: String,
    kv_cache_bytes: usize,
    kv_cache_dense_bytes: usize,
}
#[derive(Serialize)]
struct Divergence {
    chunk_id: usize,
    audio_samples: usize,
    field: &'static str,
    dense: String,
    compressed: String,
}
fn select_device() -> Device {
    #[cfg(feature = "tch-backend")]
    {
        if force_cuda_requested() {
            tracing::warn!("Forcing CUDA device selection via environment override");
            Device::Gpu(0)
        } else if tch::Cuda::is_available() {
            tracing::info!("Using CUDA device");
            Device::Gpu(0)
        } else {
            tracing::info!("Using CPU device");
            Device::Cpu
        }
    }

    #[cfg(feature = "mlx")]
    {
        qwen3_asr_rs::backend::mlx::stream::init_mlx(true);
        tracing::info!("Using MLX Metal GPU");
        Device::Gpu(0)
    }
}

fn f32_to_pcm16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|sample| (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16)
        .collect()
}
fn transcript_result(result: qwen3_asr_rs::inference::TranscribeResult) -> TranscriptResult {
    TranscriptResult {
        language: result.language,
        text: result.text,
        raw_output: result.raw_output,
        kv_cache_bytes: result.kv_cache_bytes,
        kv_cache_dense_bytes: result.kv_cache_dense_bytes,
    }
}
fn step_transcript(state: &StreamingState, snapshot: StreamingTranscript) -> StepTranscript {
    StepTranscript {
        language: snapshot.language,
        fixed_text: snapshot.fixed_text,
        draft_text: snapshot.draft_text,
        text: snapshot.text,
        kv_cache_bytes: state.kv_cache_bytes(),
        kv_cache_dense_bytes: state.kv_cache_dense_bytes(),
    }
}

fn maybe_normalize(value: &str, ignore_whitespace: bool) -> String {
    if !ignore_whitespace {
        return value.to_string();
    }
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn kv_reduction_ratio(dense_bytes: usize, compressed_bytes: usize) -> f64 {
    if dense_bytes == 0 {
        return 0.0;
    }
    (dense_bytes.saturating_sub(compressed_bytes)) as f64 / dense_bytes as f64
}

fn first_divergence(
    chunk_id: usize,
    audio_samples: usize,
    dense: &StepTranscript,
    compressed: &StepTranscript,
    ignore_whitespace: bool,
) -> Option<Divergence> {
    for (field, left, right) in [
        (
            "language",
            dense.language.as_str(),
            compressed.language.as_str(),
        ),
        (
            "fixed_text",
            dense.fixed_text.as_str(),
            compressed.fixed_text.as_str(),
        ),
        (
            "draft_text",
            dense.draft_text.as_str(),
            compressed.draft_text.as_str(),
        ),
        ("text", dense.text.as_str(), compressed.text.as_str()),
    ] {
        let left = maybe_normalize(left, ignore_whitespace);
        let right = maybe_normalize(right, ignore_whitespace);
        if left != right {
            return Some(Divergence {
                chunk_id,
                audio_samples,
                field,
                dense: left,
                compressed: right,
            });
        }
    }
    None
}

fn worst_streaming_divergence(
    streaming_steps: &[StepPair],
    ignore_whitespace: bool,
) -> Option<(usize, usize, usize)> {
    let mut worst = None;
    for step in streaming_steps {
        let mut field_count = 0usize;
        for (left, right) in [
            (step.dense.language.as_str(), step.compressed.language.as_str()),
            (
                step.dense.fixed_text.as_str(),
                step.compressed.fixed_text.as_str(),
            ),
            (
                step.dense.draft_text.as_str(),
                step.compressed.draft_text.as_str(),
            ),
            (step.dense.text.as_str(), step.compressed.text.as_str()),
        ] {
            if maybe_normalize(left, ignore_whitespace) != maybe_normalize(right, ignore_whitespace)
            {
                field_count += 1;
            }
        }
        if field_count == 0 {
            continue;
        }
        let replace = match worst {
            Some((_, _, best)) => field_count > best,
            None => true,
        };
        if replace {
            worst = Some((step.chunk_id, step.audio_samples, field_count));
        }
    }
    worst
}

fn offline_divergence(
    dense: &TranscriptResult,
    compressed: &TranscriptResult,
    audio_samples: usize,
    ignore_whitespace: bool,
) -> Option<Divergence> {
    for (field, left, right) in [
        (
            "language",
            dense.language.as_str(),
            compressed.language.as_str(),
        ),
        ("text", dense.text.as_str(), compressed.text.as_str()),
        (
            "raw_output",
            dense.raw_output.as_str(),
            compressed.raw_output.as_str(),
        ),
    ] {
        let left = maybe_normalize(left, ignore_whitespace);
        let right = maybe_normalize(right, ignore_whitespace);
        if left != right {
            return Some(Divergence {
                chunk_id: 0,
                audio_samples,
                field,
                dense: left,
                compressed: right,
            });
        }
    }
    None
}

fn main() -> Result<()> {
    if let Err(err) = maybe_reexec_with_cuda_preload() {
        tracing::warn!("Failed to re-exec with Linux CUDA preload: {err}");
    }
    preload_cuda_runtime();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let model_dir = Path::new(&args.model_path);
    if !model_dir.exists() {
        anyhow::bail!("Model directory not found: {}", args.model_path);
    }
    if !Path::new(&args.audio_file).exists() {
        anyhow::bail!("Audio file not found: {}", args.audio_file);
    }

    let samples = audio::load_audio(&args.audio_file, SAMPLE_RATE as u32)
        .with_context(|| format!("Failed to load audio: {}", args.audio_file))?;
    let pcm16 = f32_to_pcm16(&samples);
    let device = select_device();

    let dense_model = AsrInference::load_with_kv_mode(model_dir, device, KvCacheMode::DenseAppend)
        .context("Failed to load dense_append model")?;
    let compressed_model =
        AsrInference::load_with_kv_mode(model_dir, device, KvCacheMode::ExperimentalTurboQuant)
            .context("Failed to load compressed kv model")?;

    let language = args.language.as_deref();
    eprintln!("[parity] dense offline start");
    let offline_dense = transcript_result(
        dense_model.transcribe_pcm16(&pcm16, language).context("Dense offline transcription failed")?,
    );
    eprintln!("[parity] turbo offline start");
    let offline_compressed = transcript_result(
        compressed_model.transcribe_pcm16(&pcm16, language).context("TurboQuant offline transcription failed")?,
    );
    let offline_divergence =
        offline_divergence(&offline_dense, &offline_compressed, pcm16.len(), args.ignore_whitespace);

    eprintln!("[parity] streaming start");
    let config = StreamingConfig { chunk_size_ms: args.chunk_size_ms, unfixed_chunk_num: args.unfixed_chunk_num, unfixed_token_num: args.unfixed_token_num };
    let chunk_samples = ((config.chunk_size_ms as usize) * SAMPLE_RATE / 1000).max(1);
    let mut dense_state = StreamingState::new(config.clone());
    let mut compressed_state = StreamingState::new(config);
    let mut streaming_steps = Vec::new();
    let mut divergence = None;
    let mut last_divergence = None;
    let mut streaming_divergence_count = 0usize;
    let mut dense_peak_kv_bytes = 0usize;
    let mut compressed_peak_kv_bytes = 0usize;
    let mut language_divergence_count = 0usize;
    let mut fixed_text_divergence_count = 0usize;
    let mut draft_text_divergence_count = 0usize;
    let mut text_divergence_count = 0usize;

    for (chunk_id, chunk) in pcm16.chunks(chunk_samples).enumerate() {
        dense_state.append_pcm16(chunk);
        compressed_state.append_pcm16(chunk);

        let dense_snapshot = if chunk.len() == chunk_samples {
            dense_state.transcribe(&dense_model, language)?
        } else {
            dense_state.finish(&dense_model, language)?
        };
        let compressed_snapshot = if chunk.len() == chunk_samples {
            compressed_state.transcribe(&compressed_model, language)?
        } else {
            compressed_state.finish(&compressed_model, language)?
        };

        let audio_samples = ((chunk_id + 1) * chunk_samples).min(pcm16.len());
        let dense_step = step_transcript(&dense_state, dense_snapshot);
        let compressed_step = step_transcript(&compressed_state, compressed_snapshot);
        dense_peak_kv_bytes = dense_peak_kv_bytes.max(dense_step.kv_cache_bytes);
        compressed_peak_kv_bytes = compressed_peak_kv_bytes.max(compressed_step.kv_cache_bytes);
        let step_divergence = first_divergence(
            chunk_id,
            audio_samples,
            &dense_step,
            &compressed_step,
            args.ignore_whitespace,
        );
        if divergence.is_none() {
            divergence = step_divergence.as_ref().map(|value| Divergence {
                chunk_id: value.chunk_id,
                audio_samples: value.audio_samples,
                field: value.field,
                dense: value.dense.clone(),
                compressed: value.compressed.clone(),
            });
        }
        if step_divergence.is_some() {
            streaming_divergence_count += 1;
        }
        for (count, left, right) in [
            (
                &mut language_divergence_count,
                dense_step.language.as_str(),
                compressed_step.language.as_str(),
            ),
            (
                &mut fixed_text_divergence_count,
                dense_step.fixed_text.as_str(),
                compressed_step.fixed_text.as_str(),
            ),
            (
                &mut draft_text_divergence_count,
                dense_step.draft_text.as_str(),
                compressed_step.draft_text.as_str(),
            ),
            (&mut text_divergence_count, dense_step.text.as_str(), compressed_step.text.as_str()),
        ] {
            if maybe_normalize(left, args.ignore_whitespace)
                != maybe_normalize(right, args.ignore_whitespace)
            {
                *count += 1;
            }
        }
        last_divergence = step_divergence.as_ref().map(|value| Divergence {
            chunk_id: value.chunk_id,
            audio_samples: value.audio_samples,
            field: value.field,
            dense: value.dense.clone(),
            compressed: value.compressed.clone(),
        });
        streaming_steps.push(StepPair {
            chunk_id,
            audio_samples,
            dense: dense_step,
            compressed: compressed_step,
        });
    }

    eprintln!("[parity] finalization start");
    let final_dense_result = dense_state.finish(&dense_model, language)?;
    let final_dense = step_transcript(&dense_state, final_dense_result);
    let final_compressed_result = compressed_state.finish(&compressed_model, language)?;
    let final_compressed = step_transcript(&compressed_state, final_compressed_result);
    let streaming_step_count = streaming_steps.len();
    let final_divergence = first_divergence(
        streaming_step_count,
        pcm16.len(),
        &final_dense,
        &final_compressed,
        args.ignore_whitespace,
    );
    let final_kv_reduction_ratio =
        kv_reduction_ratio(final_dense.kv_cache_bytes, final_compressed.kv_cache_bytes);
    let peak_kv_reduction_ratio = kv_reduction_ratio(dense_peak_kv_bytes, compressed_peak_kv_bytes);
    let min_streaming_kv_reduction_ratio = streaming_steps.iter().map(|step| kv_reduction_ratio(step.dense.kv_cache_bytes, step.compressed.kv_cache_bytes)).fold(1.0, f64::min);
    let first_divergence_chunk_id = divergence
        .as_ref()
        .map(|value| value.chunk_id)
        .or_else(|| final_divergence.as_ref().map(|value| value.chunk_id));
    let first_divergence_audio_samples = divergence
        .as_ref()
        .map(|value| value.audio_samples)
        .or_else(|| final_divergence.as_ref().map(|value| value.audio_samples));
    let streaming_match_count = streaming_steps
        .len()
        .saturating_sub(streaming_divergence_count);
    let streaming_match_ratio = if streaming_steps.is_empty() {
        1.0
    } else {
        streaming_match_count as f64 / streaming_steps.len() as f64
    };
    let worst_streaming_divergence =
        worst_streaming_divergence(&streaming_steps, args.ignore_whitespace);
    let (
        worst_streaming_divergence_chunk_id,
        worst_streaming_divergence_audio_samples,
        worst_streaming_divergence_field_count,
    ) = worst_streaming_divergence
        .map(|value| (Some(value.0), Some(value.1), value.2))
        .unwrap_or((None, None, 0));
    let summary = ParitySummary {
        offline_match: offline_divergence.is_none(),
        final_match: final_divergence.is_none(),
        streaming_match: divergence.is_none(),
        streaming_match_count,
        streaming_match_ratio,
        streaming_divergence_count,
        language_divergence_count,
        fixed_text_divergence_count,
        draft_text_divergence_count,
        text_divergence_count,
        first_divergence_chunk_id,
        first_divergence_audio_samples,
        worst_streaming_divergence_chunk_id,
        worst_streaming_divergence_audio_samples,
        worst_streaming_divergence_field_count,
        dense_final_kv_bytes: final_dense.kv_cache_bytes,
        compressed_final_kv_bytes: final_compressed.kv_cache_bytes,
        final_kv_reduction_ratio,
        dense_peak_kv_bytes,
        compressed_peak_kv_bytes: compressed_peak_kv_bytes,
        peak_kv_reduction_ratio,
        min_streaming_kv_reduction_ratio,
    };

    let report = ParityReport {
        audio_samples: pcm16.len(),
        chunk_samples,
        summary,
        offline_dense,
        offline_compressed,
        streaming_steps,
        final_dense,
        final_compressed,
        offline_divergence,
        first_divergence: divergence.or(final_divergence),
    };
    let status = if report.summary.offline_match
        && report.summary.final_match
        && report.summary.streaming_divergence_count <= args.max_streaming_divergences && report.summary.worst_streaming_divergence_field_count <= args.max_worst_streaming_divergence_field_count
        && report.summary.streaming_match_ratio >= args.min_streaming_match_ratio
        && report.summary.language_divergence_count <= args.max_language_divergences
        && report.summary.fixed_text_divergence_count <= args.max_fixed_text_divergences
        && report.summary.draft_text_divergence_count <= args.max_draft_text_divergences
        && report.summary.text_divergence_count <= args.max_text_divergences
        && report.streaming_steps.iter().all(|step| step.compressed.kv_cache_bytes < step.dense.kv_cache_bytes)
        && report.summary.final_kv_reduction_ratio >= args.min_final_kv_reduction_ratio
        && report.summary.peak_kv_reduction_ratio >= args.min_peak_kv_reduction_ratio
    {
        "PASS"
    } else {
        "FAIL"
    };
    eprintln!(
        "kv_parity status={} offline={} final={} streaming={} match={}/{}({:.2}%) first={:?}@{:?} worst={:?}@{:?} last={:?}@{:?} fields={} kv_final={}/{} kv_peak={}/{} reduction_final={:.2}% reduction_peak={:.2}% reduction_step_min={:.2}% divergences[lang={},fixed={},draft={},text={}]",
        status,
        report.summary.offline_match,
        report.summary.final_match,
        report.summary.streaming_match,
        streaming_match_count,
        streaming_step_count,
        streaming_match_ratio * 100.0,
        report.summary.first_divergence_chunk_id,
        report.summary.first_divergence_audio_samples,
        report.summary.worst_streaming_divergence_chunk_id,
        report.summary.worst_streaming_divergence_audio_samples,
        last_divergence.as_ref().map(|value| value.chunk_id),
        last_divergence.as_ref().map(|value| value.audio_samples),
        report.summary.worst_streaming_divergence_field_count,
        report.summary.dense_final_kv_bytes,
        report.summary.compressed_final_kv_bytes,
        report.summary.dense_peak_kv_bytes,
        report.summary.compressed_peak_kv_bytes,
        report.summary.final_kv_reduction_ratio * 100.0,
        report.summary.peak_kv_reduction_ratio * 100.0,
        report.summary.min_streaming_kv_reduction_ratio * 100.0,
        report.summary.language_divergence_count,
        report.summary.fixed_text_divergence_count,
        report.summary.draft_text_divergence_count,
        report.summary.text_divergence_count,
    );
    println!("{}", serde_json::to_string_pretty(&report)?);
    if args.fail_on_divergence {
        let parity_failed = !report.summary.offline_match
            || !report.summary.final_match
            || report.summary.streaming_divergence_count > args.max_streaming_divergences || report.summary.worst_streaming_divergence_field_count > args.max_worst_streaming_divergence_field_count
            || report.summary.streaming_match_ratio < args.min_streaming_match_ratio
            || report.summary.language_divergence_count > args.max_language_divergences
            || report.summary.fixed_text_divergence_count > args.max_fixed_text_divergences
            || report.summary.draft_text_divergence_count > args.max_draft_text_divergences
            || report.summary.text_divergence_count > args.max_text_divergences
            || report.streaming_steps.iter().any(|step| step.compressed.kv_cache_bytes >= step.dense.kv_cache_bytes);
        let reduction_failed = report.summary.final_kv_reduction_ratio
            < args.min_final_kv_reduction_ratio
            || report.summary.peak_kv_reduction_ratio < args.min_peak_kv_reduction_ratio;
        if parity_failed || reduction_failed {
            std::process::exit(2);
        }
    }
    Ok(())
}
