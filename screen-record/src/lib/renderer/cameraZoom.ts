import { VideoSegment, ZoomKeyframe, ZoomBlock } from '@/types/video';

export const DEFAULT_ZOOM_STATE: ZoomKeyframe = {
  time: 0,
  duration: 0,
  zoomFactor: 1,
  positionX: 0.5,
  positionY: 0.5,
  easingType: 'linear' as const
};

// Perlin's smootherStep: zero velocity AND zero acceleration at both endpoints.
// The speed curve (derivative) is 30t²(1-t)² — touches zero as a smooth parabola,
// not a sharp V. This eliminates the visible "corner" at keyframe boundaries.
export function easeCameraMove(t: number): number {
  if (t <= 0) return 0;
  if (t >= 1) return 1;
  return t * t * t * (t * (t * 6 - 15) + 10);
}

// --- Viewport-center-space blending for drift-free camera motion ---
// posX/Y are zoom anchor params whose visual effect depends on zoom level.
// Blending them directly causes sliding. Instead, blend the actual visible
// center on screen, then convert back to anchor params.

export function toViewportCenter(zoom: number, posX: number, posY: number) {
  if (zoom <= 1.0) return { cx: 0.5, cy: 0.5 };
  return {
    cx: posX + (0.5 - posX) / zoom,
    cy: posY + (0.5 - posY) / zoom
  };
}

export function fromViewportCenter(zoom: number, cx: number, cy: number) {
  if (zoom <= 1.001) return { posX: cx, posY: cy };
  const s = 1 - 1 / zoom;
  return {
    posX: (cx - 0.5 / zoom) / s,
    posY: (cy - 0.5 / zoom) / s
  };
}

type HyperboloidPoint = [number, number, number, number];

function cameraToHyperboloid(cx: number, cy: number, altitude: number): HyperboloidPoint {
  const v = Math.max(altitude, 1e-6);
  const radiusSq = cx * cx + cy * cy;
  return [
    (radiusSq + v * v + 1) / (2 * v),
    cx / v,
    cy / v,
    (radiusSq + v * v - 1) / (2 * v),
  ];
}

function hyperboloidToCamera(point: HyperboloidPoint) {
  const altitude = 1 / Math.max(point[0] - point[3], 1e-9);
  return {
    cx: point[1] * altitude,
    cy: point[2] * altitude,
    altitude,
  };
}

/**
 * Perceptually efficient camera travel on the Poincare upper-half-space model.
 * The camera center is the horizontal footprint and 1/zoom is its altitude.
 * A hyperbolic geodesic naturally introduces a small zoom-out arc for long
 * pans, reducing screen-space optical flow without special-case heuristics.
 */
function interpolateCameraGeodesic(
  from: { cx: number; cy: number; altitude: number },
  to: { cx: number; cy: number; altitude: number },
  progress: number,
) {
  const t = Math.max(0, Math.min(1, progress));
  if (t === 0) return from;
  if (t === 1) return to;
  const a = cameraToHyperboloid(from.cx, from.cy, from.altitude);
  const b = cameraToHyperboloid(to.cx, to.cy, to.altitude);
  const lorentzDot = -a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
  const distance = Math.acosh(Math.max(1, -lorentzDot));
  if (distance < 1e-7) {
    return {
      cx: from.cx + (to.cx - from.cx) * t,
      cy: from.cy + (to.cy - from.cy) * t,
      altitude: from.altitude * Math.pow(to.altitude / from.altitude, t),
    };
  }
  const denominator = Math.sinh(distance);
  const weightA = Math.sinh((1 - t) * distance) / denominator;
  const weightB = Math.sinh(t * distance) / denominator;
  return hyperboloidToCamera([
    weightA * a[0] + weightB * b[0],
    weightA * a[1] + weightB * b[1],
    weightA * a[2] + weightB * b[2],
    weightA * a[3] + weightB * b[3],
  ]);
}

// Blend two views along the perceptually shortest coupled pan/zoom path.
export function blendZoomStates(
  stateA: ZoomKeyframe,
  stateB: ZoomKeyframe,
  t: number // 0 = stateA, 1 = stateB
): { zoom: number; posX: number; posY: number } {
  const zA = Math.max(0.1, stateA.zoomFactor);
  const zB = Math.max(0.1, stateB.zoomFactor);
  const cA = toViewportCenter(zA, stateA.positionX, stateA.positionY);
  const cB = toViewportCenter(zB, stateB.positionX, stateB.positionY);
  const camera = interpolateCameraGeodesic(
    { ...cA, altitude: 1 / zA },
    { ...cB, altitude: 1 / zB },
    t,
  );
  const zoom = 1 / Math.max(camera.altitude, 1e-6);
  const { posX, posY } = fromViewportCenter(zoom, camera.cx, camera.cy);
  return { zoom, posX, posY };
}

export function calculateCurrentZoomStateInternal(
  currentTime: number,
  segment: VideoSegment,
  viewW: number,
  viewH: number,
  srcCropW?: number,  // actual cropped video source width (for auto-path coord transform)
  srcCropH?: number,  // actual cropped video source height
  videoScale?: number  // backgroundConfig.scale / 100 — adjusts auto-zoom contain-fit for preview
): ZoomKeyframe {

  // Source crop dimensions — when provided, auto-path video-pixel coords are
  // transformed through contain-fit into canvas-anchor space.  When not provided
  // (backwards compat), viewW/viewH are assumed to match the source crop dims.
  const sCropW = srcCropW ?? viewW;
  const sCropH = srcCropH ?? viewH;

  // --- 1. CALCULATE AUTO-SMART ZOOM STATE (Background Track) ---
  const hasAutoPath = segment.smoothMotionPath && segment.smoothMotionPath.length > 0;
  let autoState: ZoomKeyframe | null = null;

  if (hasAutoPath) {
    const path = segment.smoothMotionPath!;
    // Binary search: first index where path[i].time >= currentTime (O(log n))
    let lo = 0, hi = path.length;
    while (lo < hi) { const mid = (lo + hi) >> 1; if ((path[mid] as any).time < currentTime) lo = mid + 1; else hi = mid; }
    const idx = lo < path.length ? lo : -1;
    // Default in video-pixel space (center of cropped source)
    const crop0 = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
    const vidFullW = sCropW / crop0.width;
    const vidFullH = sCropH / crop0.height;
    let cam = { x: vidFullW * crop0.x + sCropW / 2, y: vidFullH * crop0.y + sCropH / 2, zoom: 1.0 };

    if (idx === -1) {
      const last = path[path.length - 1];
      cam = { x: last.x, y: last.y, zoom: last.zoom };
    } else if (idx === 0) {
      const first = path[0];
      cam = { x: first.x, y: first.y, zoom: first.zoom };
    } else {
      const p1 = path[idx - 1];
      const p2 = path[idx];
      const t = (currentTime - p1.time) / (p2.time - p1.time);
      cam = {
        x: p1.x + (p2.x - p1.x) * t,
        y: p1.y + (p2.y - p1.y) * t,
        zoom: p1.zoom + (p2.zoom - p1.zoom) * t
      };
    }

    // Apply Influence
    if (segment.zoomInfluencePoints && segment.zoomInfluencePoints.length > 0) {
      const points = segment.zoomInfluencePoints;
      let influence = 1.0;
      // Binary search for influence points (O(log n))
      let ilo = 0, ihi = points.length;
      while (ilo < ihi) { const mid = (ilo + ihi) >> 1; if (points[mid].time < currentTime) ilo = mid + 1; else ihi = mid; }
      const iIdx = ilo < points.length ? ilo : -1;
      if (iIdx === -1) {
        influence = points[points.length - 1].value;
      } else if (iIdx === 0) {
        influence = points[0].value;
      } else {
        const ip1 = points[iIdx - 1];
        const ip2 = points[iIdx];
        const it = (currentTime - ip1.time) / (ip2.time - ip1.time);
        const cosT = (1 - Math.cos(it * Math.PI)) / 2;
        influence = ip1.value * (1 - cosT) + ip2.value * cosT;
      }
      cam.zoom = 1.0 + (cam.zoom - 1.0) * influence;
      // Use crop center in video-pixel coords so influence=0 returns to crop center
      const cropInf = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
      const fullWInf = sCropW / cropInf.width;
      const fullHInf = sCropH / cropInf.height;
      const centerX = fullWInf * cropInf.x + sCropW / 2;
      const centerY = fullHInf * cropInf.y + sCropH / 2;
      cam.x = centerX + (cam.x - centerX) * influence;
      cam.y = centerY + (cam.y - centerY) * influence;
    }

    // Convert auto-path coords (video pixel space) → canvas-anchor posX/posY
    // via contain-fit when canvas dims differ from source dims
    const crop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
    const fullW = sCropW / crop.width;
    const fullH = sCropH / crop.height;
    const cropOffsetX = fullW * crop.x;
    const cropOffsetY = fullH * crop.y;

    // Relative position within crop (0-1)
    const relX = (cam.x - cropOffsetX) / sCropW;
    const relY = (cam.y - cropOffsetY) / sCropH;

    // Contain-fit of cropped source into canvas, with optional scale adjustment.
    // When videoScale < 1 (e.g. 90%), the video is smaller and centered — the
    // auto-zoom anchor must match the actual scaled placement so the camera
    // centers correctly on the cursor.  Manual keyframes are NOT affected.
    const srcAspect = sCropW / sCropH;
    const canvasAspect = viewW / viewH;
    let fitW: number, fitH: number;
    if (srcAspect > canvasAspect) {
      fitW = viewW;
      fitH = viewW / srcAspect;
    } else {
      fitH = viewH;
      fitW = viewH * srcAspect;
    }
    const vs = videoScale ?? 1;
    const scaledFitW = fitW * vs;
    const scaledFitH = fitH * vs;
    const fitX = (viewW - scaledFitW) / 2;
    const fitY = (viewH - scaledFitH) / 2;

    autoState = {
      time: currentTime,
      duration: 0,
      zoomFactor: cam.zoom,
      positionX: (fitX + relX * scaledFitW) / viewW,
      positionY: (fitY + relY * scaledFitH) / viewH,
      easingType: 'linear'
    };
  }

  // --- 2. CALCULATE MANUAL ZOOM BLOCK STATE (Foreground Track) ---
  // Each zoom block is a bounded region: ease-in → hold → ease-out.  Outside
  // every block manualInfluence is 0, so the auto path / default shows through —
  // a gap between two blocks naturally reverts to auto-zoom.  Blocks are
  // independent (no spanning spline), which is what lets auto-zoom live between
  // two manual zooms.

  let manualState: ZoomKeyframe | null = null;
  let manualInfluence = 0.0;

  const blocks = segment.zoomBlocks;
  if (blocks && blocks.length > 0) {
    const enabledBlocks = blocks
      .filter((block) => block.enabled !== false)
      .sort((a, b) => a.startTime - b.startTime);
    for (let index = 0; index < enabledBlocks.length - 1; index += 1) {
      const from = enabledBlocks[index];
      const to = enabledBlocks[index + 1];
      if (!from.directTransitionToNext) continue;
      const transitionStart = from.endTime - Math.max(0, from.easeOut);
      const transitionEnd = to.startTime + Math.max(0, to.easeIn);
      if (
        transitionEnd <= transitionStart + 1e-6 ||
        currentTime < transitionStart ||
        currentTime > transitionEnd
      ) continue;
      const progress = easeCameraMove(
        (currentTime - transitionStart) / (transitionEnd - transitionStart),
      );
      const fromState: ZoomKeyframe = {
        time: currentTime, duration: 0, zoomFactor: from.zoomFactor,
        positionX: from.followCursor && autoState ? autoState.positionX : from.positionX,
        positionY: from.followCursor && autoState ? autoState.positionY : from.positionY,
        easingType: 'linear',
      };
      const toState: ZoomKeyframe = {
        time: currentTime, duration: 0, zoomFactor: to.zoomFactor,
        positionX: to.followCursor && autoState ? autoState.positionX : to.positionX,
        positionY: to.followCursor && autoState ? autoState.positionY : to.positionY,
        easingType: 'linear',
      };
      const direct = blendZoomStates(fromState, toState, progress);
      manualState = {
        time: currentTime, duration: 0, zoomFactor: direct.zoom,
        positionX: direct.posX, positionY: direct.posY, easingType: 'linear',
      };
      manualInfluence = 1;
      break;
    }

    // Pick the block with the strongest envelope at currentTime.  This handles
    // gaps (all envelopes 0 → auto) and any accidental overlap deterministically.
    if (!manualState) {
      let bestBlock: ZoomBlock | null = null;
      let bestEnv = 0;
      for (const b of blocks) {
        if (b.enabled === false) continue;
        const env = zoomBlockEnvelope(b, currentTime);
        if (env > bestEnv) { bestEnv = env; bestBlock = b; }
      }
      if (bestBlock && bestEnv > 0) {
        manualInfluence = bestEnv;
        const followsCursor = bestBlock.followCursor === true && autoState !== null;
        manualState = {
          time: currentTime,
          duration: 0,
          zoomFactor: bestBlock.zoomFactor,
          positionX: followsCursor ? autoState!.positionX : bestBlock.positionX,
          positionY: followsCursor ? autoState!.positionY : bestBlock.positionY,
          easingType: 'easeOut',
        };
      }
    }
  }

  // --- 3. FINAL BLENDING ---

  let result: ZoomKeyframe;

  if (autoState) {
    if (manualState && manualInfluence > 0.001) {
      // Blend Auto and Manual in viewport-center space
      const { zoom: finalZoom, posX: finalX, posY: finalY } = blendZoomStates(autoState, manualState, manualInfluence);
      result = { time: currentTime, duration: 0, zoomFactor: finalZoom, positionX: finalX, positionY: finalY, easingType: 'linear' };
    } else {
      // Pure Auto
      result = autoState;
    }
  } else if (manualState && manualInfluence > 0.001) {
    // No Auto path — always blend (no threshold skip that creates zoom jumps)
    const def = DEFAULT_ZOOM_STATE;
    const { zoom: finalZoom, posX: finalX, posY: finalY } = blendZoomStates(def, manualState, manualInfluence);
    result = { time: currentTime, duration: 0, zoomFactor: finalZoom, positionX: finalX, positionY: finalY, easingType: 'linear' };
  } else {
    return DEFAULT_ZOOM_STATE;
  }

  // Clamp position to valid viewport range — prevents off-screen navigation
  // when auto-zoom targets points outside the crop region or blending overshoots
  result.positionX = Math.max(0, Math.min(1, result.positionX));
  result.positionY = Math.max(0, Math.min(1, result.positionY));
  return result;
}

// Manual-zoom block envelope: 0 outside the block, ramps up over easeIn,
// holds at 1 across the body, ramps down over easeOut.  Reuses smootherStep so
// the camera arrives/leaves with zero velocity and acceleration.
export function zoomBlockEnvelope(b: ZoomBlock, t: number): number {
  if (t <= b.startTime || t >= b.endTime) return 0;
  const dur = b.endTime - b.startTime;
  if (dur <= 1e-6) return 0;
  let easeIn = Math.max(0, b.easeIn);
  let easeOut = Math.max(0, b.easeOut);
  // If the ramps would overlap, scale both to fit the block duration.
  if (easeIn + easeOut > dur) {
    const s = dur / (easeIn + easeOut);
    easeIn *= s;
    easeOut *= s;
  }
  const tIn = b.startTime + easeIn;
  const tOut = b.endTime - easeOut;
  if (t < tIn && easeIn > 1e-6) return easeCameraMove((t - b.startTime) / easeIn);
  if (t > tOut && easeOut > 1e-6) return easeCameraMove((b.endTime - t) / easeOut);
  return 1.0;
}

// --- Legacy migration: point keyframes → bounded zoom blocks ---
// Each keyframe becomes one block whose body brackets the keyframe time, with
// short eased ramps. Bounds reuse the old half-way-to-neighbour range math so
// migrated projects keep zooms at the same spots; gaps between non-adjacent
// keyframes now correctly revert to auto-zoom.
const MIGRATION_RAMP_SEC = 0.6;

export function zoomKeyframesToBlocks(
  keyframes: ZoomKeyframe[],
  totalDuration: number,
): ZoomBlock[] {
  if (!keyframes || keyframes.length === 0) return [];
  const sorted = [...keyframes].sort((a, b) => a.time - b.time);
  return sorted.map((kf, i) => {
    const prev = i > 0 ? sorted[i - 1] : null;
    const next = i < sorted.length - 1 ? sorted[i + 1] : null;

    let startTime: number;
    if (kf.duration > 0) {
      startTime = Math.max(prev ? prev.time : 0, kf.time - kf.duration);
    } else {
      startTime = prev ? prev.time + (kf.time - prev.time) * 0.5 : Math.max(0, kf.time - 2.0);
    }
    const endTime = next
      ? kf.time + (next.time - kf.time) * 0.5
      : Math.min(totalDuration || kf.time + 2.0, kf.time + 2.0);

    const easeIn = Math.max(0, Math.min(MIGRATION_RAMP_SEC, kf.time - startTime));
    const easeOut = Math.max(0, Math.min(MIGRATION_RAMP_SEC, endTime - kf.time));

    return {
      id: `zb-${i}-${Math.round(kf.time * 1000)}`,
      startTime,
      endTime,
      easeIn,
      easeOut,
      zoomFactor: kf.zoomFactor,
      positionX: kf.positionX,
      positionY: kf.positionY,
      followCursor: false,
      enabled: true,
    };
  });
}
