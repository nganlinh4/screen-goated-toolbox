import { BackgroundConfig, MousePosition } from '@/types/video';

// --- CONFIGURATION ---
// Default pointer movement delay (seconds)
export const DEFAULT_CURSOR_OFFSET_SEC = 0;
export const DEFAULT_CURSOR_WIGGLE_STRENGTH = 0.30;
export const DEFAULT_CURSOR_WIGGLE_DAMPING = 0.55;
export const DEFAULT_CURSOR_WIGGLE_RESPONSE = 6.5;

// =====================================================================
// Config getters
// =====================================================================

export function getCursorMovementDelaySec(backgroundConfig?: BackgroundConfig | null): number {
  const raw = backgroundConfig?.cursorMovementDelay;
  if (raw === undefined || Number.isNaN(raw)) return DEFAULT_CURSOR_OFFSET_SEC;
  return Math.max(-0.5, Math.min(0.5, raw));
}

export function getCursorSmoothness(backgroundConfig?: BackgroundConfig | null): number {
  const raw = backgroundConfig?.cursorSmoothness;
  if (raw === undefined || Number.isNaN(raw)) return 5;
  return Math.max(0, Math.min(10, raw));
}

export function getCursorShadowStrength(backgroundConfig?: BackgroundConfig | null): number {
  const raw = backgroundConfig?.cursorShadow;
  if (raw === undefined || Number.isNaN(raw)) return 35;
  return Math.max(0, Math.min(200, raw));
}

export function getCursorWiggleStrength(backgroundConfig?: BackgroundConfig | null): number {
  const raw = backgroundConfig?.cursorWiggleStrength;
  if (raw === undefined || Number.isNaN(raw)) return DEFAULT_CURSOR_WIGGLE_STRENGTH;
  return Math.max(0, Math.min(1, raw));
}

export function getCursorWiggleDamping(backgroundConfig?: BackgroundConfig | null): number {
  const raw = backgroundConfig?.cursorWiggleDamping;
  if (raw === undefined || Number.isNaN(raw)) return DEFAULT_CURSOR_WIGGLE_DAMPING;
  return Math.max(0.35, Math.min(0.98, raw));
}

export function getCursorWiggleResponse(backgroundConfig?: BackgroundConfig | null): number {
  const raw = backgroundConfig?.cursorWiggleResponse;
  if (raw === undefined || Number.isNaN(raw)) return DEFAULT_CURSOR_WIGGLE_RESPONSE;
  return Math.max(2, Math.min(12, raw));
}

export function getCursorTiltAngleRad(backgroundConfig?: BackgroundConfig | null): number {
  return (backgroundConfig?.cursorTiltAngle ?? -10) * (Math.PI / 180);
}

export function getCursorProcessingSignature(backgroundConfig?: BackgroundConfig | null): string {
  return [
    getCursorSmoothness(backgroundConfig).toFixed(2),
    getCursorWiggleStrength(backgroundConfig).toFixed(2),
    getCursorWiggleDamping(backgroundConfig).toFixed(2),
    getCursorWiggleResponse(backgroundConfig).toFixed(2),
    getCursorTiltAngleRad(backgroundConfig).toFixed(4),
  ].join('|');
}

// =====================================================================
// Math utilities
// =====================================================================

export function catmullRomInterpolate(p0: number, p1: number, p2: number, p3: number, t: number): number {
  const t2 = t * t;
  const t3 = t2 * t;
  return 0.5 * (
    (2 * p1) +
    (-p0 + p2) * t +
    (2 * p0 - 5 * p1 + 4 * p2 - p3) * t2 +
    (-p0 + 3 * p1 - 3 * p2 + p3) * t3
  );
}

export function normalizeAngleRad(angle: number): number {
  let a = angle;
  while (a > Math.PI) a -= Math.PI * 2;
  while (a < -Math.PI) a += Math.PI * 2;
  return a;
}

export function lerpAngleRad(from: number, to: number, t: number): number {
  const delta = normalizeAngleRad(to - from);
  return normalizeAngleRad(from + delta * t);
}

export function smoothDampScalar(
  current: number,
  target: number,
  velocity: number,
  smoothTime: number,
  maxSpeed: number,
  deltaTime: number
): { value: number; velocity: number } {
  const safeSmoothTime = Math.max(0.0001, smoothTime);
  const omega = 2 / safeSmoothTime;
  const x = omega * deltaTime;
  const exp = 1 / (1 + x + (0.48 * x * x) + (0.235 * x * x * x));

  let change = current - target;
  const originalTarget = target;
  const maxChange = maxSpeed * safeSmoothTime;
  change = Math.max(-maxChange, Math.min(maxChange, change));
  target = current - change;

  const temp = (velocity + omega * change) * deltaTime;
  let newVelocity = (velocity - omega * temp) * exp;
  let output = target + (change + temp) * exp;

  if ((originalTarget - current > 0) === (output > originalTarget)) {
    output = originalTarget;
    newVelocity = (output - originalTarget) / Math.max(deltaTime, 0.0001);
  }

  return { value: output, velocity: newVelocity };
}

export function smoothDampAngleRad(
  current: number,
  target: number,
  velocity: number,
  smoothTime: number,
  maxSpeed: number,
  deltaTime: number
): { value: number; velocity: number } {
  const adjustedTarget = current + normalizeAngleRad(target - current);
  return smoothDampScalar(current, adjustedTarget, velocity, smoothTime, maxSpeed, deltaTime);
}

/**
 * Analytical damped spring step for scalar values.
 * Exact solution of the damped harmonic oscillator ODE -- frame-rate independent.
 * Supports underdamped (zeta<1, bouncy), critically damped (zeta=1), overdamped (zeta>1).
 */
export function springStepScalar(
  current: number,
  target: number,
  velocity: number,
  angularFreq: number,
  dampingRatio: number,
  dt: number
): { value: number; velocity: number } {
  const disp = current - target;

  if (Math.abs(disp) < 1e-8 && Math.abs(velocity) < 1e-8) {
    return { value: target, velocity: 0 };
  }

  const omega = angularFreq;
  const zeta = dampingRatio;
  let newDisp: number;
  let newVel: number;

  if (zeta < 1.0 - 1e-6) {
    // Underdamped -- oscillatory with exponential decay (the wiggle)
    const alpha = omega * Math.sqrt(1 - zeta * zeta);
    const decay = Math.exp(-omega * zeta * dt);
    const cosA = Math.cos(alpha * dt);
    const sinA = Math.sin(alpha * dt);

    newDisp = decay * (
      disp * cosA +
      ((velocity + omega * zeta * disp) / alpha) * sinA
    );
    newVel = decay * (
      velocity * cosA -
      ((velocity * zeta * omega + omega * omega * disp) / alpha) * sinA
    );
  } else if (zeta > 1.0 + 1e-6) {
    // Overdamped -- exponential decay without oscillation
    const disc = Math.sqrt(zeta * zeta - 1);
    const s1 = -omega * (zeta - disc);
    const s2 = -omega * (zeta + disc);
    const c2 = (velocity - s1 * disp) / (s2 - s1);
    const c1 = disp - c2;
    const e1 = Math.exp(s1 * dt);
    const e2 = Math.exp(s2 * dt);

    newDisp = c1 * e1 + c2 * e2;
    newVel = c1 * s1 * e1 + c2 * s2 * e2;
  } else {
    // Critically damped -- fastest non-oscillatory settling
    const decay = Math.exp(-omega * dt);
    newDisp = (disp + (velocity + omega * disp) * dt) * decay;
    newVel = (velocity - (velocity + omega * disp) * omega * dt) * decay;
  }

  return { value: target + newDisp, velocity: newVel };
}

/** Spring step for angle values -- normalizes angle then delegates to scalar solver. */
export function springStepAngle(
  current: number,
  target: number,
  velocity: number,
  angularFreq: number,
  dampingRatio: number,
  dt: number
): { value: number; velocity: number } {
  const adjustedTarget = current + normalizeAngleRad(target - current);
  return springStepScalar(current, adjustedTarget, velocity, angularFreq, dampingRatio, dt);
}

// =====================================================================
// Cursor processing pipeline
// =====================================================================

export function smoothMousePositions(
  positions: MousePosition[],
  targetFps: number = 120,
  backgroundConfig?: BackgroundConfig | null
): MousePosition[] {
  if (positions.length < 4) return positions;
  const smoothed: MousePosition[] = [];

  for (let i = 0; i < positions.length - 3; i++) {
    const p0 = positions[i];
    const p1 = positions[i + 1];
    const p2 = positions[i + 2];
    const p3 = positions[i + 3];

    // Skip dense interpolation for static idle segments to avoid O(N) bloat.
    const dist = Math.hypot(p2.x - p1.x, p2.y - p1.y);
    if (dist < 2 && p1.isClicked === p2.isClicked && p1.cursor_type === p2.cursor_type) {
      smoothed.push({ ...p1 });
      continue;
    }

    const segmentDuration = p2.timestamp - p1.timestamp;
    const numFrames = Math.min(Math.ceil(segmentDuration * targetFps), 60);

    for (let frame = 0; frame < numFrames; frame++) {
      const t = frame / numFrames;
      const timestamp = p1.timestamp + (segmentDuration * t);
      const x = catmullRomInterpolate(p0.x, p1.x, p2.x, p3.x, t);
      const y = catmullRomInterpolate(p0.y, p1.y, p2.y, p3.y, t);
      const isClicked = Boolean(p1.isClicked || p2.isClicked);
      const cursor_type = t < 0.5 ? p1.cursor_type : p2.cursor_type;
      smoothed.push({ x, y, timestamp, isClicked, cursor_type });
    }
  }

  const windowSize = (getCursorSmoothness(backgroundConfig) * 2) + 1;
  const passes = 3; // 3-pass box blur approximates a Gaussian kernel in O(N) total
  let currentSmoothed = smoothed;

  // O(N) sliding-window box blur: proper symmetric window accumulation
  for (let pass = 0; pass < passes; pass++) {
    const n = currentSmoothed.length;
    const passSmoothed: MousePosition[] = new Array(n);
    const half = Math.floor(windowSize / 2);

    let runX = 0, runY = 0;
    let winStart = 0;
    let winEnd = Math.min(half, n - 1);

    // Initialize the running sum for the first window centered at i=0
    for (let i = 0; i <= winEnd; i++) {
      runX += currentSmoothed[i].x;
      runY += currentSmoothed[i].y;
    }

    for (let i = 0; i < n; i++) {
      const targetStart = Math.max(0, i - half);
      const targetEnd = Math.min(n - 1, i + half);

      while (winEnd < targetEnd) {
        winEnd++;
        runX += currentSmoothed[winEnd].x;
        runY += currentSmoothed[winEnd].y;
      }
      while (winStart < targetStart) {
        runX -= currentSmoothed[winStart].x;
        runY -= currentSmoothed[winStart].y;
        winStart++;
      }

      const winLen = winEnd - winStart + 1;
      passSmoothed[i] = {
        x: runX / winLen,
        y: runY / winLen,
        timestamp: currentSmoothed[i].timestamp,
        isClicked: currentSmoothed[i].isClicked,
        cursor_type: currentSmoothed[i].cursor_type,
      };
    }
    currentSmoothed = passSmoothed;
  }

  const threshold = 0.5 / (windowSize / 2);
  let lastSignificantPos = currentSmoothed[0];
  const finalSmoothed = [lastSignificantPos];

  for (let i = 1; i < currentSmoothed.length; i++) {
    const current = currentSmoothed[i];
    const distance = Math.hypot(current.x - lastSignificantPos.x, current.y - lastSignificantPos.y);

    if (distance > threshold || current.isClicked !== lastSignificantPos.isClicked) {
      finalSmoothed.push(current);
      lastSignificantPos = current;
    } else {
      finalSmoothed.push({
        ...lastSignificantPos,
        timestamp: current.timestamp
      });
    }
  }

  return finalSmoothed;
}

export function processCursorPositions(
  positions: MousePosition[],
  backgroundConfig?: BackgroundConfig | null
): MousePosition[] {
  const smoothed = smoothMousePositions(positions, 120, backgroundConfig);
  const springed = applySpringPositionDynamics(smoothed, backgroundConfig);
  const wiggled = applyAdaptiveCursorWiggle(springed, backgroundConfig);
  return applyCursorTiltOffset(wiggled, backgroundConfig);
}

/** Only asymmetric pointer-like cursors get a static tilt offset.
 *  Text beam, crosshair, resize handles etc. are symmetric and stay upright. */
export function shouldCursorTilt(cursorType: string): boolean {
  const t = cursorType.toLowerCase();
  return t.startsWith('default') || t.startsWith('pointer');
}

/** Adds a static angular offset (resting tilt) to cursor rotation. */
export function applyCursorTiltOffset(
  positions: MousePosition[],
  backgroundConfig?: BackgroundConfig | null
): MousePosition[] {
  const tiltRad = getCursorTiltAngleRad(backgroundConfig);
  if (Math.abs(tiltRad) < 0.0001) return positions;
  return positions.map(pos => ({
    ...pos,
    cursor_rotation: (pos.cursor_rotation || 0) +
      (shouldCursorTilt(pos.cursor_type || 'default') ? tiltRad : 0),
  }));
}

/**
 * Spring-based cursor position dynamics -- adds physical inertia to cursor movement.
 * The cursor trails behind during fast movement and slightly overshoots on stop,
 * creating the "alive" cinematic feel used by Screen Studio.
 * Runs BEFORE rotation wiggle so tilt is computed from spring-smoothed velocities.
 */
export function applySpringPositionDynamics(
  positions: MousePosition[],
  backgroundConfig?: BackgroundConfig | null
): MousePosition[] {
  if (positions.length < 2) return positions;

  const strength = getCursorWiggleStrength(backgroundConfig);
  if (strength <= 0.001) return positions;

  const dampingRatio = getCursorWiggleDamping(backgroundConfig);
  const responseHz = getCursorWiggleResponse(backgroundConfig);

  // Position spring: stiffer at low strength (subtle), looser at high (dramatic)
  const baseOmega = 2 * Math.PI * responseHz;
  const posOmega = baseOmega * (4.0 - strength * 2.5);
  // More damped than rotation spring -- position overshoot should be very subtle
  const posZeta = Math.min(0.92, dampingRatio + 0.18);
  // Max displacement cap prevents extreme lag at very fast mouse speeds
  const maxDisp = 8 + strength * 28;

  const result: MousePosition[] = [];
  let sx = positions[0].x;
  let sy = positions[0].y;
  let vx = 0;
  let vy = 0;

  result.push({ ...positions[0] });

  for (let i = 1; i < positions.length; i++) {
    const prev = positions[i - 1];
    const target = positions[i];
    const dt = Math.max(1 / 1000, target.timestamp - prev.timestamp);

    const stepX = springStepScalar(sx, target.x, vx, posOmega, posZeta, dt);
    const stepY = springStepScalar(sy, target.y, vy, posOmega, posZeta, dt);

    sx = stepX.value;
    sy = stepY.value;
    vx = stepX.velocity;
    vy = stepY.velocity;

    // Clamp displacement to prevent excessive trailing at extreme speeds
    const dx = sx - target.x;
    const dy = sy - target.y;
    const dist = Math.hypot(dx, dy);
    if (dist > maxDisp) {
      const ratio = maxDisp / dist;
      sx = target.x + dx * ratio;
      sy = target.y + dy * ratio;
      vx *= ratio;
      vy *= ratio;
    }

    result.push({
      ...target,
      x: sx,
      y: sy,
    });
  }

  return result;
}

export function applyAdaptiveCursorWiggle(
  positions: MousePosition[],
  backgroundConfig?: BackgroundConfig | null
): MousePosition[] {
  if (positions.length < 2) return positions;

  const strength = getCursorWiggleStrength(backgroundConfig);
  if (strength <= 0.001) return positions;

  const dampingRatio = getCursorWiggleDamping(backgroundConfig);
  const responseHz = getCursorWiggleResponse(backgroundConfig);

  const result: MousePosition[] = [];
  let lagHeading = 0;
  let lagHeadingVel = 0;
  let hasHeading = false;
  let cursorRotation = 0;
  let cursorRotationVel = 0;

  // Derive physics params from user-facing knobs
  const maxTiltRad = (2.2 + strength * 8.8) * (Math.PI / 180);
  const headingSmoothTime = 0.28 - strength * 0.17;
  const tiltGain = 0.33 + strength * 0.92;
  const speedStart = 120;
  const speedFull = 1650;

  // Underdamped spring params for the rotation channel
  // omega = natural angular frequency; zeta = damping ratio (<1 -> bounce)
  const rotationOmega = 2 * Math.PI * responseHz;
  const rotationZeta = dampingRatio;

  result.push({ ...positions[0], cursor_rotation: 0 });

  for (let i = 1; i < positions.length; i++) {
    const prevTarget = positions[i - 1];
    const target = positions[i];
    const dtRaw = Math.max(1 / 1000, target.timestamp - prevTarget.timestamp);

    const targetVx = (target.x - prevTarget.x) / dtRaw;
    const targetVy = (target.y - prevTarget.y) / dtRaw;
    const speed = Math.hypot(targetVx, targetVy);

    let tiltTarget = 0;

    if (speed > speedStart) {
      const heading = Math.atan2(targetVy, targetVx);
      if (!hasHeading) {
        lagHeading = heading;
        hasHeading = true;
      }

      const headingStep = smoothDampAngleRad(
        lagHeading, heading, lagHeadingVel,
        headingSmoothTime, 18, dtRaw
      );
      lagHeading = headingStep.value;
      lagHeadingVel = headingStep.velocity;

      // SmoothStep speed fade (less abrupt than linear)
      const t = Math.max(0, Math.min(1, (speed - speedStart) / (speedFull - speedStart)));
      const speedFade = t * t * (3 - 2 * t);

      const rawTilt = normalizeAngleRad(heading - lagHeading) * tiltGain * speedFade;
      tiltTarget = Math.max(-maxTiltRad, Math.min(maxTiltRad, rawTilt));
    }
    // else: tiltTarget stays 0 -> spring settles with bounce

    // Underdamped spring drives rotation toward tiltTarget
    // During movement: tracks smoothly (target changes gradually)
    // On stop: target jumps to 0 -> spring overshoots -> wiggle
    const rotStep = springStepAngle(
      cursorRotation, tiltTarget, cursorRotationVel,
      rotationOmega, rotationZeta, dtRaw
    );
    cursorRotation = rotStep.value;
    cursorRotationVel = rotStep.velocity;

    result.push({
      ...target,
      x: target.x,
      y: target.y,
      cursor_rotation: cursorRotation,
    });
  }

  return result;
}

export function interpolateCursorPositionInternal(
  currentTime: number,
  positions: MousePosition[],
): { x: number; y: number; isClicked: boolean; cursor_type: string; cursor_rotation?: number } | null {
  if (!positions || positions.length === 0) return null;

  const exactMatch = positions.find((pos: MousePosition) => Math.abs(pos.timestamp - currentTime) < 0.001);
  if (exactMatch) {
    return {
      x: exactMatch.x,
      y: exactMatch.y,
      isClicked: Boolean(exactMatch.isClicked),
      cursor_type: exactMatch.cursor_type || 'default',
      cursor_rotation: exactMatch.cursor_rotation || 0,
    };
  }

  const nextIndex = positions.findIndex((pos: MousePosition) => pos.timestamp > currentTime);
  if (nextIndex === -1) {
    const last = positions[positions.length - 1];
    return {
      x: last.x,
      y: last.y,
      isClicked: Boolean(last.isClicked),
      cursor_type: last.cursor_type || 'default',
      cursor_rotation: last.cursor_rotation || 0,
    };
  }

  if (nextIndex === 0) {
    const first = positions[0];
    return {
      x: first.x,
      y: first.y,
      isClicked: Boolean(first.isClicked),
      cursor_type: first.cursor_type || 'default',
      cursor_rotation: first.cursor_rotation || 0,
    };
  }

  const prev = positions[nextIndex - 1];
  const next = positions[nextIndex];
  const t = (currentTime - prev.timestamp) / (next.timestamp - prev.timestamp);

  return {
    x: prev.x + (next.x - prev.x) * t,
    y: prev.y + (next.y - prev.y) * t,
    isClicked: Boolean(prev.isClicked || next.isClicked),
    cursor_type: next.cursor_type || 'default',
    cursor_rotation: lerpAngleRad(prev.cursor_rotation || 0, next.cursor_rotation || 0, t),
  };
}

// =====================================================================
// Cursor type helpers
// =====================================================================

/** Arrow, pointing hand, and text cursors rotate. Grip cursors (grab, grabbing) stay upright. */
export function shouldCursorRotate(cursorType: string): boolean {
  const t = cursorType.toLowerCase();
  return t.startsWith('default-') || t.startsWith('pointer-') || t.startsWith('text-');
}

export function getCursorRotationPivot(cursorType: string): { x: number; y: number } {
  switch (cursorType.toLowerCase()) {
    case 'pointer-screenstudio':
    case 'openhand-screenstudio':
    case 'closehand-screenstudio':
    case 'pointer-macos26':
    case 'openhand-macos26':
    case 'closehand-macos26':
    case 'pointer-sgtcute':
    case 'openhand-sgtcute':
    case 'closehand-sgtcute':
    case 'pointer-sgtcool':
    case 'openhand-sgtcool':
    case 'closehand-sgtcool':
    case 'pointer-sgtai':
    case 'openhand-sgtai':
    case 'closehand-sgtai':
    case 'pointer-sgtpixel':
    case 'openhand-sgtpixel':
    case 'closehand-sgtpixel':
    case 'pointer-jepriwin11':
    case 'openhand-jepriwin11':
    case 'closehand-jepriwin11':
    case 'pointer-sgtwatermelon':
    case 'openhand-sgtwatermelon':
    case 'closehand-sgtwatermelon':
      return { x: 3.0, y: 8.5 };
    case 'text-screenstudio':
    case 'text-macos26':
    case 'text-sgtcute':
    case 'text-sgtcool':
    case 'text-sgtai':
    case 'text-sgtpixel':
    case 'text-jepriwin11':
    case 'text-sgtwatermelon':
      return { x: 0, y: 0 };
    default:
      return { x: 3.6, y: 5.6 };
  }
}
