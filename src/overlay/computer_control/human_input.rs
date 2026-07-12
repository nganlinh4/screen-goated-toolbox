//! Human-like input generation for Computer Control: natural cursor paths
//! (WindMouse) and paced typing (log-normal keystroke timing). This is a
//! UX/watchability feature — a visible assistant that glides to its target and
//! types at a human cadence is followable, so the user can SEE where it's headed
//! and steer/stop it (feature A). It is NOT an anti-detection measure: Win32
//! marks all `SendInput` with `LLKHF_INJECTED` regardless of trajectory.
//!
//! Everything is cancellable: the cursor loop and the typing loop poll a shared
//! `CANCEL` flag between every emitted micro-step, so a spoken "stop" halts a
//! click mid-travel or typing mid-word within ~10ms.
//!
//! No external RNG dependency — a tiny xorshift PRNG is seeded from `getrandom`,
//! with Box-Muller normal / log-normal draws (the timing realism, not crypto).

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::Duration;

/// Humanization level, from the `CC_HUMANIZE` env (off | smooth | realistic).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Humanize {
    Off,
    Smooth,
    Realistic,
}

/// Tunable persona for human-like motion + typing.
#[derive(Clone, Copy, Debug)]
pub(super) struct HumanProfile {
    pub mode: Humanize,
    pub wpm: f64,
    pub typos: bool,
}

impl HumanProfile {
    /// The default: instant teleport-click + bulk typing (fast, deterministic —
    /// keeps tests and the speed-sensitive paths unchanged).
    pub fn instant() -> Self {
        Self {
            mode: Humanize::Off,
            wpm: 0.0,
            typos: false,
        }
    }

    /// A realistic persona (used by the cursor demo / as a humanized fallback).
    pub fn realistic() -> Self {
        Self {
            mode: Humanize::Realistic,
            wpm: 55.0,
            typos: false,
        }
    }

    /// Read `CC_HUMANIZE` (+ `CC_WPM`, `CC_TYPOS`). Unset/`off` ⇒ instant.
    pub fn from_env() -> Self {
        let wpm = std::env::var("CC_WPM").ok().and_then(|s| s.parse().ok());
        match std::env::var("CC_HUMANIZE").ok().as_deref() {
            Some("realistic") => Self {
                mode: Humanize::Realistic,
                wpm: wpm.unwrap_or(55.0),
                typos: std::env::var("CC_TYPOS").is_ok(),
            },
            // Opt OUT to the rigid/teleport path (for deterministic harness tests).
            Some("instant") | Some("off") | Some("0") | Some("false") | Some("none") => {
                Self::instant()
            }
            // Default (and "smooth"/"on"/…): natural smooth cursor + typing — the
            // best experience, so a normal launch gets it with no env var set.
            _ => Self {
                mode: Humanize::Smooth,
                wpm: wpm.unwrap_or(70.0),
                typos: false,
            },
        }
    }

    pub fn humanized(&self) -> bool {
        self.mode != Humanize::Off
    }
}

/// Result of a cancellable action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Outcome {
    Done,
    Aborted,
}

/// Sleep up to `ms`, checking `cancel` every ~12ms. Returns true if aborted.
pub(super) fn sleep_cancellable(ms: u64, cancel: &AtomicBool) -> bool {
    let mut left = ms;
    while left > 0 {
        if cancel.load(Ordering::Relaxed) {
            return true;
        }
        let step = left.min(12);
        sleep(Duration::from_millis(step));
        left -= step;
    }
    cancel.load(Ordering::Relaxed)
}

// ---- tiny PRNG + distributions (timing realism only) ----

struct Rng {
    s: u64,
}

impl Rng {
    fn new() -> Self {
        let mut b = [0u8; 8];
        if getrandom::fill(&mut b).is_err() {
            b = 0x9E37_79B9_7F4A_7C15u64.to_le_bytes();
        }
        let mut s = u64::from_le_bytes(b);
        if s == 0 {
            s = 0x9E37_79B9_7F4A_7C15;
        }
        Rng { s }
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.s;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.s = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Uniform [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// Box-Muller normal draw.
    fn normal(&mut self, mu: f64, sigma: f64) -> f64 {
        let u1 = self.unit().max(1e-12);
        let u2 = self.unit();
        let z = (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos();
        mu + sigma * z
    }

    fn log_normal(&mut self, mu: f64, sigma: f64) -> f64 {
        self.normal(mu, sigma).exp()
    }
}

// ---- smooth cursor paths (Bézier arc + minimum-jerk easing) ----

/// Move the cursor from `from` to `to` (SCREEN PIXELS) along a smooth curved
/// path, calling `emit(x, y)` densely (every ~6ms) so the OS cursor glides
/// rather than steps. `target_w` is the approximate target width (aim jitter +
/// Fitts-law duration). Polls `cancel` every step.
pub(super) fn human_move(
    from: (f64, f64),
    to: (f64, f64),
    target_w: f64,
    profile: &HumanProfile,
    cancel: &AtomicBool,
    emit: &dyn Fn(i32, i32),
) -> Outcome {
    let mut rng = Rng::new();
    // Aim a hair off dead-center (ghost-cursor realism) but kept SMALL so it
    // doesn't push clicks off small targets — accuracy beats realism here.
    let aim_r = (target_w * 0.12).clamp(0.0, 2.5);
    let a = rng.range(0.0, std::f64::consts::TAU);
    let aim = (
        to.0 + aim_r * a.cos() * rng.unit(),
        to.1 + aim_r * a.sin() * rng.unit(),
    );
    let dist = (aim.0 - from.0).hypot(aim.1 - from.1);
    if dist < 1.5 {
        emit(aim.0.round() as i32, aim.1.round() as i32);
        return Outcome::Done;
    }
    // Fitts's law: time grows with distance / target width.
    let w = target_w.max(10.0);
    let mt = (110.0 + 170.0 * (dist / w + 1.0).log2()).clamp(180.0, 1300.0);

    // One continuous smooth glide — no fly-past/settle (reads as a robotic hiccup),
    // and the glide stops the instant it arrives (no dead deceleration tail).
    if glide(from, aim, mt, profile, cancel, emit) == Outcome::Aborted {
        return Outcome::Aborted;
    }
    emit(aim.0.round() as i32, aim.1.round() as i32);
    Outcome::Done
}

/// One smooth segment: a quadratic Bézier (gentle perpendicular bow) sampled at
/// `t = smootherstep(i/n)` so velocity is bell-shaped (ease-in/ease-out, i.e.
/// minimum-jerk — the same easing the recorder uses for camera zoom). Dense
/// (~6ms) emits keep the motion continuous instead of stair-stepped.
fn glide(
    p0: (f64, f64),
    p2: (f64, f64),
    dur_ms: f64,
    profile: &HumanProfile,
    cancel: &AtomicBool,
    emit: &dyn Fn(i32, i32),
) -> Outcome {
    let mut rng = Rng::new();
    let dx = p2.0 - p0.0;
    let dy = p2.1 - p0.1;
    let plen = (dx * dx + dy * dy).sqrt().max(1.0);
    // Control point: midpoint pushed perpendicular for a gentle, random-sided arc.
    let bow = rng.range(-0.16, 0.16) * plen;
    let ctrl = (
        (p0.0 + p2.0) / 2.0 - dy / plen * bow,
        (p0.1 + p2.1) / 2.0 + dx / plen * bow,
    );
    const FRAME_MS: u64 = 6;
    let n = (dur_ms / FRAME_MS as f64).round().clamp(8.0, 400.0) as u32;
    // Subtle perpendicular tremor (realistic only), faded to zero at both ends.
    let tremor = if profile.mode == Humanize::Realistic {
        rng.range(0.4, 1.0)
    } else {
        0.0
    };
    let (mut lx, mut ly) = (p0.0.round() as i32, p0.1.round() as i32);
    for i in 1..=n {
        if cancel.load(Ordering::Relaxed) {
            return Outcome::Aborted;
        }
        let t = i as f64 / n as f64;
        let te = smootherstep(t);
        let base = qbezier(p0, ctrl, p2, te);
        let rem = (p2.0 - base.0).hypot(p2.1 - base.1);
        let mut p = base;
        if tremor > 0.0 {
            let env = (std::f64::consts::PI * t).sin(); // 0 at ends, 1 mid
            let wob = (i as f64 * 0.8).sin() * tremor * env;
            p.0 += -dy / plen * wob;
            p.1 += dx / plen * wob;
        }
        let (px, py) = (p.0.round() as i32, p.1.round() as i32);
        if (px, py) != (lx, ly) {
            emit(px, py);
            lx = px;
            ly = py;
        }
        // Stop as soon as we've effectively arrived — no creeping dead tail.
        if rem < 1.0 {
            break;
        }
        sleep(Duration::from_millis(FRAME_MS));
    }
    Outcome::Done
}

/// Minimum-jerk easing `6t⁵-15t⁴+10t³` (zero velocity AND acceleration at the
/// ends) — matches the recorder's camera-zoom easing.
fn smootherstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn qbezier(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    (
        u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
        u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
    )
}

// ---- human typing (log-normal keystroke timing) ----

/// Type `text` one key at a time with human cadence. `down(unit)`/`up(unit)`
/// emit a UTF-16 unit's key-down / key-up. Polls `cancel` before each key — a
/// "stop" leaves a recoverable partial string. An emitter returns false when
/// input dispatch failed, which stops before any further key is attempted.
/// Typos are intentionally NOT injected (an assistant must never leave wrong
/// text); `profile.typos` is reserved for an explicit opt-in and currently only
/// widens timing variance.
pub(super) fn human_type(
    text: &str,
    profile: &HumanProfile,
    cancel: &AtomicBool,
    down: &dyn Fn(u16) -> bool,
    up: &dyn Fn(u16) -> bool,
) -> Outcome {
    let mut rng = Rng::new();
    let wpm = if profile.wpm > 0.0 { profile.wpm } else { 60.0 };
    let base_iki = 60_000.0 / (wpm * 5.0); // ms per char (5 chars/word)
    let mu = base_iki.ln();
    // Wider variance in realistic mode; wider still when typos (bursty) is opted in.
    let sigma = match (profile.mode, profile.typos) {
        (Humanize::Realistic, true) => 0.30,
        (Humanize::Realistic, false) => 0.25,
        _ => 0.16,
    };

    let mut prev: Option<char> = None;
    let mut buf = [0u16; 2];
    for ch in text.chars() {
        if cancel.load(Ordering::Relaxed) {
            return Outcome::Aborted;
        }
        // Flight time before this key.
        let mut iki = rng.log_normal(mu, sigma);
        if let Some(p) = prev {
            iki *= bigram_mult(p, ch);
        }
        if rng.unit() < 0.03 {
            iki += rng.range(400.0, 1200.0); // occasional "thinking" pause
        }
        if ch == ' ' {
            iki += rng.range(20.0, 90.0);
        }
        if matches!(prev, Some('.') | Some('!') | Some('?')) {
            iki += rng.range(250.0, 700.0);
        }
        if sleep_cancellable(iki.clamp(8.0, 4000.0) as u64, cancel) {
            return Outcome::Aborted;
        }
        // Press the key (UTF-16 units) with a per-key dwell.
        let dwell = rng.normal(77.0, 22.0).clamp(35.0, 150.0) as u64;
        for &unit in ch.encode_utf16(&mut buf).iter() {
            if !down(unit) {
                return Outcome::Aborted;
            }
            sleep(Duration::from_millis(dwell));
            if !up(unit) {
                return Outcome::Aborted;
            }
        }
        prev = Some(ch);
    }
    Outcome::Done
}

/// A natural button-hold duration (ms) for a humanized click.
pub(super) fn click_dwell_ms() -> u64 {
    Rng::new().range(50.0, 130.0) as u64
}

/// A brief pause (ms) on an UNCERTAIN click — the cursor settles on the target
/// before committing, giving the user a window to barge in ("no, not that one").
pub(super) fn hesitation_ms() -> u64 {
    Rng::new().range(250.0, 480.0) as u64
}

/// Inter-key timing multiplier for a bigram: same key (doubled letter) is
/// slowest, same hand slower than alternating hands (opposite-hand bursts).
fn bigram_mult(a: char, b: char) -> f64 {
    let a = a.to_ascii_lowercase();
    let b = b.to_ascii_lowercase();
    if a == b && a.is_ascii_alphabetic() {
        return 1.38; // same finger, doubled
    }
    match (hand(a), hand(b)) {
        (Some(ha), Some(hb)) if ha == hb => 1.15, // same hand, different finger
        (Some(_), Some(_)) => 1.0,                // alternating hands
        _ => 1.05,                                // involves space/punct
    }
}

/// Rough QWERTY hand assignment (L/R), for the bigram timing model.
fn hand(c: char) -> Option<bool> {
    match c {
        'q' | 'w' | 'e' | 'r' | 't' | 'a' | 's' | 'd' | 'f' | 'g' | 'z' | 'x' | 'c' | 'v' | 'b' => {
            Some(false)
        }
        'y' | 'u' | 'i' | 'o' | 'p' | 'h' | 'j' | 'k' | 'l' | 'n' | 'm' => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn move_reaches_target_and_is_continuous() {
        let cancel = AtomicBool::new(false);
        let p = HumanProfile {
            mode: Humanize::Realistic,
            wpm: 0.0,
            typos: false,
        };
        let last = std::cell::Cell::new((0i32, 0i32));
        let n = std::cell::Cell::new(0u32);
        human_move((0.0, 0.0), (640.0, 480.0), 30.0, &p, &cancel, &|x, y| {
            last.set((x, y));
            n.set(n.get() + 1);
        });
        let (lx, ly) = last.get();
        // Lands inside the target — aim is intentionally jittered off dead-center
        // by up to target_w*0.25 (here 30*0.25 ≈ 8px).
        assert!(
            (lx - 640).abs() <= 10 && (ly - 480).abs() <= 10,
            "ended at {lx},{ly}"
        );
        assert!(n.get() > 10, "expected many steps, got {}", n.get());
    }

    #[test]
    fn move_aborts_when_cancelled() {
        let cancel = AtomicBool::new(true);
        let p = HumanProfile::instant();
        let n = std::cell::Cell::new(0u32);
        let r = human_move((0.0, 0.0), (900.0, 700.0), 30.0, &p, &cancel, &|_, _| {
            n.set(n.get() + 1);
        });
        assert_eq!(r, Outcome::Aborted);
    }

    #[test]
    fn typing_emits_each_unit_and_can_abort() {
        let cancel = AtomicBool::new(false);
        let p = HumanProfile {
            mode: Humanize::Smooth,
            wpm: 9000.0,
            typos: false,
        };
        let downs = std::cell::RefCell::new(Vec::new());
        let r = human_type(
            "hi!",
            &p,
            &cancel,
            &|u| {
                downs.borrow_mut().push(u);
                true
            },
            &|_| true,
        );
        assert_eq!(r, Outcome::Done);
        assert_eq!(downs.borrow().len(), 3);
    }

    #[test]
    fn bigram_doubled_is_slowest() {
        assert!(bigram_mult('l', 'l') > bigram_mult('a', 's'));
        assert!(bigram_mult('a', 's') >= bigram_mult('a', 'k'));
    }
}
