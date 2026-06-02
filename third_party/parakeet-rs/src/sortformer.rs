//! NVIDIA Sortformer v2 Streaming Speaker Diarization
//!
//! This module implements NVIDIA's Sortformer v2 streaming model for speaker diarization.
//!
//! Key features:
//! - Streaming inference with ~10s chunks (124 frames at 80ms each)
//! - FIFO buffer for context management
//! - Smart speaker cache compression (keeps important frames, not just recent)
//! - Silence profile tracking
//! - Post-processing: median filtering, hysteresis thresholding
//! - Supports up to 4 speakers
//!
//! Reference: https://huggingface.co/nvidia/diar_streaming_sortformer_4spk-v2
//! Note that, my ONNX export:
//! CHUNK_LEN = 124
//! FIFO_LEN = 124
//! CACHE_LEN = 188
//! FEAT_DIM = 128
//! EMB_DIM = 512
//! Note, my stft code is adapted from: https://librosa.org/doc/main/generated/librosa.stft.html

use crate::error::{Error, Result};
use crate::execution::ModelConfig;
use ndarray::{s, Array1, Array2, Array3, Axis};
use ort::session::Session;
use std::path::Path;

mod cache;
mod features;
mod postprocess;

// Model constants
const N_FFT: usize = 512;
const WIN_LENGTH: usize = 400;
const HOP_LENGTH: usize = 160;
const N_MELS: usize = 128;
const PREEMPH: f32 = 0.97;
const LOG_ZERO_GUARD: f32 = 5.960_464_5e-8;
const SAMPLE_RATE: usize = 16000;

// Streaming constants
const CHUNK_LEN: usize = 124; // Frames per chunk (~10s at 80ms)
const FIFO_LEN: usize = 124; // FIFO buffer length
const SPKCACHE_LEN: usize = 188; // Speaker cache length
const SPKCACHE_UPDATE_PERIOD: usize = 124;
const SUBSAMPLING: usize = 8; // Audio frames -> model frames
const EMB_DIM: usize = 512; // Embedding dimension
const NUM_SPEAKERS: usize = 4; // Model supports 4 speakers
const FRAME_DURATION: f32 = 0.08; // 80ms per frame

// Cache compression params (from NeMo)
const SPKCACHE_SIL_FRAMES_PER_SPK: usize = 3;
const PRED_SCORE_THRESHOLD: f32 = 0.25;
const STRONG_BOOST_RATE: f32 = 0.75;
const WEAK_BOOST_RATE: f32 = 1.5;
const MIN_POS_SCORES_RATE: f32 = 0.5;
const SIL_THRESHOLD: f32 = 0.2;
const MAX_INDEX: usize = 99999;

/// Post-processing configuration for speaker diarization. (NVIDIA official configs from v2 YAMLs)
///
/// Controls how raw model predictions are converted into speaker segments.
/// NVIDIA provides pre-tuned configs for different datasets (CallHome, DIHARD3, AMI).
///
/// # Parameters
/// - `onset`: Probability threshold to START a speaker segment (higher = more strict)
/// - `offset`: Probability threshold to END a speaker segment (lower = longer segments)
/// - `pad_onset`: Seconds to subtract from segment start times
/// - `pad_offset`: Seconds to add to segment end times
/// - `min_duration_on`: Minimum segment length in seconds (filters short blips)
/// - `min_duration_off`: Minimum gap between segments before merging
/// - `median_window`: Smoothing window size (odd number, higher = smoother)
///
/// # Pre-tuned Configs
/// - `callhome()` - (default)
/// - `dihard3()`
///
/// # Custom Config
/// Use `custom(onset, offset)` to create your own config for fine-tuning.
///
/// See: https://github.com/NVIDIA-NeMo/NeMo/tree/main/examples/speaker_tasks/diarization/conf/neural_diarizer
#[derive(Debug, Clone)]
pub struct DiarizationConfig {
    pub onset: f32,
    pub offset: f32,
    pub pad_onset: f32,
    pub pad_offset: f32,
    pub min_duration_on: f32,
    pub min_duration_off: f32,
    pub median_window: usize,
}

impl Default for DiarizationConfig {
    fn default() -> Self {
        Self::callhome()
    }
}

impl DiarizationConfig {
    /// CallHome dataset config for v2 (default)
    /// From: diar_streaming_sortformer_4spk-v2_callhome-part1.yaml
    pub fn callhome() -> Self {
        Self {
            onset: 0.641,
            offset: 0.561,
            pad_onset: 0.229,
            pad_offset: 0.079,
            min_duration_on: 0.511,
            min_duration_off: 0.296,
            median_window: 11,
        }
    }

    /// DIHARD3 dataset config for v2
    /// From: diar_streaming_sortformer_4spk-v2_dihard3-dev.yaml
    pub fn dihard3() -> Self {
        Self {
            onset: 0.56,
            offset: 1.0,
            pad_onset: 0.063,
            pad_offset: 0.002,
            min_duration_on: 0.007,
            min_duration_off: 0.151,
            median_window: 11,
        }
    }

    /// Create a custom config for fine-tuning diarization behavior.
    ///
    /// # Arguments
    /// * `onset` - Probability threshold to start a segment (0.0-1.0, typical: 0.5-0.7)
    /// * `offset` - Probability threshold to end a segment (0.0-1.0, typical: 0.4-0.6)
    ///
    /// # Example
    /// ```rust
    /// use parakeet_rs::sortformer::DiarizationConfig;
    ///
    /// // More sensitive detection (lower thresholds)
    /// let sensitive = DiarizationConfig::custom(0.5, 0.4);
    ///
    /// // Stricter detection (higher thresholds, fewer false positives)
    /// let strict = DiarizationConfig::custom(0.7, 0.6);
    ///
    /// // Full customization
    /// let mut config = DiarizationConfig::custom(0.6, 0.5);
    /// config.min_duration_on = 0.3;  // Ignore segments shorter than 300ms
    /// config.median_window = 15;      // More smoothing
    /// ```
    pub fn custom(onset: f32, offset: f32) -> Self {
        Self {
            onset,
            offset,
            pad_onset: 0.0,
            pad_offset: 0.0,
            min_duration_on: 0.1,
            min_duration_off: 0.1,
            median_window: 11,
        }
    }
}

/// Speaker segment with start time, end time, and speaker ID
#[derive(Debug, Clone)]
pub struct SpeakerSegment {
    pub start: f32,
    pub end: f32,
    pub speaker_id: usize,
}

/// Streaming Sortformer v2 speaker diarization engine
pub struct Sortformer {
    session: Session,
    config: DiarizationConfig,
    // Streaming state. note that, Same way as Nemo
    spkcache: Array3<f32>,               // (1, 0..SPKCACHE_LEN, EMB_DIM)
    spkcache_preds: Option<Array3<f32>>, // (1, 0..SPKCACHE_LEN, NUM_SPEAKERS)
    fifo: Array3<f32>,                   // (1, 0..FIFO_LEN, EMB_DIM)
    fifo_preds: Array3<f32>,             // (1, 0..FIFO_LEN, NUM_SPEAKERS)
    mean_sil_emb: Array2<f32>,           // (1, EMB_DIM)
    n_sil_frames: usize,
    // Mel filterbank (cached)
    mel_basis: Array2<f32>,
}

impl Sortformer {
    /// a new Sortformer instance from ONNX model path
    pub fn new<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        Self::with_config(model_path, None, DiarizationConfig::default())
    }

    /// Create with custom config
    pub fn with_config<P: AsRef<Path>>(
        model_path: P,
        execution_config: Option<ModelConfig>,
        config: DiarizationConfig,
    ) -> Result<Self> {
        let config_to_use = execution_config.unwrap_or_default();

        let session = config_to_use
            .apply_to_session_builder(Session::builder()?)?
            .commit_from_file(model_path.as_ref())?;

        let mel_basis = crate::audio::create_mel_filterbank(N_FFT, N_MELS, SAMPLE_RATE);

        let mut instance = Self {
            session,
            config,
            spkcache: Array3::zeros((1, 0, EMB_DIM)),
            spkcache_preds: None,
            fifo: Array3::zeros((1, 0, EMB_DIM)),
            fifo_preds: Array3::zeros((1, 0, NUM_SPEAKERS)),
            mean_sil_emb: Array2::zeros((1, EMB_DIM)),
            n_sil_frames: 0,
            mel_basis,
        };
        instance.reset_state();
        Ok(instance)
    }

    /// Reset streaming state
    pub fn reset_state(&mut self) {
        self.spkcache = Array3::zeros((1, 0, EMB_DIM));
        self.spkcache_preds = None;
        self.fifo = Array3::zeros((1, 0, EMB_DIM));
        self.fifo_preds = Array3::zeros((1, 0, NUM_SPEAKERS));
        self.mean_sil_emb = Array2::zeros((1, EMB_DIM));
        self.n_sil_frames = 0;
    }

    /// Main diarization entry point
    pub fn diarize(
        &mut self,
        mut audio: Vec<f32>,
        sample_rate: u32,
        channels: u16,
    ) -> Result<Vec<SpeakerSegment>> {
        // Resample if needed
        if sample_rate != SAMPLE_RATE as u32 {
            return Err(Error::Audio(format!(
                "Expected {} Hz, got {} Hz",
                SAMPLE_RATE, sample_rate
            )));
        }

        if audio.is_empty() {
            return Ok(vec![]);
        }

        // Convert to mono
        if channels > 1 {
            audio = audio
                .chunks(channels as usize)
                .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                .collect();
        }

        // Reset state for new audio
        self.reset_state();

        // Extract mel features (B, T, D)
        let features = self.extract_mel_features(&audio);
        let total_frames = features.shape()[1];

        // Process in chunks
        let chunk_stride = CHUNK_LEN * SUBSAMPLING;
        let num_chunks = total_frames.div_ceil(chunk_stride);

        let mut all_chunk_preds = Vec::new();

        for chunk_idx in 0..num_chunks {
            let start = chunk_idx * chunk_stride;
            let end = (start + chunk_stride).min(total_frames);
            let current_len = end - start;

            // Extract chunk features
            let mut chunk_feat = features.slice(s![.., start..end, ..]).to_owned();

            // Pad last chunk if needed
            if current_len < chunk_stride {
                let mut padded = Array3::zeros((1, chunk_stride, N_MELS));
                padded
                    .slice_mut(s![.., ..current_len, ..])
                    .assign(&chunk_feat);
                chunk_feat = padded;
            }

            // Run streaming update
            let chunk_preds = self.streaming_update(&chunk_feat, current_len)?;
            all_chunk_preds.push(chunk_preds);
        }

        // Concatenate all predictions
        let full_preds = Self::concat_predictions(&all_chunk_preds);

        // Apply median filtering
        let filtered_preds = if self.config.median_window > 1 {
            self.median_filter(&full_preds)
        } else {
            full_preds
        };

        // Binarize to segments
        let segments = self.binarize(&filtered_preds);

        Ok(segments)
    }

    /// Streaming diarization: process one audio chunk without resetting state.
    ///
    /// Unlike `diarize()`, this method preserves internal state (FIFO, speaker cache,
    /// silence profile) across calls, enabling true streaming diarization.
    ///
    /// # Arguments
    /// * `audio_16k_mono` - Audio chunk at 16kHz mono (any length, typically 2-30s)
    ///
    /// # Returns
    /// Speaker segments with timestamps relative to this chunk (starting at 0.0)
    pub fn diarize_chunk(&mut self, audio_16k_mono: &[f32]) -> Result<Vec<SpeakerSegment>> {
        if audio_16k_mono.is_empty() {
            return Ok(vec![]);
        }

        let features = self.extract_mel_features(audio_16k_mono);
        let total_frames = features.shape()[1];

        let chunk_stride = CHUNK_LEN * SUBSAMPLING;
        let num_chunks = total_frames.div_ceil(chunk_stride);

        let mut all_chunk_preds = Vec::new();

        for chunk_idx in 0..num_chunks {
            let start = chunk_idx * chunk_stride;
            let end = (start + chunk_stride).min(total_frames);
            let current_len = end - start;

            let mut chunk_feat = features.slice(s![.., start..end, ..]).to_owned();

            if current_len < chunk_stride {
                let mut padded = Array3::zeros((1, chunk_stride, N_MELS));
                padded
                    .slice_mut(s![.., ..current_len, ..])
                    .assign(&chunk_feat);
                chunk_feat = padded;
            }

            let chunk_preds = self.streaming_update(&chunk_feat, current_len)?;
            all_chunk_preds.push(chunk_preds);
        }

        let full_preds = Self::concat_predictions(&all_chunk_preds);

        let filtered_preds = if self.config.median_window > 1 {
            self.median_filter(&full_preds)
        } else {
            full_preds
        };

        let segments = self.binarize(&filtered_preds);

        Ok(segments)
    }

    /// NeMo's streaming_update with smart cache compression
    fn streaming_update(
        &mut self,
        chunk_feat: &Array3<f32>,
        current_len: usize,
    ) -> Result<Array2<f32>> {
        let spkcache_len = self.spkcache.shape()[1];
        let fifo_len = self.fifo.shape()[1];

        // Prepare inputs
        let chunk_lengths = Array1::from_vec(vec![current_len as i64]);
        let spkcache_lengths = Array1::from_vec(vec![spkcache_len as i64]);
        let fifo_lengths = Array1::from_vec(vec![fifo_len as i64]);

        // Prepare FIFO input
        let fifo_input = if fifo_len > 0 {
            self.fifo.clone()
        } else {
            Array3::zeros((1, 0, EMB_DIM))
        };

        // Prepare spkcache input (may be empty)
        let spkcache_input = if spkcache_len > 0 {
            self.spkcache.clone()
        } else {
            Array3::zeros((1, 0, EMB_DIM))
        };

        // Create input values
        let chunk_value = ort::value::Value::from_array(chunk_feat.clone())?;
        let chunk_lengths_value = ort::value::Value::from_array(chunk_lengths)?;
        let spkcache_value = ort::value::Value::from_array(spkcache_input)?;
        let spkcache_lengths_value = ort::value::Value::from_array(spkcache_lengths)?;
        let fifo_value = ort::value::Value::from_array(fifo_input)?;
        let fifo_lengths_value = ort::value::Value::from_array(fifo_lengths)?;

        // Run ONNX inference and extract all data in a block to release borrow
        let (preds, new_embs, chunk_len) = {
            let outputs = self.session.run(ort::inputs!(
                "chunk" => chunk_value,
                "chunk_lengths" => chunk_lengths_value,
                "spkcache" => spkcache_value,
                "spkcache_lengths" => spkcache_lengths_value,
                "fifo" => fifo_value,
                "fifo_lengths" => fifo_lengths_value
            ))?;

            // Extract outputs
            let (preds_shape, preds_data) = outputs["spkcache_fifo_chunk_preds"]
                .try_extract_tensor::<f32>()
                .map_err(|e| Error::Model(format!("Failed to extract preds: {e}")))?;
            let (embs_shape, embs_data) = outputs["chunk_pre_encode_embs"]
                .try_extract_tensor::<f32>()
                .map_err(|e| Error::Model(format!("Failed to extract embs: {e}")))?;

            // Convert to ndarray
            let preds_dims = preds_shape.as_ref();
            let embs_dims = embs_shape.as_ref();

            let preds = Array3::from_shape_vec(
                (
                    preds_dims[0] as usize,
                    preds_dims[1] as usize,
                    preds_dims[2] as usize,
                ),
                preds_data.to_vec(),
            )
            .map_err(|e| Error::Model(format!("Failed to reshape preds: {e}")))?;

            let new_embs = Array3::from_shape_vec(
                (
                    embs_dims[0] as usize,
                    embs_dims[1] as usize,
                    embs_dims[2] as usize,
                ),
                embs_data.to_vec(),
            )
            .map_err(|e| Error::Model(format!("Failed to reshape embs: {e}")))?;

            // Calculate valid frames
            let valid_frames = current_len.div_ceil(SUBSAMPLING);

            (preds, new_embs, valid_frames)
        };

        // Extract predictions for different parts
        let fifo_preds = if fifo_len > 0 {
            preds
                .slice(s![0, spkcache_len..spkcache_len + fifo_len, ..])
                .to_owned()
        } else {
            Array2::zeros((0, NUM_SPEAKERS))
        };

        let chunk_preds = preds
            .slice(s![
                0,
                spkcache_len + fifo_len..spkcache_len + fifo_len + chunk_len,
                ..
            ])
            .to_owned();
        let chunk_embs = new_embs.slice(s![0, ..chunk_len, ..]).to_owned();

        // Append chunk embeddings to FIFO
        self.fifo = Self::concat_axis1(&self.fifo, &chunk_embs.insert_axis(Axis(0)));

        // Update FIFO predictions
        if fifo_len > 0 {
            let combined = Self::concat_axis1_2d(&fifo_preds, &chunk_preds);
            self.fifo_preds = combined.insert_axis(Axis(0));
        } else {
            self.fifo_preds = chunk_preds.clone().insert_axis(Axis(0));
        }

        let fifo_len_after = self.fifo.shape()[1];

        // Move from FIFO to cache when FIFO exceeds limit
        if fifo_len_after > FIFO_LEN {
            let mut pop_out_len = SPKCACHE_UPDATE_PERIOD;
            pop_out_len = pop_out_len.max(chunk_len.saturating_sub(FIFO_LEN) + fifo_len);
            pop_out_len = pop_out_len.min(fifo_len_after);

            let pop_out_embs = self.fifo.slice(s![.., ..pop_out_len, ..]).to_owned();
            let pop_out_preds = self.fifo_preds.slice(s![.., ..pop_out_len, ..]).to_owned();

            // Update silence profile
            self.update_silence_profile(&pop_out_embs, &pop_out_preds);

            // Remove from FIFO
            self.fifo = self.fifo.slice(s![.., pop_out_len.., ..]).to_owned();
            self.fifo_preds = self.fifo_preds.slice(s![.., pop_out_len.., ..]).to_owned();

            // Append to cache
            self.spkcache = Self::concat_axis1(&self.spkcache, &pop_out_embs);

            if let Some(ref cache_preds) = self.spkcache_preds {
                self.spkcache_preds = Some(Self::concat_axis1(cache_preds, &pop_out_preds));
            }

            // Smart compression when cache exceeds limit
            if self.spkcache.shape()[1] > SPKCACHE_LEN {
                if self.spkcache_preds.is_none() {
                    // Initialize cache predictions from initial output
                    let initial_cache_preds = preds.slice(s![.., ..spkcache_len, ..]).to_owned();
                    let combined = Self::concat_axis1(&initial_cache_preds, &pop_out_preds);
                    self.spkcache_preds = Some(combined);
                }

                // Use smart compression
                self.compress_spkcache();
            }
        }

        Ok(chunk_preds)
    }

}
