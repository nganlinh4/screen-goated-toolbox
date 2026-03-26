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

interface InteractionState {
  isClicking: boolean;
  clickTime: number;
  hoverTime: number;
  lastPos: { x: number, y: number };
}

interface PhysicsState {
  x: number;
  y: number;
  zoom: number;
  vx: number;
  vy: number;
  vz: number;
}

export class AutoZoomGenerator {
  // Hardcoded dimensions removed.
  // They are now passed dynamically in generateMotionPath.

  generateMotionPath(
    segment: VideoSegment,
    mousePositions: MousePosition[],
    videoWidth: number,
    videoHeight: number,
    config?: AutoZoomConfig
  ): { time: number; x: number; y: number; zoom: number }[] {

    const P = resolvePhysics(config ?? DEFAULT_AUTO_ZOOM_CONFIG);
    const path: { time: number; x: number; y: number; zoom: number }[] = [];

    // 0. Filter and Sort Data
    const data = mousePositions
      .filter(p => p.timestamp >= segment.trimStart - 1.0 && p.timestamp <= segment.trimEnd + 1.0)
      .sort((a, b) => a.timestamp - b.timestamp);

    if (data.length < 2) return [];

    // 1. Initialize Simulation
    const dt = 1 / 60; // 60hz Physics Simulation

    let state: PhysicsState = {
      x: videoWidth / 2,
      y: videoHeight / 2,
      zoom: 1.0,
      vx: 0,
      vy: 0,
      vz: 0
    };

    let interaction: InteractionState = {
      isClicking: false,
      clickTime: -100,
      hoverTime: 0,
      lastPos: { x: data[0].x, y: data[0].y }
    };

    let smoothedZoomTarget = P.BASE_ZOOM;

    // Zoom smoothing lerp — tighter follow = faster zoom response
    const zoomLerp = 0.06 + 0.14 * (config?.followTightness ?? 0.5); // 0.06–0.20

    // Run Simulation
    for (let t = segment.trimStart; t <= segment.trimEnd; t += dt) {

      // A. Identify Target (Where SHOULD the camera be?)
      const currentMouse = this.sample(data, t);
      const velocity = this.getVelocity(data, t);

      // Dynamic look-ahead
      const lookAhead = P.LOOK_AHEAD_MAX * (1 - Math.exp(-velocity / P.LOOK_AHEAD_SCALE));
      const futureMouse = this.sample(data, t + lookAhead);
      const isClicked = this.checkClick(data, t, 0.5);

      // Update Interaction State
      const moveDist = Math.sqrt(Math.pow(currentMouse.x - interaction.lastPos.x, 2) + Math.pow(currentMouse.y - interaction.lastPos.y, 2));
      if (moveDist < 2.0) {
        interaction.hoverTime += dt;
      } else {
        interaction.hoverTime = Math.max(0, interaction.hoverTime - dt * 2);
      }
      interaction.lastPos = { x: currentMouse.x, y: currentMouse.y };

      // B. Determine Target Zoom
      let rawTargetZoom = P.BASE_ZOOM;

      // Velocity Penalty — zoom out when cursor moves fast
      const speedFactor = Math.min(1.0, velocity / P.MAX_VELOCITY_ZOOM_PENALTY);
      rawTargetZoom = rawTargetZoom * (1 - speedFactor) + P.MIN_ZOOM * speedFactor;

      // Click Focus
      if (isClicked) {
        rawTargetZoom = Math.max(rawTargetZoom, Math.min(1.7, P.BASE_ZOOM));
      }

      // Deep Read (Long Hover)
      if (interaction.hoverTime > 2.0) {
        rawTargetZoom = P.MAX_ZOOM;
      }

      // Smooth the zoom target
      smoothedZoomTarget = smoothedZoomTarget + (rawTargetZoom - smoothedZoomTarget) * zoomLerp;

      // C. Determine Target Position
      let targetX = futureMouse.x;
      let targetY = futureMouse.y;

      // Override: Manual Keyframes
      if (segment.zoomKeyframes && segment.zoomKeyframes.length > 0) {
        const kfInfluence = this.getKeyframeInfluence(segment.zoomKeyframes, t, videoWidth, videoHeight);
        if (kfInfluence.weight > 0) {
          const kfX = kfInfluence.x * videoWidth;
          const kfY = kfInfluence.y * videoHeight;
          const kfZ = kfInfluence.zoom;
          targetX = targetX * (1 - kfInfluence.weight) + kfX * kfInfluence.weight;
          targetY = targetY * (1 - kfInfluence.weight) + kfY * kfInfluence.weight;
          smoothedZoomTarget = smoothedZoomTarget * (1 - kfInfluence.weight) + kfZ * kfInfluence.weight;
        }
      }

      // D. Apply Physics (Spring/Damper)
      const ax = (-P.TENSION * (state.x - targetX) - P.FRICTION * state.vx) / P.MASS;
      const ay = (-P.TENSION * (state.y - targetY) - P.FRICTION * state.vy) / P.MASS;
      const az = (-P.TENSION * (state.zoom - smoothedZoomTarget) - P.FRICTION * state.vz) / (P.MASS * 1.2);

      state.vx += ax * dt;
      state.vy += ay * dt;
      state.vz += az * dt;

      state.x += state.vx * dt;
      state.y += state.vy * dt;
      state.zoom += state.vz * dt;

      // Clamp Zoom safety
      state.zoom = Math.max(1.0, Math.min(5.0, state.zoom));

      // Record Frame
      path.push({
        time: Number(t.toFixed(3)),
        x: Number(state.x.toFixed(1)),
        y: Number(state.y.toFixed(1)),
        zoom: Number(state.zoom.toFixed(3))
      });
    }

    return path;
  }

  // --- Helpers ---

  private sample(data: MousePosition[], t: number): { x: number, y: number } {
    if (t <= data[0].timestamp) return { x: data[0].x, y: data[0].y };
    if (t >= data[data.length - 1].timestamp) return { x: data[data.length - 1].x, y: data[data.length - 1].y };

    // Find index
    const idx = data.findIndex(p => p.timestamp >= t);
    if (idx === -1) return { x: data[data.length - 1].x, y: data[data.length - 1].y };

    // Lerp
    const p1 = data[idx - 1];
    const p2 = data[idx];
    const ratio = (t - p1.timestamp) / (p2.timestamp - p1.timestamp);

    return {
      x: p1.x + (p2.x - p1.x) * ratio,
      y: p1.y + (p2.y - p1.y) * ratio
    };
  }

  private getVelocity(data: MousePosition[], t: number): number {
    const window = 0.1;
    const p1 = this.sample(data, t - window);
    const p2 = this.sample(data, t + window);
    const dist = Math.sqrt(Math.pow(p2.x - p1.x, 2) + Math.pow(p2.y - p1.y, 2));
    return dist / (window * 2);
  }

  private checkClick(data: MousePosition[], t: number, window: number): boolean {
    const start = t - window / 2;
    const end = t + window / 2;
    return data.some(p => p.timestamp >= start && p.timestamp <= end && p.isClicked);
  }

  private getKeyframeInfluence(keyframes: ZoomKeyframe[], t: number, _videoWidth: number, _videoHeight: number): { x: number, y: number, zoom: number, weight: number } {
    const WINDOW = 1.5;

    const nearby = keyframes
      .map(kf => ({ kf, dist: Math.abs(kf.time - t) }))
      .filter(item => item.dist < WINDOW)
      .sort((a, b) => a.dist - b.dist);

    if (nearby.length === 0) return { x: 0.5, y: 0.5, zoom: 1, weight: 0 };

    const best = nearby[0];
    const ratio = best.dist / WINDOW;
    const weight = (1 + Math.cos(ratio * Math.PI)) / 2;

    return {
      x: best.kf.positionX,
      y: best.kf.positionY,
      zoom: best.kf.zoomFactor,
      weight: weight
    };
  }
}

export const autoZoomGenerator = new AutoZoomGenerator();