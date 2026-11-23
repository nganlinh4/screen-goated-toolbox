//! Per-frame capture-size lookup for window recordings whose source window was
//! resized mid-capture. Built once per export; every lookup is a binary search.

use super::config::MousePosition;

const EXACT_MATCH_SEC: f64 = 0.001;

#[derive(Clone, Copy, Debug, PartialEq)]
struct CaptureSize {
    timestamp: f64,
    width: f64,
    height: f64,
}

#[derive(Default)]
pub(super) struct CaptureDimensionTrack {
    sizes: Vec<CaptureSize>,
}

impl CaptureDimensionTrack {
    /// Keeps only samples that carry a usable capture size, in time order.
    pub(super) fn new(mouse_positions: &[MousePosition]) -> Self {
        let mut sizes: Vec<CaptureSize> = mouse_positions
            .iter()
            .filter(|position| position.timestamp.is_finite())
            .filter_map(|position| {
                let (width, height) = position.capture_width.zip(position.capture_height)?;
                (width.is_finite() && height.is_finite() && width > 1.0 && height > 1.0).then_some(
                    CaptureSize {
                        timestamp: position.timestamp,
                        width,
                        height,
                    },
                )
            })
            .collect();
        sizes.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
        Self { sizes }
    }

    /// Capture size at `time`, interpolated between the surrounding samples
    /// and held at the ends. Recordings without size samples use the fallback.
    pub(super) fn sample(
        &self,
        time: f64,
        fallback_width: f64,
        fallback_height: f64,
    ) -> (f64, f64) {
        let sizes = &self.sizes;
        if sizes.is_empty() {
            return (fallback_width.max(1.0), fallback_height.max(1.0));
        }
        let exact = sizes.partition_point(|size| size.timestamp <= time - EXACT_MATCH_SEC);
        if let Some(size) = sizes.get(exact)
            && (size.timestamp - time).abs() < EXACT_MATCH_SEC
        {
            return (size.width, size.height);
        }
        let next_index = sizes.partition_point(|size| size.timestamp <= time);
        let prev = next_index.checked_sub(1).map(|index| sizes[index]);
        let next = sizes.get(next_index).copied();
        match (prev, next) {
            (Some(p), Some(n)) => {
                let dt = (n.timestamp - p.timestamp).max(0.000001);
                let t = ((time - p.timestamp) / dt).clamp(0.0, 1.0);
                (
                    p.width + (n.width - p.width) * t,
                    p.height + (n.height - p.height) * t,
                )
            }
            (Some(edge), None) | (None, Some(edge)) => (edge.width, edge.height),
            (None, None) => unreachable!("non-empty track has a neighbour"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CaptureDimensionTrack;
    use crate::overlay::screen_record::native_export::config::MousePosition;

    fn position(timestamp: f64, size: Option<(f64, f64)>) -> MousePosition {
        MousePosition {
            x: 0.0,
            y: 0.0,
            timestamp,
            is_clicked: false,
            cursor_type: None,
            cursor_rotation: None,
            capture_width: size.map(|(w, _)| w),
            capture_height: size.map(|(_, h)| h),
        }
    }

    #[test]
    fn interpolates_between_sized_samples_and_skips_unsized_ones() {
        let track = CaptureDimensionTrack::new(&[
            position(0.0, Some((800.0, 600.0))),
            position(0.5, None),
            position(1.0, Some((1000.0, 700.0))),
            position(2.0, Some((0.5, 0.5))),
        ]);
        assert_eq!(track.sample(-1.0, 1.0, 1.0), (800.0, 600.0));
        assert_eq!(track.sample(0.0004, 1.0, 1.0), (800.0, 600.0));
        assert_eq!(track.sample(0.5, 1.0, 1.0), (900.0, 650.0));
        assert_eq!(track.sample(1.0, 1.0, 1.0), (1000.0, 700.0));
        assert_eq!(track.sample(5.0, 1.0, 1.0), (1000.0, 700.0));
    }

    #[test]
    fn display_recordings_without_sizes_use_the_fallback() {
        let positions: Vec<_> = (0..1000).map(|i| position(i as f64 / 60.0, None)).collect();
        let track = CaptureDimensionTrack::new(&positions);
        assert_eq!(track.sample(3.0, 1920.0, 1080.0), (1920.0, 1080.0));
    }

    #[test]
    fn hour_long_cursor_track_lookups_stay_logarithmic() {
        // One hour of 60 Hz samples, looked up for every 60 fps frame with
        // 8 motion-blur taps. A linear scan per lookup would take minutes.
        let positions: Vec<_> = (0..216_000)
            .map(|i| position(i as f64 / 60.0, Some((1280.0 + (i % 7) as f64, 720.0))))
            .collect();
        let track = CaptureDimensionTrack::new(&positions);
        let started = std::time::Instant::now();
        let mut checksum = 0.0;
        for frame in 0..216_000 {
            for tap in 0..8 {
                let time = frame as f64 / 60.0 + tap as f64 / 480.0;
                checksum += track.sample(time, 1.0, 1.0).0;
            }
        }
        assert!(checksum > 0.0);
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }
}
