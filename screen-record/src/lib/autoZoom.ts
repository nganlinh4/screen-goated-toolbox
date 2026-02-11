import { VideoSegment, MousePosition, ZoomKeyframe } from '@/types/video';

// Physics Configuration
const PHYSICS = {
  // Mass-Spring-Damper Constants
  // Lower tension = lazier spring (floaty)
  TENSION: 20.0,
  // Critically damped for T=20, M=5 => 2 * sqrt(20 * 5) = 20
  FRICTION: 20.0,
  MASS: 5.0,      // Heavy camera for inertia

  // Behaviour — dynamic look-ahead
  // Camera "leads" the cursor: minimal anticipation when still, more when fast.
  // Exponential saturation: lookAhead = MAX * (1 - e^(-speed / SCALE))
  LOOK_AHEAD_MAX: 0.45,   // seconds — ceiling even at extreme speeds
  LOOK_AHEAD_SCALE: 700,  // px/s at which we reach ~63% of max

  // Velocity-dependent zoom-out
  MAX_VELOCITY_ZOOM_PENALTY: 1500, // px/s at which zoom fully drops to MIN

  // Limits
  BASE_ZOOM: 2.0,
  MIN_ZOOM: 1.0,
  MAX_ZOOM: 2.0
};

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
    videoHeight: number
  ): { time: number; x: number; y: number; zoom: number }[] {

    const path: { time: number; x: number; y: number; zoom: number }[] = [];

    // 0. Filter and Sort Data
    const data = mousePositions
      .filter(p => p.timestamp >= segment.trimStart - 1.0 && p.timestamp <= segment.trimEnd + 1.0)
      .sort((a, b) => a.timestamp - b.timestamp);

    if (data.length < 2) return [];

    // 1. Initialize Simulation
    const dt = 1 / 60; // 60hz Physics Simulation

    let state: PhysicsState = {
      x: videoWidth / 2, // Start centered based on actual video width
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

    let smoothedZoomTarget = PHYSICS.BASE_ZOOM;

    // Run Simulation
    for (let t = segment.trimStart; t <= segment.trimEnd; t += dt) {

      // A. Identify Target (Where SHOULD the camera be?)
      const currentMouse = this.sample(data, t);

      // Calculate Mouse Characteristics
      const velocity = this.getVelocity(data, t); // pixels per sec

      // Dynamic look-ahead: still cursor → ~0s, fast cursor → up to MAX.
      // Exponential saturation prevents over-prediction at extreme speeds.
      const lookAhead = PHYSICS.LOOK_AHEAD_MAX * (1 - Math.exp(-velocity / PHYSICS.LOOK_AHEAD_SCALE));
      const futureMouse = this.sample(data, t + lookAhead);
      const isClicked = this.checkClick(data, t, 0.5); // Check if click happens within 0.5s window

      // Update Interaction State
      const moveDist = Math.sqrt(Math.pow(currentMouse.x - interaction.lastPos.x, 2) + Math.pow(currentMouse.y - interaction.lastPos.y, 2));
      if (moveDist < 2.0) { // Mouse is still (< 2px movement in this step)
        interaction.hoverTime += dt;
      } else {
        interaction.hoverTime = Math.max(0, interaction.hoverTime - dt * 2); // Decay hover status
      }
      interaction.lastPos = { x: currentMouse.x, y: currentMouse.y };

      // B. Determine Target Zoom
      let rawTargetZoom = PHYSICS.BASE_ZOOM;

      // Rule 1: Velocity Penalty — zoom out when cursor moves fast
      const speedFactor = Math.min(1.0, velocity / PHYSICS.MAX_VELOCITY_ZOOM_PENALTY);
      rawTargetZoom = rawTargetZoom * (1 - speedFactor) + PHYSICS.MIN_ZOOM * speedFactor;

      // Click Focus (Clicking -> Zoom In)
      if (isClicked) {
        rawTargetZoom = Math.max(rawTargetZoom, 1.7);
      }

      // Deep Read (Long Hover -> Zoom In Deep)
      if (interaction.hoverTime > 2.0) {
        rawTargetZoom = PHYSICS.MAX_ZOOM;
      }

      // Smooth the zoom target to prevent jitter
      // Simple LERP filter
      smoothedZoomTarget = smoothedZoomTarget + (rawTargetZoom - smoothedZoomTarget) * 0.05;

      // C. Determine Target Position
      // We start with the Future Mouse Position (Anticipation)
      let targetX = futureMouse.x;
      let targetY = futureMouse.y;

      // Override: Manual Keyframes
      // If user sets a manual keyframe, it acts as a magnet
      if (segment.zoomKeyframes && segment.zoomKeyframes.length > 0) {
        const kfInfluence = this.getKeyframeInfluence(segment.zoomKeyframes, t, videoWidth, videoHeight);
        if (kfInfluence.weight > 0) {
          // targetX/Y are pixels, kf is normalized 0-1
          const kfX = kfInfluence.x * videoWidth;
          const kfY = kfInfluence.y * videoHeight;
          const kfZ = kfInfluence.zoom;

          // Blend Target
          // If weight is 1.0, we strictly follow keyframe
          targetX = targetX * (1 - kfInfluence.weight) + kfX * kfInfluence.weight;
          targetY = targetY * (1 - kfInfluence.weight) + kfY * kfInfluence.weight;
          smoothedZoomTarget = smoothedZoomTarget * (1 - kfInfluence.weight) + kfZ * kfInfluence.weight;
        }
      }

      // D. Apply Physics (Spring/Damper)
      // Force = -k*(x - target) - d*v
      const ax = (-PHYSICS.TENSION * (state.x - targetX) - PHYSICS.FRICTION * state.vx) / PHYSICS.MASS;
      const ay = (-PHYSICS.TENSION * (state.y - targetY) - PHYSICS.FRICTION * state.vy) / PHYSICS.MASS;
      // Use higher mass for zoom for smoother feel
      const az = (-PHYSICS.TENSION * (state.zoom - smoothedZoomTarget) - PHYSICS.FRICTION * state.vz) / (PHYSICS.MASS * 2.0);

      state.vx += ax * dt;
      state.vy += ay * dt;
      state.vz += az * dt;

      state.x += state.vx * dt;
      state.y += state.vy * dt;
      state.zoom += state.vz * dt;

      // No hard position clamp — the camera freely tracks the cursor.
      // With custom canvas padding, the contain-fit mapping and the renderer's
      // [0,1] posX/posY clamp provide canvas-level safety.  The critically-damped
      // spring won't overshoot, and cursor data is always within video bounds.

      // Clamp Zoom safety
      state.zoom = Math.max(1.0, Math.min(5.0, state.zoom)); // Absolute safety limits

      // F. Record Frame
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