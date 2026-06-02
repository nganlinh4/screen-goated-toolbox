use super::{
    Sortformer, EMB_DIM, MAX_INDEX, MIN_POS_SCORES_RATE, NUM_SPEAKERS, PRED_SCORE_THRESHOLD,
    SIL_THRESHOLD, SPKCACHE_LEN, SPKCACHE_SIL_FRAMES_PER_SPK, STRONG_BOOST_RATE, WEAK_BOOST_RATE,
};
use ndarray::{s, Array2, Array3};

impl Sortformer {
    /// Update mean silence embedding
    pub(super) fn update_silence_profile(&mut self, embs: &Array3<f32>, preds: &Array3<f32>) {
        let preds_2d = preds.slice(s![0, .., ..]);

        for t in 0..preds_2d.shape()[0] {
            let sum: f32 = (0..NUM_SPEAKERS).map(|s| preds_2d[[t, s]]).sum();
            if sum < SIL_THRESHOLD {
                // This is a silence frame
                let emb = embs.slice(s![0, t, ..]);

                // Update running mean
                let old_sum: Vec<f32> = self
                    .mean_sil_emb
                    .slice(s![0, ..])
                    .iter()
                    .map(|&x| x * self.n_sil_frames as f32)
                    .collect();

                self.n_sil_frames += 1;

                for i in 0..EMB_DIM {
                    self.mean_sil_emb[[0, i]] = (old_sum[i] + emb[i]) / self.n_sil_frames as f32;
                }
            }
        }
    }

    /// Smart cache compression
    pub(super) fn compress_spkcache(&mut self) {
        let cache_preds = match &self.spkcache_preds {
            Some(p) => p.clone(),
            None => return,
        };

        let n_frames = self.spkcache.shape()[1];
        let spkcache_len_per_spk = SPKCACHE_LEN / NUM_SPEAKERS - SPKCACHE_SIL_FRAMES_PER_SPK;
        let strong_boost_per_spk = (spkcache_len_per_spk as f32 * STRONG_BOOST_RATE) as usize;
        let weak_boost_per_spk = (spkcache_len_per_spk as f32 * WEAK_BOOST_RATE) as usize;
        let min_pos_scores_per_spk = (spkcache_len_per_spk as f32 * MIN_POS_SCORES_RATE) as usize;

        // Calculate quality scores
        let preds_2d = cache_preds.slice(s![0, .., ..]).to_owned();
        let mut scores = self.get_log_pred_scores(&preds_2d);

        // Disable low scores
        scores = self.disable_low_scores(&preds_2d, scores, min_pos_scores_per_spk);

        // Boost important frames
        scores = self.boost_topk_scores(scores, strong_boost_per_spk, 2.0);
        scores = self.boost_topk_scores(scores, weak_boost_per_spk, 1.0);

        // Add silence frames placeholder
        if SPKCACHE_SIL_FRAMES_PER_SPK > 0 {
            let mut padded = Array2::from_elem(
                (n_frames + SPKCACHE_SIL_FRAMES_PER_SPK, NUM_SPEAKERS),
                f32::NEG_INFINITY,
            );
            padded.slice_mut(s![..n_frames, ..]).assign(&scores);
            for i in n_frames..n_frames + SPKCACHE_SIL_FRAMES_PER_SPK {
                for j in 0..NUM_SPEAKERS {
                    padded[[i, j]] = f32::INFINITY;
                }
            }
            scores = padded;
        }

        // Select top frames
        let (topk_indices, is_disabled) = self.get_topk_indices(&scores, n_frames);

        // Gather embeddings
        let (new_embs, new_preds) = self.gather_spkcache(&topk_indices, &is_disabled);

        self.spkcache = new_embs;
        self.spkcache_preds = Some(new_preds);
    }

    /// Calculate quality scores
    fn get_log_pred_scores(&self, preds: &Array2<f32>) -> Array2<f32> {
        let mut scores = Array2::zeros(preds.dim());

        for t in 0..preds.shape()[0] {
            let mut log_1_probs_sum = 0.0f32;
            for s in 0..NUM_SPEAKERS {
                let p = preds[[t, s]].max(PRED_SCORE_THRESHOLD);
                let log_1_p = (1.0 - p).max(PRED_SCORE_THRESHOLD).ln();
                log_1_probs_sum += log_1_p;
            }

            for s in 0..NUM_SPEAKERS {
                let p = preds[[t, s]].max(PRED_SCORE_THRESHOLD);
                let log_p = p.ln();
                let log_1_p = (1.0 - p).max(PRED_SCORE_THRESHOLD).ln();
                scores[[t, s]] = log_p - log_1_p + log_1_probs_sum - 0.5f32.ln();
            }
        }

        scores
    }

    /// Disable non-speech and overlapped speech
    fn disable_low_scores(
        &self,
        preds: &Array2<f32>,
        mut scores: Array2<f32>,
        min_pos_scores_per_spk: usize,
    ) -> Array2<f32> {
        // Count positive scores per speaker
        let mut pos_count = [0usize; NUM_SPEAKERS];
        for t in 0..scores.shape()[0] {
            for s in 0..NUM_SPEAKERS {
                if scores[[t, s]] > 0.0 {
                    pos_count[s] += 1;
                }
            }
        }

        for t in 0..preds.shape()[0] {
            for s in 0..NUM_SPEAKERS {
                let is_speech = preds[[t, s]] > 0.5;

                if !is_speech {
                    scores[[t, s]] = f32::NEG_INFINITY;
                } else {
                    let is_pos = scores[[t, s]] > 0.0;
                    if !is_pos && pos_count[s] >= min_pos_scores_per_spk {
                        scores[[t, s]] = f32::NEG_INFINITY;
                    }
                }
            }
        }

        scores
    }

    /// Boost top K frames per speaker
    fn boost_topk_scores(
        &self,
        mut scores: Array2<f32>,
        n_boost_per_spk: usize,
        scale_factor: f32,
    ) -> Array2<f32> {
        for s in 0..NUM_SPEAKERS {
            // Get column for this speaker
            let col: Vec<(usize, f32)> = (0..scores.shape()[0])
                .map(|t| (t, scores[[t, s]]))
                .collect();

            // Sort by score descending
            let mut sorted = col.clone();
            sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            // Boost top K
            for item in sorted.iter().take(n_boost_per_spk.min(sorted.len())) {
                let t = item.0;
                if scores[[t, s]] != f32::NEG_INFINITY {
                    scores[[t, s]] -= scale_factor * 0.5f32.ln();
                }
            }
        }

        scores
    }

    /// Get indices of top frames
    fn get_topk_indices(
        &self,
        scores: &Array2<f32>,
        n_frames_no_sil: usize,
    ) -> (Vec<usize>, Vec<bool>) {
        let n_frames = scores.shape()[0];

        // Flatten scores as (S, T) then reshape to (S*T,)
        // This means we iterate: speaker 0 all times, then speaker 1 all times, etc.
        // flat_index = speaker * n_frames + time
        let mut flat_scores: Vec<(usize, f32)> = Vec::with_capacity(n_frames * NUM_SPEAKERS);
        for s in 0..NUM_SPEAKERS {
            for t in 0..n_frames {
                let flat_idx = s * n_frames + t;
                flat_scores.push((flat_idx, scores[[t, s]]));
            }
        }

        // Sort by score descending to get top-K
        flat_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top SPKCACHE_LEN and replace invalid scores with MAX_INDEX
        let mut topk_flat: Vec<usize> = flat_scores
            .iter()
            .take(SPKCACHE_LEN)
            .map(|(idx, score)| {
                if *score == f32::NEG_INFINITY {
                    MAX_INDEX
                } else {
                    *idx
                }
            })
            .collect();

        // Sort flat indices ascending (this puts MAX_INDEX at the end)
        topk_flat.sort();

        // Compute is_disabled and convert to frame indices
        let mut is_disabled = vec![false; SPKCACHE_LEN];
        let mut frame_indices = vec![0usize; SPKCACHE_LEN];

        for (i, &flat_idx) in topk_flat.iter().enumerate() {
            if flat_idx == MAX_INDEX {
                // Invalid entries are disabled
                is_disabled[i] = true;
                frame_indices[i] = 0; // We set disabled to 0
            } else {
                // convert to frame index
                let frame_idx = flat_idx % n_frames;

                // check if frame is beyond valid range
                if frame_idx >= n_frames_no_sil {
                    is_disabled[i] = true;
                    frame_indices[i] = 0; // same as above: set disabled to 0
                } else {
                    frame_indices[i] = frame_idx;
                }
            }
        }

        (frame_indices, is_disabled)
    }

    /// Gather selected frames
    fn gather_spkcache(
        &self,
        indices: &[usize],
        is_disabled: &[bool],
    ) -> (Array3<f32>, Array3<f32>) {
        let mut new_embs = Array3::zeros((1, SPKCACHE_LEN, EMB_DIM));
        let mut new_preds = Array3::zeros((1, SPKCACHE_LEN, NUM_SPEAKERS));

        let cache_preds = self.spkcache_preds.as_ref().unwrap();

        for (i, (&idx, &disabled)) in indices.iter().zip(is_disabled.iter()).enumerate() {
            if i >= SPKCACHE_LEN {
                break;
            }

            if disabled {
                // Use silence embedding
                new_embs
                    .slice_mut(s![0, i, ..])
                    .assign(&self.mean_sil_emb.slice(s![0, ..]));
                // Predictions stay zero
            } else if idx < self.spkcache.shape()[1] {
                new_embs
                    .slice_mut(s![0, i, ..])
                    .assign(&self.spkcache.slice(s![0, idx, ..]));
                new_preds
                    .slice_mut(s![0, i, ..])
                    .assign(&cache_preds.slice(s![0, idx, ..]));
            }
        }

        (new_embs, new_preds)
    }
}
