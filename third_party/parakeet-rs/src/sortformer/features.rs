use super::{Sortformer, HOP_LENGTH, LOG_ZERO_GUARD, N_FFT, N_MELS, PREEMPH, WIN_LENGTH};
use ndarray::{Array2, Array3};
use rustfft::{num_complex::Complex, FftPlanner};
use std::f32::consts::PI;

impl Sortformer {
    fn apply_preemphasis(audio: &[f32]) -> Vec<f32> {
        let mut result = Vec::with_capacity(audio.len());
        result.push(audio[0]);
        for i in 1..audio.len() {
            result.push(audio[i] - PREEMPH * audio[i - 1]);
        }
        result
    }

    fn hann_window(window_length: usize) -> Vec<f32> {
        // Librosa uses periodic window (fftbins=True): divide by N, not N-1
        (0..window_length)
            .map(|i| 0.5 - 0.5 * ((2.0 * PI * i as f32) / window_length as f32).cos())
            .collect()
    }

    fn stft(audio: &[f32]) -> Array2<f32> {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(N_FFT);

        // Create Hann window of length win_length, then zero-pad to n_fft (centered)
        // This is exactly what librosa does: util.pad_center(fft_window, size=n_fft)
        let hann = Self::hann_window(WIN_LENGTH);
        let win_offset = (N_FFT - WIN_LENGTH) / 2;
        let mut fft_window = vec![0.0f32; N_FFT];
        fft_window[win_offset..(WIN_LENGTH + win_offset)].copy_from_slice(&hann[..WIN_LENGTH]);

        // Pad signal for center=True (like librosa/torch.stft)
        // Padding is n_fft // 2 on each side
        let pad_amount = N_FFT / 2;
        let mut padded_audio = vec![0.0; pad_amount];
        padded_audio.extend_from_slice(audio);
        padded_audio.extend(vec![0.0; pad_amount]);

        let num_frames = (padded_audio.len() - N_FFT) / HOP_LENGTH + 1;
        let freq_bins = N_FFT / 2 + 1;
        let mut spectrogram = Array2::<f32>::zeros((freq_bins, num_frames));

        for frame_idx in 0..num_frames {
            let start = frame_idx * HOP_LENGTH;
            let mut frame: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); N_FFT];

            // Extract n_fft samples and multiply by zero-padded window
            for i in 0..N_FFT {
                if start + i < padded_audio.len() {
                    frame[i] = Complex::new(padded_audio[start + i] * fft_window[i], 0.0);
                }
            }

            fft.process(&mut frame);
            for k in 0..freq_bins {
                let magnitude = frame[k].norm();
                // Power spectrum (magnitude^2) - NeMo uses mag_power=2.0
                spectrogram[[k, frame_idx]] = magnitude * magnitude;
            }
        }

        spectrogram
    }

    pub(super) fn extract_mel_features(&self, audio: &[f32]) -> Array3<f32> {
        // 1. Add dither (small random noise to prevent log(0))
        // NeMo uses dither=1e-5, but for determinism we skip random noise
        // The log_zero_guard handles zero values

        // 2. Apply preemphasis (NeMo uses preemph=0.97)
        let preemphasized = Self::apply_preemphasis(audio);

        // 3. STFT
        let spectrogram = Self::stft(&preemphasized);

        // 4. Apply mel filterbank (with Slaney normalization)
        let mel_spec = self.mel_basis.dot(&spectrogram);

        // 5. Log with guard value (NeMo uses log_zero_guard_value = 2^-24)
        // NeMo uses normalize='NA' which means NO normalization
        let log_mel_spec = mel_spec.mapv(|x| (x + LOG_ZERO_GUARD).ln());

        let num_frames = log_mel_spec.shape()[1];
        let mut features = Array3::<f32>::zeros((1, num_frames, N_MELS));

        // Transpose to (batch, time, features) - NeMo outputs (B, D, T), model expects (B, T, D)
        for t in 0..num_frames {
            for m in 0..N_MELS {
                features[[0, t, m]] = log_mel_spec[[m, t]];
            }
        }

        features
    }
}
