// Spring physics functions for cursor path generation.
// Damped harmonic oscillator ODE (analytical solution), smooth-damp, angle helpers.

pub(super) struct SpringResult {
    pub value: f64,
    pub velocity: f64,
}

/// Analytical damped spring step (exact solution of damped harmonic oscillator ODE).
pub(super) fn spring_step_scalar(
    current: f64,
    target: f64,
    velocity: f64,
    angular_freq: f64,
    damping_ratio: f64,
    dt: f64,
) -> SpringResult {
    let disp = current - target;
    if disp.abs() < 1e-8 && velocity.abs() < 1e-8 {
        return SpringResult {
            value: target,
            velocity: 0.0,
        };
    }

    let omega = angular_freq;
    let zeta = damping_ratio;

    let (new_disp, new_vel) = if zeta < 1.0 - 1e-6 {
        // Underdamped — oscillatory
        let alpha = omega * (1.0 - zeta * zeta).sqrt();
        let decay = (-omega * zeta * dt).exp();
        let cos_a = (alpha * dt).cos();
        let sin_a = (alpha * dt).sin();
        let nd = decay * (disp * cos_a + ((velocity + omega * zeta * disp) / alpha) * sin_a);
        let nv = decay
            * (velocity * cos_a
                - ((velocity * zeta * omega + omega * omega * disp) / alpha) * sin_a);
        (nd, nv)
    } else if zeta > 1.0 + 1e-6 {
        // Overdamped — exponential decay
        let disc = (zeta * zeta - 1.0).sqrt();
        let s1 = -omega * (zeta - disc);
        let s2 = -omega * (zeta + disc);
        let c2 = (velocity - s1 * disp) / (s2 - s1);
        let c1 = disp - c2;
        let e1 = (s1 * dt).exp();
        let e2 = (s2 * dt).exp();
        (c1 * e1 + c2 * e2, c1 * s1 * e1 + c2 * s2 * e2)
    } else {
        // Critically damped
        let decay = (-omega * dt).exp();
        let nd = (disp + (velocity + omega * disp) * dt) * decay;
        let nv = (velocity - (velocity + omega * disp) * omega * dt) * decay;
        (nd, nv)
    };

    SpringResult {
        value: target + new_disp,
        velocity: new_vel,
    }
}

pub(super) fn normalize_angle(angle: f64) -> f64 {
    let mut a = angle;
    while a > std::f64::consts::PI {
        a -= std::f64::consts::TAU;
    }
    while a < -std::f64::consts::PI {
        a += std::f64::consts::TAU;
    }
    a
}

pub(super) fn spring_step_angle(
    current: f64,
    target: f64,
    velocity: f64,
    angular_freq: f64,
    damping_ratio: f64,
    dt: f64,
) -> SpringResult {
    let adjusted_target = current + normalize_angle(target - current);
    spring_step_scalar(
        current,
        adjusted_target,
        velocity,
        angular_freq,
        damping_ratio,
        dt,
    )
}

/// Smooth-damp scalar (Spring-Damper, Unity-style). Used for heading smoothing.
pub(super) fn smooth_damp_scalar(
    current: f64,
    target: f64,
    velocity: f64,
    smooth_time: f64,
    max_speed: f64,
    dt: f64,
) -> SpringResult {
    let safe_t = smooth_time.max(0.0001);
    let omega = 2.0 / safe_t;
    let x = omega * dt;
    let exp = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);

    let mut change = current - target;
    let original_target = target;
    let max_change = max_speed * safe_t;
    change = change.clamp(-max_change, max_change);
    let adj_target = current - change;

    let temp = (velocity + omega * change) * dt;
    let mut new_velocity = (velocity - omega * temp) * exp;
    let mut output = adj_target + (change + temp) * exp;

    if (original_target - current > 0.0) == (output > original_target) {
        output = original_target;
        // WYSIWYG drift fix: zero the velocity on the overshoot clamp, matching
        // cursorDynamics.ts (the preview) and canonical Unity SmoothDamp. After the
        // clamp output == original_target, so (output - original_target)/dt == 0 — the
        // spring must arrive at rest, not carry the pre-clamp velocity forward.
        new_velocity = (output - original_target) / dt.max(0.0001);
    }

    SpringResult {
        value: output,
        velocity: new_velocity,
    }
}

pub(super) fn smooth_damp_angle(
    current: f64,
    target: f64,
    velocity: f64,
    smooth_time: f64,
    max_speed: f64,
    dt: f64,
) -> SpringResult {
    let adjusted_target = current + normalize_angle(target - current);
    smooth_damp_scalar(
        current,
        adjusted_target,
        velocity,
        smooth_time,
        max_speed,
        dt,
    )
}

pub(super) fn lerp_angle(from: f64, to: f64, t: f64) -> f64 {
    normalize_angle(from + normalize_angle(to - from) * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    // Cross-language render-math golden. The TS preview (cursorDynamics.ts) is
    // canonical; this Rust export must reproduce the SAME fixture within 1e-6.
    // Regenerate via screen-record/tests/unit/_generateRenderGolden.gen.ts.
    // See .claude/parity/render-camera-cursor.md.
    const GOLDEN: &str =
        include_str!("../../../../../parity-fixtures/render-camera-cursor/golden.json");

    fn golden() -> serde_json::Value {
        serde_json::from_str(GOLDEN).expect("golden fixture parses")
    }

    fn tol() -> f64 {
        golden()["tolerance"].as_f64().unwrap()
    }

    // ── Pure-helper behavior ──────────────────────────────────────────────────

    #[test]
    fn normalize_angle_wraps_into_pi_range() {
        for a in [4.0, -4.0, 7.5, -7.5, 3.2, -3.2, 12.0, -12.0] {
            let n = normalize_angle(a);
            assert!(
                (-PI - 1e-9..=PI + 1e-9).contains(&n),
                "{a} -> {n} out of range"
            );
            // Differs from the original by a whole number of TAU turns.
            let turns = (a - n) / std::f64::consts::TAU;
            assert!((turns - turns.round()).abs() < 1e-9);
        }
        assert_eq!(normalize_angle(0.0), 0.0);
    }

    #[test]
    fn spring_short_circuits_at_rest() {
        let r = spring_step_scalar(5.0, 5.0, 0.0, 30.0, 1.0, 1.0 / 120.0);
        assert_eq!(r.value, 5.0);
        assert_eq!(r.velocity, 0.0);
    }

    #[test]
    fn critically_damped_spring_settles_without_overshoot() {
        let (mut v, mut vel, target) = (0.0_f64, 0.0_f64, 100.0_f64);
        for _ in 0..600 {
            let r = spring_step_scalar(v, target, vel, 30.0, 1.0, 1.0 / 120.0);
            v = r.value;
            vel = r.velocity;
            assert!(v <= target + 1e-6, "critical spring overshot: {v}");
        }
        assert!((v - target).abs() < 1e-2);
        assert!(vel.abs() < 1e-2);
    }

    #[test]
    fn underdamped_spring_overshoots_then_settles() {
        let (mut v, mut vel, target) = (0.0_f64, 0.0_f64, 100.0_f64);
        let mut overshot = false;
        for _ in 0..600 {
            let r = spring_step_scalar(v, target, vel, 30.0, 0.4, 1.0 / 120.0);
            v = r.value;
            vel = r.velocity;
            if v > target {
                overshot = true;
            }
        }
        assert!(overshot, "underdamped spring should overshoot (the wiggle)");
        assert!((v - target).abs() < 1e-2);
    }

    #[test]
    fn smooth_damp_clamp_branch_zeros_velocity() {
        // Large inbound velocity overshoots analytically -> clamp to target with
        // zero velocity. This is the WYSIWYG drift fix: before the fix Rust
        // carried the pre-clamp velocity forward, diverging from the TS preview.
        let r = smooth_damp_scalar(0.0, 10.0, 5000.0, 0.5, 1e9, 1.0 / 60.0);
        assert!((r.value - 10.0).abs() < 1e-9);
        assert_eq!(
            r.velocity, 0.0,
            "velocity must be zeroed on the clamp branch"
        );
    }

    // ── Cross-language golden ─────────────────────────────────────────────────

    #[test]
    fn normalize_angle_matches_golden() {
        let g = golden();
        for c in g["cursorPrimitives"]["normalizeAngle"].as_array().unwrap() {
            let got = normalize_angle(c["angle"].as_f64().unwrap());
            assert!((got - c["value"].as_f64().unwrap()).abs() <= tol());
        }
    }

    #[test]
    fn lerp_angle_matches_golden() {
        let g = golden();
        for c in g["cursorPrimitives"]["lerpAngle"].as_array().unwrap() {
            let got = lerp_angle(
                c["from"].as_f64().unwrap(),
                c["to"].as_f64().unwrap(),
                c["t"].as_f64().unwrap(),
            );
            assert!((got - c["value"].as_f64().unwrap()).abs() <= tol());
        }
    }

    #[test]
    fn smooth_damp_scalar_matches_golden() {
        let g = golden();
        for key in ["settle", "overshootClamp"] {
            let run = &g["cursorPrimitives"]["smoothDampScalar"][key];
            let target = run["target"].as_f64().unwrap();
            let smooth_time = run["smoothTime"].as_f64().unwrap();
            let max_speed = run["maxSpeed"].as_f64().unwrap();
            let dt = run["dt"].as_f64().unwrap();
            let mut value = run["start"].as_f64().unwrap();
            let mut velocity = run["initialVelocity"].as_f64().unwrap();
            for step in run["steps"].as_array().unwrap() {
                let r = smooth_damp_scalar(value, target, velocity, smooth_time, max_speed, dt);
                value = r.value;
                velocity = r.velocity;
                assert!(
                    (value - step["value"].as_f64().unwrap()).abs() <= tol(),
                    "{key} value drift at step {}",
                    step["step"]
                );
                assert!(
                    (velocity - step["velocity"].as_f64().unwrap()).abs() <= tol(),
                    "{key} velocity drift at step {}",
                    step["step"]
                );
            }
        }
    }

    #[test]
    fn spring_step_scalar_matches_golden() {
        let g = golden();
        for cfg in g["cursorPrimitives"]["springStepScalar"]
            .as_array()
            .unwrap()
        {
            let target = cfg["target"].as_f64().unwrap();
            let omega = cfg["omega"].as_f64().unwrap();
            let zeta = cfg["zeta"].as_f64().unwrap();
            let dt = cfg["dt"].as_f64().unwrap();
            let mut value = 0.0_f64;
            let mut velocity = 0.0_f64;
            for step in cfg["trajectory"].as_array().unwrap() {
                let r = spring_step_scalar(value, target, velocity, omega, zeta, dt);
                value = r.value;
                velocity = r.velocity;
                assert!((value - step["value"].as_f64().unwrap()).abs() <= tol());
                assert!((velocity - step["velocity"].as_f64().unwrap()).abs() <= tol());
            }
        }
    }

    #[test]
    fn spring_step_angle_matches_golden() {
        let g = golden();
        let cfg = &g["cursorPrimitives"]["springStepAngle"];
        let target = cfg["target"].as_f64().unwrap();
        let omega = cfg["omega"].as_f64().unwrap();
        let zeta = cfg["zeta"].as_f64().unwrap();
        let dt = cfg["dt"].as_f64().unwrap();
        let mut value = cfg["from"].as_f64().unwrap();
        let mut velocity = 0.0_f64;
        for step in cfg["trajectory"].as_array().unwrap() {
            let r = spring_step_angle(value, target, velocity, omega, zeta, dt);
            value = r.value;
            velocity = r.velocity;
            assert!((value - step["value"].as_f64().unwrap()).abs() <= tol());
            assert!((velocity - step["velocity"].as_f64().unwrap()).abs() <= tol());
        }
    }
}
