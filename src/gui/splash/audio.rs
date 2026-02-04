// --- SPLASH AUDIO ENGINE ---
// Procedural audio synthesis for the splash screen animation.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::f32::consts::PI;
use std::sync::{Arc, Mutex};

/// Shared state between main thread and audio thread
pub struct SharedAudioState {
    pub physics_t: f32,
    pub warp_progress: f32,
    pub impact_trigger: bool,
    pub is_dark: bool,
    pub is_finished: bool,
}

/// Internal state used ONLY by the audio thread (no lock needed)
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
    samples_rendered: u64,
    last_physics_t: f32,
    last_warp_progress: f32,
    last_is_dark: bool,
}

pub struct SplashAudio {
    pub _stream: cpal::Stream,
    pub state: Arc<Mutex<SharedAudioState>>,
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
        let sample_rate = u32::from(config.sample_rate()) as f32;
        let channels = config.channels() as usize;

        // Internal rendering state stays in the closure
        let mut r = RenderState {
            vox_phase1: 0.0,
            vox_phase2: 0.0,
            vox_phase3: 0.0,
            impact_phase1: 0.0,
            impact_phase2: 0.0,
            impact_phase3: 0.0,
            whoosh_phase: 0.0,
            noise_state: 0.0,
            impact_env: 0.0,
            samples_rendered: 0,
            last_physics_t: 0.0,
            last_warp_progress: 0.0,
            last_is_dark: false,
        };

        let stream = device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _| {
                    // Try non-blocking lock to avoid audio pops from blocking
                    let (physics_t, warp_progress, is_dark, is_finished, trigger_impact) =
                        if let Ok(mut s_lock) = state_clone.try_lock() {
                            let pt = s_lock.physics_t;
                            let wp = s_lock.warp_progress;
                            let dark = s_lock.is_dark;
                            let fin = s_lock.is_finished;
                            let trigger = s_lock.impact_trigger;
                            if trigger {
                                s_lock.impact_trigger = false;
                            }
                            r.last_physics_t = pt;
                            r.last_warp_progress = wp;
                            r.last_is_dark = dark;
                            (pt, wp, dark, fin, trigger)
                        } else {
                            // Lock busy - use cached values
                            (
                                r.last_physics_t,
                                r.last_warp_progress,
                                r.last_is_dark,
                                false,
                                false,
                            )
                        };

                    if is_finished {
                        for x in data.iter_mut() {
                            *x = 0.0;
                        }
                        return;
                    }

                    if trigger_impact {
                        r.impact_env = 1.0;
                    }

                    // Startup fade to prevent initial pop (first ~50ms)
                    let startup_samples = (sample_rate * 0.05) as u64;

                    for frame in data.chunks_mut(channels) {
                        // 0. ENVELOPE (original)
                        let attack = (physics_t / 0.05).min(1.0);
                        let decay = (1.0 - (physics_t - 1.6).max(0.0) / 0.8).max(0.0);
                        let env = attack * decay;

                        // 1. VOXEL SHIMMER (original frequencies/volumes, proper phase)
                        let theme_freq = if is_dark { 110.0 } else { 220.0 };
                        let base_freq = theme_freq + (physics_t * 40.0);
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
                            + ((r.vox_phase1 * 43758.5453).sin().fract() - 0.5) * 0.012)
                            .clamp(-1.0, 1.0);
                        let wind = r.noise_state * 0.012 * env;

                        // 3. ASSEMBLY IMPACT (original frequencies/volumes, proper phase)
                        let mut impact = 0.0;
                        if r.impact_env > 0.0001 {
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
                            r.impact_env *= 0.99985;
                        }

                        // 4. WARP WHOOSH (original frequencies/volumes, proper phase)
                        let mut whoosh = 0.0;
                        if warp_progress > 0.0001 {
                            let p = warp_progress;
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

                        let mixed =
                            ((voxels + wind + impact + whoosh) * startup_fade).clamp(-1.0, 1.0);
                        for sample in frame.iter_mut() {
                            *sample = mixed;
                        }
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
        })
    }
}
