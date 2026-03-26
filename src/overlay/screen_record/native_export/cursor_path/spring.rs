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
    let new_velocity = (velocity - omega * temp) * exp;
    let mut output = adj_target + (change + temp) * exp;

    if (original_target - current > 0.0) == (output > original_target) {
        output = original_target;
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
