import { VideoSegment, MousePosition, ZoomKeyframe, AutoZoomConfig, DEFAULT_AUTO_ZOOM_CONFIG } from '@/types/video';

// Default physics — derived from DEFAULT_AUTO_ZOOM_CONFIG (followTightness=0.5, zoomLevel=2.0, speedSensitivity=0.5)
const FIXED_MASS = 2.0;

/** Map user-facing AutoZoomConfig → internal physics constants. */
function resolvePhysics(cfg: AutoZoomConfig) {
  // followTightness 0–1 → ω₀ (natural frequency) via log interpolation
  // 0.0 → ω₀=3 (~1.0s settling), 0.5 → ω₀≈8 (~0.38s), 1.0 → ω₀=20 (~0.15s)
  const omegaMin = 3.0;
  const omegaMax = 20.0;
  const omega = omegaMin * Math.pow(omegaMax / omegaMin, cfg.followTightness);
  const tension = omega * omega * FIXED_MASS;
  const friction = 2.0 * omega * FIXED_MASS; // critically damped

  // Look-ahead scales inversely with tightness: tight tracking needs less prediction
  const lookAheadMax = 0.35 * (1.0 - cfg.followTightness * 0.7); // 0.35s → 0.105s
  const lookAheadScale = 700 - 400 * cfg.followTightness;         // 700 → 300

  // speedSensitivity 0–1 → velocity zoom penalty threshold (inverse)
  // 0.0 (no sensitivity) → very high threshold (5000 — almost never zoom out)
  // 1.0 (max sensitivity) → low threshold (800 — zoom out easily)
  const maxVelocityZoomPenalty = 5000 - 4200 * cfg.speedSensitivity;

  return {
    TENSION: tension,
    FRICTION: friction,
    MASS: FIXED_MASS,
    LOOK_AHEAD_MAX: lookAheadMax,
    LOOK_AHEAD_SCALE: lookAheadScale,
    MAX_VELOCITY_ZOOM_PENALTY: maxVelocityZoomPenalty,
    BASE_ZOOM: cfg.zoomLevel,
    MIN_ZOOM: 1.0,
    MAX_ZOOM: cfg.zoomLevel,
  };
}


// --- Binary search: find first index where data[i].timestamp >= t ---
function lowerBound(data: MousePosition[], t: number): number {
  let lo = 0, hi = data.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (data[mid].timestamp < t) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

// --- Interpolate position at time t using binary search (O(log n)) ---
function sampleAt(data: MousePosition[], t: number): { x: number; y: number } {
  if (t <= data[0].timestamp) return { x: data[0].x, y: data[0].y };
  if (t >= data[data.length - 1].timestamp) {
    const last = data[data.length - 1];
    return { x: last.x, y: last.y };
  }
  const idx = lowerBound(data, t);
  if (idx === 0) return { x: data[0].x, y: data[0].y };
  const p1 = data[idx - 1];
  const p2 = data[idx];
  const span = p2.timestamp - p1.timestamp;
  const ratio = span > 0 ? (t - p1.timestamp) / span : 0;
  return {
    x: p1.x + (p2.x - p1.x) * ratio,
    y: p1.y + (p2.y - p1.y) * ratio,
  };
}

export class AutoZoomGenerator {

  generateMotionPath(
    segment: VideoSegment,
    mousePositions: MousePosition[],
    videoWidth: number,
    videoHeight: number,
    config?: AutoZoomConfig
  ): { time: number; x: number; y: number; zoom: number }[] {

    const t0 = performance.now();
    const P = resolvePhysics(config ?? DEFAULT_AUTO_ZOOM_CONFIG);

    // 0. Filter and Sort Data
    const data = mousePositions
      .filter(p => p.timestamp >= segment.trimStart - 1.0 && p.timestamp <= segment.trimEnd + 1.0)
      .sort((a, b) => a.timestamp - b.timestamp);

    if (data.length < 2) return [];

    // Pre-index click timestamps for O(log n) click checking
    const clickTimes: number[] = [];
    for (let i = 0; i < data.length; i++) {
      if (data[i].isClicked) clickTimes.push(data[i].timestamp);
    }

    // 1. Initialize Simulation
    const dt = 1 / 60;
    const totalFrames = Math.ceil((segment.trimEnd - segment.trimStart) / dt) + 1;
    const path: { time: number; x: number; y: number; zoom: number }[] = new Array(totalFrames);

    let sx = videoWidth / 2, sy = videoHeight / 2, sz = 1.0;
    let svx = 0, svy = 0, svz = 0;

    let hoverTime = 0;
    let lastPosX = data[0].x, lastPosY = data[0].y;
    let smoothedZoomTarget = P.BASE_ZOOM;
    const zoomLerp = 0.06 + 0.14 * (config?.followTightness ?? 0.5);
    const hasKeyframes = segment.zoomKeyframes && segment.zoomKeyframes.length > 0;

    // Run Simulation
    let frameIdx = 0;
    for (let t = segment.trimStart; t <= segment.trimEnd; t += dt) {

      // A. Sample current + velocity via binary search (O(log n) each)
      const cur = sampleAt(data, t);
      const vel0 = sampleAt(data, t - 0.1);
      const vel1 = sampleAt(data, t + 0.1);
      const velocity = Math.sqrt((vel1.x - vel0.x) ** 2 + (vel1.y - vel0.y) ** 2) / 0.2;

      // Dynamic look-ahead + future position
      const lookAhead = P.LOOK_AHEAD_MAX * (1 - Math.exp(-velocity / P.LOOK_AHEAD_SCALE));
      const future = sampleAt(data, t + lookAhead);

      // Click check via sorted clickTimes (O(log n) + small scan)
      let isClicked = false;
      if (clickTimes.length > 0) {
        const cStart = t - 0.25;
        const cEnd = t + 0.25;
        let ci = lowerBoundNum(clickTimes, cStart);
        if (ci < clickTimes.length && clickTimes[ci] <= cEnd) isClicked = true;
      }

      // Update hover state
      const dx = cur.x - lastPosX, dy = cur.y - lastPosY;
      if (dx * dx + dy * dy < 4.0) {
        hoverTime += dt;
      } else {
        hoverTime = Math.max(0, hoverTime - dt * 2);
      }
      lastPosX = cur.x;
      lastPosY = cur.y;

      // B. Target Zoom
      let rawTargetZoom = P.BASE_ZOOM;
      const speedFactor = Math.min(1.0, velocity / P.MAX_VELOCITY_ZOOM_PENALTY);
      rawTargetZoom = rawTargetZoom * (1 - speedFactor) + P.MIN_ZOOM * speedFactor;
      if (isClicked) rawTargetZoom = Math.max(rawTargetZoom, Math.min(1.7, P.BASE_ZOOM));
      if (hoverTime > 2.0) rawTargetZoom = P.MAX_ZOOM;
      smoothedZoomTarget += (rawTargetZoom - smoothedZoomTarget) * zoomLerp;

      // C. Target Position
      let targetX = future.x;
      let targetY = future.y;

      if (hasKeyframes) {
        const kf = nearestKeyframe(segment.zoomKeyframes!, t);
        if (kf.weight > 0) {
          targetX = targetX * (1 - kf.weight) + kf.x * videoWidth * kf.weight;
          targetY = targetY * (1 - kf.weight) + kf.y * videoHeight * kf.weight;
          smoothedZoomTarget = smoothedZoomTarget * (1 - kf.weight) + kf.zoom * kf.weight;
        }
      }

      // D. Physics
      const ax = (-P.TENSION * (sx - targetX) - P.FRICTION * svx) / P.MASS;
      const ay = (-P.TENSION * (sy - targetY) - P.FRICTION * svy) / P.MASS;
      const az = (-P.TENSION * (sz - smoothedZoomTarget) - P.FRICTION * svz) / (P.MASS * 1.2);
      svx += ax * dt; svy += ay * dt; svz += az * dt;
      sx += svx * dt; sy += svy * dt; sz += svz * dt;
      if (sz < 1.0) sz = 1.0; else if (sz > 5.0) sz = 5.0;

      // Record
      path[frameIdx++] = {
        time: Math.round(t * 1000) / 1000,
        x: Math.round(sx * 10) / 10,
        y: Math.round(sy * 10) / 10,
        zoom: Math.round(sz * 1000) / 1000,
      };
    }

    path.length = frameIdx;
    const elapsed = performance.now() - t0;
    const dur = segment.trimEnd - segment.trimStart;
    console.log(`[AutoZoom] generateMotionPath: ${elapsed.toFixed(1)}ms for ${dur.toFixed(1)}s clip (${data.length} samples, ${frameIdx} frames)`);
    return path;
  }
}

// Binary search on plain number array
function lowerBoundNum(arr: number[], val: number): number {
  let lo = 0, hi = arr.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (arr[mid] < val) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

// Find nearest keyframe within 1.5s (keyframes are few, simple scan is fine)
function nearestKeyframe(
  keyframes: ZoomKeyframe[],
  t: number
): { x: number; y: number; zoom: number; weight: number } {
  const WINDOW = 1.5;
  let bestDist = WINDOW;
  let bestKf: ZoomKeyframe | null = null;
  for (const kf of keyframes) {
    const d = Math.abs(kf.time - t);
    if (d < bestDist) { bestDist = d; bestKf = kf; }
  }
  if (!bestKf) return { x: 0.5, y: 0.5, zoom: 1, weight: 0 };
  const ratio = bestDist / WINDOW;
  return {
    x: bestKf.positionX,
    y: bestKf.positionY,
    zoom: bestKf.zoomFactor,
    weight: (1 + Math.cos(ratio * Math.PI)) / 2,
  };
}

export const autoZoomGenerator = new AutoZoomGenerator();