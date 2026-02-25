import { VideoSegment, ZoomKeyframe } from '@/types/video';

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

// Blend two zoom states with log-space zoom + viewport-center-space position
export function blendZoomStates(
  stateA: ZoomKeyframe,
  stateB: ZoomKeyframe,
  t: number // 0 = stateA, 1 = stateB
): { zoom: number; posX: number; posY: number } {
  const zA = Math.max(0.1, stateA.zoomFactor);
  const zB = Math.max(0.1, stateB.zoomFactor);
  // Log-space zoom for perceptually uniform scaling
  const zoom = zA * Math.pow(zB / zA, t);
  // Viewport-center-space position for drift-free motion
  const cA = toViewportCenter(zA, stateA.positionX, stateA.positionY);
  const cB = toViewportCenter(zB, stateB.positionX, stateB.positionY);
  const cx = cA.cx + (cB.cx - cA.cx) * t;
  const cy = cA.cy + (cB.cy - cA.cy) * t;
  const { posX, posY } = fromViewportCenter(zoom, cx, cy);
  return { zoom, posX, posY };
}

export function calculateCurrentZoomStateInternal(
  currentTime: number,
  segment: VideoSegment,
  viewW: number,
  viewH: number,
  srcCropW?: number,  // actual cropped video source width (for auto-path coord transform)
  srcCropH?: number   // actual cropped video source height
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
    const idx = path.findIndex((p: any) => p.time >= currentTime);
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
      const iIdx = points.findIndex((p: { time: number }) => p.time >= currentTime);
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

    // Contain-fit of cropped source into canvas
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
    const fitX = (viewW - fitW) / 2;
    const fitY = (viewH - fitH) / 2;

    autoState = {
      time: currentTime,
      duration: 0,
      zoomFactor: cam.zoom,
      positionX: (fitX + relX * fitW) / viewW,
      positionY: (fitY + relY * fitH) / viewH,
      easingType: 'linear'
    };
  }

  // --- 2. CALCULATE MANUAL KEYFRAME STATE (Foreground Track) ---
  // Improved logic to blend seamlessly with Auto-Zoom

  let manualState: ZoomKeyframe | null = null;
  let manualInfluence = 0.0;

  const sortedKeyframes = [...segment.zoomKeyframes].sort((a: ZoomKeyframe, b: ZoomKeyframe) => a.time - b.time);

  if (sortedKeyframes.length > 0) {
    // Dynamic blending window size based on movement
    const calculateDynamicWindow = (kf1: ZoomKeyframe, kf2?: ZoomKeyframe) => {
      if (!kf2) return 3.0; // Default tail if single keyframe
      const dx = Math.abs(kf1.positionX - kf2.positionX);
      const dy = Math.abs(kf1.positionY - kf2.positionY);
      const dz = Math.abs(kf1.zoomFactor - kf2.zoomFactor);
      const distanceScore = Math.sqrt(dx * dx + dy * dy) + (dz * 0.5);
      return Math.max(1.5, Math.min(4.0, distanceScore * 3.0)); // Adaptive 1.5s to 4s
    };

    const nextKfIdx = sortedKeyframes.findIndex(k => k.time > currentTime);
    const prevKf = nextKfIdx > 0 ? sortedKeyframes[nextKfIdx - 1] : (nextKfIdx === -1 ? sortedKeyframes[sortedKeyframes.length - 1] : null);
    const nextKf = nextKfIdx !== -1 ? sortedKeyframes[nextKfIdx] : null;

    if (prevKf && nextKf) {
      // BETWEEN TWO KEYFRAMES — always smoothly interpolate between adjacent keyframes.
      // Manual keyframes form a continuous connected curve regardless of auto-path.
      // No decay to default between keyframes — no independent humps.
      manualInfluence = 1.0;
      const timeDiff = nextKf.time - prevKf.time;
      const rawT = (currentTime - prevKf.time) / timeDiff;
      const t = Math.max(0, Math.min(1, rawT));
      const easedT = easeCameraMove(t);

      const { zoom: currentZoom, posX, posY } = blendZoomStates(prevKf, nextKf, easedT);

      manualState = {
        time: currentTime, duration: 0, zoomFactor: currentZoom, positionX: posX, positionY: posY, easingType: 'easeOut'
      };
    } else if (prevKf) {
      // AFTER LAST KEYFRAME
      if (hasAutoPath) {
        const currentTarget = autoState || DEFAULT_ZOOM_STATE;
        const decayWindow = calculateDynamicWindow(prevKf, currentTarget);

        const timeFromPrev = currentTime - prevKf.time;
        if (timeFromPrev < decayWindow) {
          const progress = timeFromPrev / decayWindow; // 0 at keyframe → 1 at end of decay
          manualInfluence = 1 - easeCameraMove(progress);
        }
      } else {
        // Hold last keyframe forever if no auto path
        manualInfluence = 1.0;
      }
      manualState = prevKf;
    } else if (nextKf) {
      // BEFORE FIRST KEYFRAME — cosine ease from default to keyframe
      const currentTarget = autoState || DEFAULT_ZOOM_STATE;
      const hasCustomDuration = nextKf.duration > 0;
      const rampWindow = hasCustomDuration ? nextKf.duration : calculateDynamicWindow(nextKf, currentTarget);

      const timeToNext = nextKf.time - currentTime;
      if (timeToNext <= rampWindow) {
        const progress = 1 - timeToNext / rampWindow; // 0 at ramp start → 1 at keyframe
        manualInfluence = easeCameraMove(progress);
      }
      manualState = nextKf;
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
