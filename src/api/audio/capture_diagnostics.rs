//! Bounded capture metadata and signal summaries; never stores audio samples.

use cpal::traits::DeviceTrait;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

mod endpoints;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
const MAX_CHANNELS: usize = 32;

#[derive(Clone)]
pub(crate) struct CaptureDiagnostics(Arc<Inner>);

struct Inner {
    id: u64,
    started: Instant,
    stats: Mutex<Stats>,
    skipped: AtomicU64,
}

#[derive(Default, Clone)]
struct Level {
    samples: u64,
    nonzero: u64,
    nonfinite: u64,
    clipped: u64,
    sum_sq: f64,
    peak: f64,
}

impl Level {
    fn observe(&mut self, value: f64) {
        self.samples += 1;
        if !value.is_finite() {
            self.nonfinite += 1;
            return;
        }
        self.nonzero += u64::from(value != 0.0);
        self.clipped += u64::from(value.abs() >= 1.0);
        self.sum_sq += value * value;
        self.peak = self.peak.max(value.abs());
    }

    fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "samples": self.samples, "nonzero": self.nonzero,
            "nonfinite": self.nonfinite, "clipped": self.clipped,
            "rms": (self.sum_sq / self.samples.max(1) as f64).sqrt(), "peak": self.peak,
        })
    }
}

#[derive(Default)]
struct Stats {
    controls: Option<(Arc<AtomicBool>, Arc<AtomicBool>)>,
    channels: usize,
    raw: Vec<Level>,
    mono: Level,
    output: Level,
    callbacks: u64,
    suppressed: u64,
    malformed: u64,
    last_callback: Option<Instant>,
    selected: Option<String>,
}

impl Inner {
    fn report(&self, phase: &str) {
        let Ok(stats) = self.stats.lock() else { return };
        crate::log_info!(
            "[AudioCapture] id={} phase={} uptime_ms={} callbacks={} suppressed={} malformed={} skipped_observations={} since_callback_ms={:?} channels={} raw={} mono_before_resample={} delivered={} stop_pause={:?}",
            self.id,
            phase,
            self.started.elapsed().as_millis(),
            stats.callbacks,
            stats.suppressed,
            stats.malformed,
            self.skipped.load(Ordering::Relaxed),
            stats.last_callback.map(|time| time.elapsed().as_millis()),
            stats.channels,
            serde_json::Value::Array(stats.raw.iter().map(Level::json).collect()),
            stats.mono.json(),
            stats.output.json(),
            stats
                .controls
                .as_ref()
                .map(|(stop, pause)| (stop.load(Ordering::Relaxed), pause.load(Ordering::Relaxed)))
        );
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.report("released");
    }
}

impl CaptureDiagnostics {
    pub(crate) fn controls(&self, stop: Arc<AtomicBool>, pause: Arc<AtomicBool>) {
        if let Ok(mut stats) = self.0.stats.lock() {
            stats.controls = Some((stop, pause));
        }
    }
    pub(crate) fn new(owner: &str, source: &str) -> Self {
        let inner = Arc::new(Inner {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            started: Instant::now(),
            stats: Mutex::new(Stats::default()),
            skipped: AtomicU64::new(0),
        });
        crate::log_info!(
            "[AudioCapture] id={} phase=opening pid={} owner={:?} source={:?}",
            inner.id,
            std::process::id(),
            owner,
            source
        );
        let weak = Arc::downgrade(&inner);
        let id = inner.id;
        if let Err(error) = std::thread::Builder::new()
            .name("audio-diagnostics".into())
            .spawn(move || {
                let mut previous = String::new();
                let mut inventory = true;
                loop {
                    let Some(inner) = weak.upgrade() else { break };
                    let selected = inner.stats.lock().ok().and_then(|s| s.selected.clone());
                    // Endpoint queries and formatting run outside the audio callback.
                    let state = endpoints::snapshot(selected.as_deref(), inventory);
                    if state != previous {
                        crate::log_info!(
                            "[AudioCapture] id={} phase=endpoints state={}",
                            id,
                            state
                        );
                        previous = state;
                    }
                    inventory = false;
                    inner.report("health");
                    drop(inner);
                    std::thread::sleep(Duration::from_secs(5));
                }
            })
        {
            crate::log_info!(
                "[AudioCapture] id={} phase=monitor_error error={:?}",
                id,
                error
            );
        }
        Self(inner)
    }

    pub(crate) fn configure(&self, device: &cpal::Device, config: &cpal::SupportedStreamConfig) {
        let id = device.id().ok().map(|id| id.id().to_string());
        let name = device.description().ok().map(|d| d.name().to_string());
        if let Ok(mut stats) = self.0.stats.lock() {
            stats.channels = config.channels() as usize;
            stats.raw = vec![Level::default(); stats.channels.min(MAX_CHANNELS)];
            stats.selected = id.clone();
        }
        crate::log_info!(
            "[AudioCapture] id={} phase=configured device_id={:?} name={:?} rate={} channels={} format={:?} buffer={:?}",
            self.0.id,
            id,
            name,
            config.sample_rate(),
            config.channels(),
            config.sample_format(),
            config.buffer_size()
        );
        crate::log_info!(
            "[AudioCapture] id={} phase=selected_endpoint state={}",
            self.0.id,
            endpoints::snapshot(id.as_deref(), false)
        );
    }

    pub(crate) fn event(&self, phase: &str, detail: &str) {
        crate::log_info!(
            "[AudioCapture] id={} phase={} detail={:?}",
            self.0.id,
            phase,
            detail
        );
    }

    pub(crate) fn raw(&self, samples: impl Iterator<Item = f64>, suppressed: bool) {
        let Ok(mut stats) = self.0.stats.try_lock() else {
            self.0.skipped.fetch_add(1, Ordering::Relaxed);
            return;
        };
        stats.callbacks += 1;
        stats.suppressed += u64::from(suppressed);
        stats.last_callback = Some(Instant::now());
        let channels = stats.channels.max(1);
        let mut count = 0;
        let mut sum = 0.0;
        for (index, value) in samples.enumerate() {
            if let Some(level) = stats.raw.get_mut(index % channels) {
                level.observe(value);
            }
            sum += value;
            count += 1;
            if count % channels == 0 {
                stats.mono.observe(sum / channels as f64);
                sum = 0.0;
            }
        }
        stats.malformed += u64::from(count % channels != 0);
    }

    pub(crate) fn delivered(&self, samples: impl Iterator<Item = f64>) {
        if let Ok(mut stats) = self.0.stats.try_lock() {
            for value in samples {
                stats.output.observe(value);
            }
        } else {
            self.0.skipped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinguishes_silence_from_invalid_or_clipped_samples() {
        let mut level = Level::default();
        for sample in [0.0, 0.5, -1.0, f64::NAN] {
            level.observe(sample);
        }
        assert_eq!(level.samples, 4);
        assert_eq!(level.nonzero, 2);
        assert_eq!(level.clipped, 1);
        assert_eq!(level.nonfinite, 1);
        assert_eq!(level.peak, 1.0);
        assert!(level.json()["rms"].as_f64().unwrap().is_finite());
    }

    fn observer() -> CaptureDiagnostics {
        CaptureDiagnostics(Arc::new(Inner {
            id: 0,
            started: Instant::now(),
            skipped: AtomicU64::new(0),
            stats: Mutex::new(Stats {
                channels: 2,
                raw: vec![Level::default(); 2],
                ..Stats::default()
            }),
        }))
    }

    #[test]
    fn preserves_channel_evidence_when_mono_cancels_and_capture_is_suppressed() {
        let diagnostics = observer();
        diagnostics.raw([0.5, -0.5, 0.25, -0.25].into_iter(), true);
        let stats = diagnostics.0.stats.lock().unwrap();
        assert_eq!(stats.callbacks, 1);
        assert_eq!(stats.suppressed, 1);
        assert_eq!(stats.raw[0].nonzero, 2);
        assert_eq!(stats.raw[1].nonzero, 2);
        assert_eq!(stats.mono.nonzero, 0);
        assert_eq!(stats.output.samples, 0);
    }

    #[test]
    fn counts_delivered_silence_and_malformed_frames_separately() {
        let diagnostics = observer();
        diagnostics.raw([0.0, 0.0, 0.0].into_iter(), false);
        diagnostics.delivered([0.0, 0.0].into_iter());
        let stats = diagnostics.0.stats.lock().unwrap();
        assert_eq!(stats.malformed, 1);
        assert_eq!(stats.output.samples, 2);
        assert_eq!(stats.output.nonzero, 0);
        assert!(stats.last_callback.is_some());
    }
}
