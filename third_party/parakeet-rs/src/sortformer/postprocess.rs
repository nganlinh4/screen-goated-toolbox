use super::{Sortformer, SpeakerSegment, FRAME_DURATION, NUM_SPEAKERS};
use ndarray::{Array2, Array3, Axis};

impl Sortformer {
    /// Concatenate along axis 1 for 3D arrays
    pub(super) fn concat_axis1(a: &Array3<f32>, b: &Array3<f32>) -> Array3<f32> {
        if a.shape()[1] == 0 {
            return b.clone();
        }
        if b.shape()[1] == 0 {
            return a.clone();
        }
        ndarray::concatenate(Axis(1), &[a.view(), b.view()]).unwrap()
    }

    /// Concatenate along axis 0 for 2D arrays
    pub(super) fn concat_axis1_2d(a: &Array2<f32>, b: &Array2<f32>) -> Array2<f32> {
        if a.shape()[0] == 0 {
            return b.clone();
        }
        if b.shape()[0] == 0 {
            return a.clone();
        }
        ndarray::concatenate(Axis(0), &[a.view(), b.view()]).unwrap()
    }

    /// Concatenate predictions
    pub(super) fn concat_predictions(preds: &[Array2<f32>]) -> Array2<f32> {
        if preds.is_empty() {
            return Array2::zeros((0, NUM_SPEAKERS));
        }
        if preds.len() == 1 {
            return preds[0].clone();
        }

        let views: Vec<_> = preds.iter().map(|p| p.view()).collect();
        ndarray::concatenate(Axis(0), &views).unwrap()
    }

    /// Apply median filter to predictions
    pub(super) fn median_filter(&self, preds: &Array2<f32>) -> Array2<f32> {
        let window = self.config.median_window;
        let half = window / 2;
        let mut filtered = preds.clone();

        for spk in 0..NUM_SPEAKERS {
            for t in 0..preds.shape()[0] {
                let start = t.saturating_sub(half);
                let end = (t + half + 1).min(preds.shape()[0]);

                let mut values: Vec<f32> = (start..end).map(|i| preds[[i, spk]]).collect();
                values.sort_by(|a, b| a.partial_cmp(b).unwrap());

                filtered[[t, spk]] = values[values.len() / 2];
            }
        }

        filtered
    }

    /// Binarize predictions to segments (padding applied during thresholding)
    pub(super) fn binarize(&self, preds: &Array2<f32>) -> Vec<SpeakerSegment> {
        let mut segments = Vec::new();
        let num_frames = preds.shape()[0];

        for spk in 0..NUM_SPEAKERS {
            let mut in_seg = false;
            let mut seg_start = 0;
            let mut temp_segments = Vec::new();

            for t in 0..num_frames {
                let p = preds[[t, spk]];

                if p >= self.config.onset && !in_seg {
                    in_seg = true;
                    seg_start = t;
                } else if p < self.config.offset && in_seg {
                    in_seg = false;

                    // Apply padding during conversion
                    let start_t =
                        (seg_start as f32 * FRAME_DURATION - self.config.pad_onset).max(0.0);
                    let end_t = t as f32 * FRAME_DURATION + self.config.pad_offset;

                    if end_t - start_t >= self.config.min_duration_on {
                        temp_segments.push(SpeakerSegment {
                            start: start_t,
                            end: end_t,
                            speaker_id: spk,
                        });
                    }
                }
            }

            // Handle segment at end
            if in_seg {
                let start_t = (seg_start as f32 * FRAME_DURATION - self.config.pad_onset).max(0.0);
                let end_t = num_frames as f32 * FRAME_DURATION + self.config.pad_offset;

                if end_t - start_t >= self.config.min_duration_on {
                    temp_segments.push(SpeakerSegment {
                        start: start_t,
                        end: end_t,
                        speaker_id: spk,
                    });
                }
            }

            // Merge close segments (min_duration_off)
            if temp_segments.len() > 1 {
                let mut filtered = vec![temp_segments[0].clone()];
                for seg in temp_segments.into_iter().skip(1) {
                    let last = filtered.last_mut().unwrap();
                    let gap = seg.start - last.end;
                    if gap < self.config.min_duration_off {
                        last.end = seg.end; // Merge
                    } else {
                        filtered.push(seg);
                    }
                }
                segments.extend(filtered);
            } else {
                segments.extend(temp_segments);
            }
        }

        // Sort by start time
        segments.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap());
        segments
    }
}
