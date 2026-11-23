// --- SPLASH AUDIO ENGINE ---
// Procedural audio synthesis for the splash screen animation.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// The voxel shimmer and cosmic wind have faded out by this animation time.
const SHIMMER_END: f32 = 2.4;
/// Per-sample decay of the assembly impact (about 600 ms of ring-out).
const IMPACT_DECAY_PER_SAMPLE: f32 = 0.99985;
/// Impact envelope below which it is inaudible and no longer rendered.
const IMPACT_SILENCE: f32 = 0.0001;

/// Shared state between main thread and audio thread
pub struct SharedAudioState {
    pub physics_t: f32,
    pub warp_progress: f32,
    pub impact_trigger: bool,
    pub is_dark: bool,
    pub is_finished: bool,
}

/// One buffer's view of the shared state.
#[derive(Clone, Copy)]
struct Inputs {
    physics_t: f32,
    warp_progress: f32,
    is_dark: bool,
    trigger_impact: bool,
}

/// Internal state used ONLY by the audio thread (no lock needed)
#[derive(Default)]
struct RenderState {
    // Proper phase accumulators (0.0 to 1.0, wrap around)
    vox_phase1: f32,
    vox_phase2: f32,
    vox_phase3: f32,
    impact_phase1: f32,
    impact_phase2: f32,
    impact_phase3: f32,
    whoosh_phase: f32,
    // Envelope/state
    noise_state: f32,
    impact_env: f32,
    impact_seen: bool,
    samples_rendered: u64,
    last_physics_t: f32,
    last_warp_progress: f32,
    last_is_dark: bool,
}

impl RenderState {
    fn render(&mut self, data: &mut [f32], channels: usize, sample_rate: f32, input: Inputs) {
        let r = self;
        if input.trigger_impact {
            r.impact_env = 1.0;
            r.impact_seen = true;
        }

        // Startup fade to prevent initial pop (first ~50ms)
        let startup_samples = (sample_rate * 0.05) as u64;

        for frame in data.chunks_mut(channels) {
            // 0. ENVELOPE (original)
            let attack = (input.physics_t / 0.05).min(1.0);
            let decay = (1.0 - (input.physics_t - 1.6).max(0.0) / (SHIMMER_END - 1.6)).max(0.0);
            let env = attack * decay;

            // 1. VOXEL SHIMMER (original frequencies/volumes, proper phase)
            let theme_freq = if input.is_dark { 110.0 } else { 220.0 };
            let base_freq = theme_freq + (input.physics_t * 40.0);
            let vol_vox = env * 0.03;

            r.vox_phase1 += base_freq / sample_rate;
            r.vox_phase2 += (base_freq * 1.5) / sample_rate;
            r.vox_phase3 += (base_freq * 2.5) / sample_rate;
            while r.vox_phase1 >= 1.0 {
                r.vox_phase1 -= 1.0;
            }
            while r.vox_phase2 >= 1.0 {
                r.vox_phase2 -= 1.0;
            }
            while r.vox_phase3 >= 1.0 {
                r.vox_phase3 -= 1.0;
            }

            let s1 = (r.vox_phase1 * 2.0 * PI).sin();
            let s2 = (r.vox_phase2 * 2.0 * PI).sin();
            let s3 = (r.vox_phase3 * 2.0 * PI).sin();
            let voxels = (s1 + s2 * 0.5 + s3 * 0.3) * vol_vox;

            // 2. COSMIC WIND (original)
            r.noise_state = (r.noise_state * 0.994
                + ((r.vox_phase1 * 43_758.547).sin().fract() - 0.5) * 0.012)
                .clamp(-1.0, 1.0);
            let wind = r.noise_state * 0.012 * env;

            // 3. ASSEMBLY IMPACT (original frequencies/volumes, proper phase)
            let mut impact = 0.0;
            if r.impact_env > IMPACT_SILENCE {
                let f_base = 180.0 + r.impact_env * 360.0;
                r.impact_phase1 += f_base / sample_rate;
                r.impact_phase2 += (f_base * 2.1) / sample_rate;
                r.impact_phase3 += (f_base * 3.5) / sample_rate;
                while r.impact_phase1 >= 1.0 {
                    r.impact_phase1 -= 1.0;
                }
                while r.impact_phase2 >= 1.0 {
                    r.impact_phase2 -= 1.0;
                }
                while r.impact_phase3 >= 1.0 {
                    r.impact_phase3 -= 1.0;
                }

                let h1 = (r.impact_phase1 * 2.0 * PI).sin();
                let h2 = (r.impact_phase2 * 2.0 * PI).sin();
                let h3 = (r.impact_phase3 * 2.0 * PI).sin();
                // Smooth envelope curve (squared for natural decay tail)
                let smooth_env = r.impact_env * r.impact_env;
                impact = (h1 + h2 * 0.4 + h3 * 0.2) * smooth_env * 0.05;
                // Slower decay for longer ring-out (~600ms instead of ~200ms)
                r.impact_env *= IMPACT_DECAY_PER_SAMPLE;
            }

            // 4. WARP WHOOSH (original frequencies/volumes, proper phase)
            let mut whoosh = 0.0;
            if input.warp_progress > 0.0001 {
                let p = input.warp_progress;
                let whoosh_freq = 80.0 + p.powf(1.5) * 4500.0;
                r.whoosh_phase += whoosh_freq / sample_rate;
                while r.whoosh_phase >= 1.0 {
                    r.whoosh_phase -= 1.0;
                }

                let attack_w = (p / 0.1).min(1.0);
                let decay_w = (1.0 - (p - 0.15).max(0.0) / 0.7).max(0.0);
                let whoosh_vol = attack_w * decay_w * 0.07;
                whoosh = (r.whoosh_phase * 2.0 * PI).sin() * whoosh_vol;
            }

            // Apply startup fade
            let startup_fade = if r.samples_rendered < startup_samples {
                r.samples_rendered as f32 / startup_samples as f32
            } else {
                1.0
            };
            r.samples_rendered = r.samples_rendered.saturating_add(1);

            let mixed = ((voxels + wind + impact + whoosh) * startup_fade).clamp(-1.0, 1.0);
            for sample in frame.iter_mut() {
                *sample = mixed;
            }
        }
    }

    /// The intro shimmer and the impact have both rung out, so the stream
    /// would only play silence until an exit whoosh.
    fn intro_finished(&self, physics_t: f32) -> bool {
        physics_t >= SHIMMER_END && self.impact_seen && self.impact_env <= IMPACT_SILENCE
    }
}

pub struct SplashAudio {
    _stream: cpal::Stream,
    pub state: Arc<Mutex<SharedAudioState>>,
    intro_done: Arc<AtomicBool>,
}

impl SplashAudio {
    pub fn new() -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let config = device.default_output_config().ok()?;

        let state = Arc::new(Mutex::new(SharedAudioState {
            physics_t: 0.0,
            warp_progress: 0.0,
            impact_trigger: false,
            is_dark: false,
            is_finished: false,
        }));

        let state_clone = Arc::clone(&state);
        let intro_done = Arc::new(AtomicBool::new(false));
        let intro_done_clone = Arc::clone(&intro_done);
        let sample_rate = config.sample_rate() as f32;
        let channels = config.channels() as usize;

        // Internal rendering state stays in the closure
        let mut r = RenderState::default();

        let stream = device
            .build_output_stream(
                config.into(),
                move |data: &mut [f32], _| {
                    // Try non-blocking lock to avoid audio pops from blocking
                    let (input, is_finished) = if let Ok(mut s_lock) = state_clone.try_lock() {
                        let trigger_impact = std::mem::take(&mut s_lock.impact_trigger);
                        r.last_physics_t = s_lock.physics_t;
                        r.last_warp_progress = s_lock.warp_progress;
                        r.last_is_dark = s_lock.is_dark;
                        let input = Inputs {
                            physics_t: s_lock.physics_t,
                            warp_progress: s_lock.warp_progress,
                            is_dark: s_lock.is_dark,
                            trigger_impact,
                        };
                        (input, s_lock.is_finished)
                    } else {
                        // Lock busy - use cached values
                        let input = Inputs {
                            physics_t: r.last_physics_t,
                            warp_progress: r.last_warp_progress,
                            is_dark: r.last_is_dark,
                            trigger_impact: false,
                        };
                        (input, false)
                    };

                    if is_finished {
                        data.fill(0.0);
                        return;
                    }
                    r.render(data, channels, sample_rate, input);
                    if r.intro_finished(input.physics_t) {
                        intro_done_clone.store(true, Ordering::Relaxed);
                    }
                },
                |err| crate::log_info!("Splash audio stream error: {}", err),
                None,
            )
            .ok()?;

        stream.play().ok()?;
        Some(Self {
            _stream: stream,
            state,
            intro_done,
        })
    }

    /// Opens the output stream off the UI thread and stores it in `slot`.
    pub fn open_in_background(slot: Arc<Mutex<Option<Self>>>) {
        std::thread::spawn(move || {
            if let Some(audio) = Self::new()
                && let Ok(mut lock) = slot.lock()
            {
                *lock = Some(audio);
            }
        });
    }

    pub fn intro_done(&self) -> bool {
        self.intro_done.load(Ordering::Relaxed)
    }
}

/// Whether the splash still needs its output stream. An open WASAPI stream
/// keeps the audio engine awake and can hold off idle sleep, so it is closed
/// once the intro has rung out and reopened only for an audible exit.
pub(super) fn needs_stream(exiting: bool, quiet_exit: bool, intro_done: bool) -> bool {
    if exiting { !quiet_exit } else { !intro_done }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;
    const BUFFER_SECS: f32 = 0.02;

    fn buffer() -> Vec<f32> {
        vec![0.0; (RATE * BUFFER_SECS) as usize * 2]
    }

    fn input(physics_t: f32, trigger_impact: bool) -> Inputs {
        Inputs {
            physics_t,
            warp_progress: 0.0,
            is_dark: false,
            trigger_impact,
        }
    }

    #[test]
    fn the_intro_reports_done_only_after_the_impact_rings_out_to_silence() {
        let mut state = RenderState::default();
        let mut data = buffer();
        let mut impact_sent = false;
        let mut done_at = None;
        for step in 0..400 {
            let physics_t = (step as f32 * BUFFER_SECS).min(3.6);
            let trigger_impact = physics_t > 1.6 && !impact_sent;
            impact_sent |= trigger_impact;
            state.render(&mut data, 2, RATE, input(physics_t, trigger_impact));
            if trigger_impact {
                assert!(data.iter().any(|sample| sample.abs() > 1e-3));
            }
            if state.intro_finished(physics_t) {
                done_at = Some(physics_t);
                break;
            }
        }
        let done_at = done_at.expect("the intro must finish");
        assert!(done_at >= SHIMMER_END, "finished at {done_at}");
        assert!(done_at < 3.6, "finished at {done_at}");

        state.render(&mut data, 2, RATE, input(3.6, false));
        assert!(data.iter().all(|sample| sample.abs() < 1e-6));
    }

    #[test]
    fn the_intro_is_not_done_before_the_impact_has_played() {
        let mut state = RenderState::default();
        let mut data = buffer();
        state.render(&mut data, 2, RATE, input(3.6, false));
        assert!(!state.intro_finished(3.6));
    }

    #[test]
    fn the_stream_stays_open_only_while_something_can_still_be_heard() {
        assert!(needs_stream(false, false, false));
        assert!(!needs_stream(false, false, true));
        assert!(needs_stream(true, false, true));
        assert!(!needs_stream(true, true, false));
    }
}
