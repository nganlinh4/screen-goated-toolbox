import { BackgroundConfig, CursorVisibilitySegment, MousePosition, VideoSegment } from '@/types/video';
import { logSmartPointerGeneration } from '@/lib/cursorDebug';
import { processCursorPositions } from '@/lib/renderer/cursorDynamics';

// --- Configuration ---
const IDLE_DURATION_THRESHOLD = 0.45; // seconds of low velocity to trigger idle
const CENTER_LOCK_DURATION = 0.9;     // seconds to confirm sustained center-lock gameplay
const CENTER_LOCK_BOX_RATIO = 0.04;   // fraction of the smaller source dimension used as half-size
const CENTER_LOCK_BOX_MIN_HALF = 24;  // px — keep the center-lock box strict on small captures
const CENTER_LOCK_BOX_MAX_HALF = 60;  // px — avoid center-lock swallowing normal movement
const VELOCITY_WINDOW = 0.1;          // seconds — sliding window for velocity calc
const ACTIVE_NET_DISTANCE_MIN = 1.0;  // px — min first→last displacement to count as active (lowered: any movement)
const ACTIVE_PATH_DISTANCE_MIN = 1.5; // px — min path distance to count as active (lowered: any movement)
const CLICK_ACTIVE_DURATION = 0.35;   // seconds — keep cursor active briefly after a click event
const MARGIN_BEFORE = 0.04;           // seconds — tiny preroll so fades mostly overlap real movement
const MARGIN_AFTER = 0.08;            // seconds — tiny tail so fades can complete naturally
const MIN_GAP_TO_MERGE = 0.18;        // seconds — merge visible segments only when pauses are tiny

// --- Animation ---
const FADE_IN_DURATION = 0.2;         // seconds — entrance animation
const FADE_OUT_DURATION = 0.25;       // seconds — dismissal animation
const MIN_FULLY_VISIBLE_DURATION = 0.06; // seconds — keep a tiny fully-visible core when possible
const SCALE_HIDDEN = 0.5;             // scale when fully hidden
const SCALE_VISIBLE = 1.0;            // scale when fully visible

// Easing functions
function easeOutCubic(t: number): number {
  return 1 - Math.pow(1 - t, 3);
}

function easeInCubic(t: number): number {
  return t * t * t;
}

function getSegmentFadeDurations(startTime: number, endTime: number): { fadeIn: number; fadeOut: number } {
  const duration = Math.max(0, endTime - startTime);
  const preferredTotal = FADE_IN_DURATION + FADE_OUT_DURATION;
  const maxFadeTotal = Math.max(0, duration - MIN_FULLY_VISIBLE_DURATION);

  if (duration <= 0 || maxFadeTotal <= 0 || preferredTotal <= 0) {
    return { fadeIn: 0, fadeOut: 0 };
  }

  const actualTotal = Math.min(preferredTotal, maxFadeTotal);
  const fadeIn = actualTotal * (FADE_IN_DURATION / preferredTotal);
  const fadeOut = actualTotal - fadeIn;
  return { fadeIn, fadeOut };
}

function withBoundaryMotionSamples(
  rawPositions: MousePosition[],
  motionPositions: MousePosition[],
  timelineStart: number,
  timelineEnd: number
): MousePosition[] {
  if (rawPositions.length === 0) return motionPositions;
  const seeded: MousePosition[] = [];
  const firstRaw = rawPositions[0];
  const lastRaw = rawPositions[rawPositions.length - 1];
  const firstMotion = motionPositions[0] ?? firstRaw;
  const lastMotion = motionPositions[motionPositions.length - 1] ?? lastRaw;

  seeded.push({
    ...firstRaw,
    cursor_rotation: firstMotion.cursor_rotation ?? firstRaw.cursor_rotation,
    timestamp: timelineStart,
  });

  for (const position of motionPositions) {
    if (position.timestamp > timelineStart + 0.0005 && position.timestamp < timelineEnd - 0.0005) {
      seeded.push(position);
    }
  }

  if (timelineEnd - timelineStart > 0.0005) {
    seeded.push({
      ...lastRaw,
      cursor_rotation: lastMotion.cursor_rotation ?? lastRaw.cursor_rotation,
      timestamp: timelineEnd,
    });
  }

  seeded.sort((a, b) => a.timestamp - b.timestamp);

  const deduped: MousePosition[] = [];
  for (const position of seeded) {
    const previous = deduped[deduped.length - 1];
    if (previous && Math.abs(previous.timestamp - position.timestamp) < 0.0005) {
      deduped[deduped.length - 1] = position;
    } else {
      deduped.push(position);
    }
  }
  return deduped;
}

function getFrameDimensions(
  positions: MousePosition[],
  frameWidth?: number,
  frameHeight?: number
): { width: number; height: number } {
  const validFrameWidth = typeof frameWidth === 'number' && Number.isFinite(frameWidth) && frameWidth > 1
    ? frameWidth
    : 0;
  const validFrameHeight = typeof frameHeight === 'number' && Number.isFinite(frameHeight) && frameHeight > 1
    ? frameHeight
    : 0;

  if (validFrameWidth > 0 && validFrameHeight > 0) {
    return { width: validFrameWidth, height: validFrameHeight };
  }

  const captureWidth = positions.find((position) =>
    typeof position.captureWidth === 'number' &&
    Number.isFinite(position.captureWidth) &&
    position.captureWidth > 1
  )?.captureWidth;
  const captureHeight = positions.find((position) =>
    typeof position.captureHeight === 'number' &&
    Number.isFinite(position.captureHeight) &&
    position.captureHeight > 1
  )?.captureHeight;

  if (typeof captureWidth === 'number' && typeof captureHeight === 'number') {
    return { width: captureWidth, height: captureHeight };
  }

  const fallbackWidth = Math.max(1, ...positions.map((position) => Math.abs(position.x)));
  const fallbackHeight = Math.max(1, ...positions.map((position) => Math.abs(position.y)));
  return { width: fallbackWidth, height: fallbackHeight };
}

function getCenterLockHalfSize(frameWidth: number, frameHeight: number): number {
  return Math.max(
    CENTER_LOCK_BOX_MIN_HALF,
    Math.min(
      CENTER_LOCK_BOX_MAX_HALF,
      Math.min(frameWidth, frameHeight) * CENTER_LOCK_BOX_RATIO
    )
  );
}

function isInsideCenterLockBox(
  position: MousePosition,
  frameWidth: number,
  frameHeight: number
): boolean {
  const halfSize = getCenterLockHalfSize(frameWidth, frameHeight);
  const centerX = frameWidth / 2;
  const centerY = frameHeight / 2;
  return (
    Math.abs(position.x - centerX) <= halfSize &&
    Math.abs(position.y - centerY) <= halfSize
  );
}

/**
 * Merge overlapping or touching cursor visibility segments.
 * Segments with startTime <= previous endTime are combined.
 */
export function mergePointerSegments(segments: CursorVisibilitySegment[]): CursorVisibilitySegment[] {
  if (segments.length <= 1) return segments;
  const sorted = [...segments].sort((a, b) => a.startTime - b.startTime);
  const merged: CursorVisibilitySegment[] = [{ ...sorted[0] }];
  for (let i = 1; i < sorted.length; i++) {
    const last = merged[merged.length - 1];
    if (sorted[i].startTime <= last.endTime) {
      last.endTime = Math.max(last.endTime, sorted[i].endTime);
    } else {
      merged.push({ ...sorted[i] });
    }
  }
  return merged;
}

/**
 * Clamp visibility segments to [0, duration], drop invalid ranges, and merge overlaps.
 * Use this for stale project data and post-delay/drag writes.
 */
export function clampVisibilitySegmentsToDuration(
  segments: CursorVisibilitySegment[] | undefined,
  duration: number
): CursorVisibilitySegment[] {
  const safeDuration = Math.max(0, duration);
  if (!segments?.length || safeDuration <= 0) return [];

  const clipped = segments
    .map((segment) => ({
      id: segment.id,
      startTime: Math.max(0, Math.min(safeDuration, segment.startTime)),
      endTime: Math.max(0, Math.min(safeDuration, segment.endTime)),
    }))
    .filter((segment) => segment.endTime - segment.startTime > 0.001);

  return mergePointerSegments(clipped);
}

/**
 * Analyze mouse positions to find idle periods and generate visibility segments.
 * Returns segments where the cursor should be VISIBLE.
 */
export function generateCursorVisibility(
  segment: VideoSegment,
  mousePositions: MousePosition[],
  timelineDuration?: number,
  frameWidth?: number,
  frameHeight?: number,
  backgroundConfig?: BackgroundConfig | null
): CursorVisibilitySegment[] {
  const timelineStart = 0;
  const inferredEnd = Math.max(
    segment.trimEnd || 0,
    ...(mousePositions.length > 0 ? mousePositions.map(p => p.timestamp) : [0])
  );
  const timelineEnd = Math.max(timelineStart, timelineDuration ?? inferredEnd);

  // Filter positions to full timeline range (independent from trim segments)
  const positions = mousePositions.filter(
    p => p.timestamp >= timelineStart && p.timestamp <= timelineEnd
  );
  const processedMotionPositions = processCursorPositions(positions, backgroundConfig).filter(
    p => p.timestamp >= timelineStart && p.timestamp <= timelineEnd
  );
  const motionPositions = withBoundaryMotionSamples(
    positions,
    processedMotionPositions,
    timelineStart,
    timelineEnd
  );

  if (positions.length < 2 || motionPositions.length < 2) {
    // Not enough data — return one segment covering whole timeline (always visible)
    const fallbackSegment = {
      id: crypto.randomUUID(),
      startTime: timelineStart,
      endTime: timelineEnd,
    };
    const { width: sourceWidth, height: sourceHeight } = getFrameDimensions(
      positions,
      frameWidth,
      frameHeight
    );
    logSmartPointerGeneration({
      timelineEnd,
      sampleCount: positions.length,
      motionSampleCount: motionPositions.length,
      sourceWidth,
      sourceHeight,
      centerLockHalfSize: getCenterLockHalfSize(sourceWidth, sourceHeight),
      visibleSegments: [{
        start: Math.round(fallbackSegment.startTime * 1000) / 1000,
        end: Math.round(fallbackSegment.endTime * 1000) / 1000,
      }],
      idleRanges: [],
      transitions: [],
    });
    return [fallbackSegment];
  }

  const mouseEvents = (segment.keystrokeEvents || []).filter(
    e => e.type === 'mousedown' || e.type === 'wheel'
  );

  // Pre-collect interaction timestamps for efficient post-interaction active window lookup.
  const interactionTimestamps = [
    ...positions.filter(p => !!p.isClicked).map(p => p.timestamp),
    ...mouseEvents.map(e => e.startTime),
    ...mouseEvents.filter(e => e.endTime > e.startTime).map(e => e.endTime),
  ];

  // Returns true if any interaction occurred within CLICK_ACTIVE_DURATION seconds before t.
  function withinInteractionWindow(t: number): boolean {
    return interactionTimestamps.some(ct => t >= ct && t - ct <= CLICK_ACTIVE_DURATION);
  }

  // Build activity timeline: for each position, determine if cursor is "active"
  const activeFlags: { time: number; active: boolean; clicked: boolean }[] = [];
  const decisionSamples: Array<{
    time: number;
    meaningfulMovement: boolean;
    nearInteraction: boolean;
    clicked: boolean;
    netDistance: number;
    pathDistance: number;
    centerLockOverride: boolean;
    finalActive: boolean;
  }> = [];

  for (let i = 0; i < motionPositions.length; i++) {
    const t = motionPositions[i].timestamp;

    // Sliding-window velocity check
    const windowStart = t - VELOCITY_WINDOW / 2;
    const windowEnd = t + VELOCITY_WINDOW / 2;
    const windowPositions = motionPositions.filter(
      p => p.timestamp >= windowStart && p.timestamp <= windowEnd
    );

    let netDistance = 0;
    let pathDistance = 0;
    const clicked = windowPositions.some(p => !!p.isClicked) ||
      mouseEvents.some(e => t >= e.startTime && t <= e.endTime);

    if (windowPositions.length >= 2) {
      const first = windowPositions[0];
      const last = windowPositions[windowPositions.length - 1];
      const dt = last.timestamp - first.timestamp;
      if (dt > 0) {
        const dx = last.x - first.x;
        const dy = last.y - first.y;
        netDistance = Math.sqrt(dx * dx + dy * dy);
      }

      for (let j = 1; j < windowPositions.length; j++) {
        const dx = windowPositions[j].x - windowPositions[j - 1].x;
        const dy = windowPositions[j].y - windowPositions[j - 1].y;
        pathDistance += Math.sqrt(dx * dx + dy * dy);
      }
    }

    // Any detectable movement keeps cursor active (thresholds lowered vs velocity-based check).
    const meaningfulMovement =
      netDistance >= ACTIVE_NET_DISTANCE_MIN || pathDistance >= ACTIVE_PATH_DISTANCE_MIN;

    // Keep active during and for CLICK_ACTIVE_DURATION after any click/scroll.
    const nearInteraction = withinInteractionWindow(t);

    const active = clicked || nearInteraction || meaningfulMovement;
    activeFlags.push({ time: t, active, clicked });
    decisionSamples.push({
      time: t,
      meaningfulMovement,
      nearInteraction,
      clicked,
      netDistance,
      pathDistance,
      centerLockOverride: false,
      finalActive: active,
    });
  }

  // The only movement exception allowed to hide is center-lock gameplay:
  // sustained movement inside a strict box around the exact source-frame center.
  const { width: sourceWidth, height: sourceHeight } = getFrameDimensions(
    motionPositions,
    frameWidth,
    frameHeight
  );
  const centerLockRanges: { start: number; end: number }[] = [];
  for (let i = 0; i < motionPositions.length; i++) {
    const t = motionPositions[i].timestamp;

    // Check if positions over CENTER_LOCK_DURATION all stay within the strict center box.
    const windowPositions = motionPositions.filter(
      p => p.timestamp >= t && p.timestamp <= t + CENTER_LOCK_DURATION
    );

    if (windowPositions.length < 2) continue;

    const first = windowPositions[0];
    const last = windowPositions[windowPositions.length - 1];
    const windowDuration = last.timestamp - first.timestamp;
    if (windowDuration < CENTER_LOCK_DURATION * 0.8) continue;

    const fullyInsideCenterBox = windowPositions.every((position) =>
      isInsideCenterLockBox(position, sourceWidth, sourceHeight)
    );

    if (fullyInsideCenterBox) {
      centerLockRanges.push({
        start: first.timestamp,
        end: last.timestamp,
      });
    }
  }

  // Merge center-lock ranges to make membership checks stable.
  const mergedCenterLock: { start: number; end: number }[] = [];
  for (const range of centerLockRanges) {
    if (
      mergedCenterLock.length > 0 &&
      range.start <= mergedCenterLock[mergedCenterLock.length - 1].end + 0.05
    ) {
      mergedCenterLock[mergedCenterLock.length - 1].end = Math.max(
        mergedCenterLock[mergedCenterLock.length - 1].end,
        range.end
      );
    } else {
      mergedCenterLock.push({ ...range });
    }
  }

  // Inside the strict center-lock box, clicks and micro-movement are allowed to stay hidden.
  // Everywhere else, any movement must keep the cursor visible.
  if (mergedCenterLock.length > 0) {
    for (let i = 0; i < activeFlags.length; i++) {
      const flag = activeFlags[i];
      const centerLockRange = mergedCenterLock.find(
        range => flag.time >= range.start && flag.time <= range.end
      );
      if (!centerLockRange) continue;
      flag.active = false;
      decisionSamples[i].centerLockOverride = true;
      decisionSamples[i].finalActive = false;
    }
  }

  // Find consecutive idle runs exceeding IDLE_DURATION_THRESHOLD
  // Then build visible intervals from the active runs
  const idleRanges: { start: number; end: number }[] = [];
  let idleStart: number | null = activeFlags[0] && !activeFlags[0].active ? timelineStart : null;

  for (let i = 0; i < activeFlags.length; i++) {
    if (!activeFlags[i].active) {
      if (idleStart === null) idleStart = activeFlags[i].time;
    } else {
      if (idleStart !== null) {
        const idleEnd = activeFlags[i].time;
        if (idleEnd - idleStart >= IDLE_DURATION_THRESHOLD) {
          idleRanges.push({ start: idleStart, end: idleEnd });
        }
        idleStart = null;
      }
    }
  }
  // Handle trailing idle
  if (idleStart !== null) {
    const idleEnd = timelineEnd;
    if (idleEnd - idleStart >= IDLE_DURATION_THRESHOLD) {
      idleRanges.push({ start: idleStart, end: idleEnd });
    }
  }

  const transitions = decisionSamples.reduce<Array<{
    time: number;
    state: 'active' | 'idle';
    meaningfulMovement: boolean;
    nearInteraction: boolean;
    clicked: boolean;
    centerLockOverride: boolean;
    netDistance: number;
    pathDistance: number;
  }>>((acc, sample) => {
    const state = sample.finalActive ? 'active' : 'idle';
    const previous = acc[acc.length - 1];
    if (
      previous &&
      previous.state === state &&
      previous.meaningfulMovement === sample.meaningfulMovement &&
      previous.nearInteraction === sample.nearInteraction &&
      previous.clicked === sample.clicked &&
      previous.centerLockOverride === sample.centerLockOverride
    ) {
      return acc;
    }
    acc.push({
      time: Math.round(sample.time * 1000) / 1000,
      state,
      meaningfulMovement: sample.meaningfulMovement,
      nearInteraction: sample.nearInteraction,
      clicked: sample.clicked,
      centerLockOverride: sample.centerLockOverride,
      netDistance: Math.round(sample.netDistance * 1000) / 1000,
      pathDistance: Math.round(sample.pathDistance * 1000) / 1000,
    });
    return acc;
  }, []);

  // Invert idle ranges to get visible ranges
  const visibleRanges: { start: number; end: number }[] = [];
  let cursor = timelineStart;

  for (const idle of idleRanges) {
    if (idle.start > cursor) {
      visibleRanges.push({ start: cursor, end: idle.start });
    }
    cursor = idle.end;
  }
  if (cursor < timelineEnd) {
    visibleRanges.push({ start: cursor, end: timelineEnd });
  }

  // Post-process: extend margins
  const extended = visibleRanges.map(r => ({
    start: r.start - MARGIN_BEFORE,
    end: r.end + MARGIN_AFTER,
  }));

  // Merge gaps smaller than MIN_GAP_TO_MERGE
  const merged: { start: number; end: number }[] = [];
  for (const r of extended) {
    if (merged.length > 0 && r.start - merged[merged.length - 1].end < MIN_GAP_TO_MERGE) {
      merged[merged.length - 1].end = Math.max(merged[merged.length - 1].end, r.end);
    } else {
      merged.push({ ...r });
    }
  }

  const visibleSegments = idleRanges.length === 0
    ? [{
      id: crypto.randomUUID(),
      startTime: timelineStart,
      endTime: timelineEnd,
    }]
    : merged
      .map(r => ({
        id: crypto.randomUUID(),
        startTime: Math.max(timelineStart, r.start),
        endTime: Math.min(timelineEnd, r.end),
      }))
      .filter(s => s.endTime > s.startTime);

  logSmartPointerGeneration({
    timelineEnd,
    sampleCount: positions.length,
    motionSampleCount: motionPositions.length,
    sourceWidth,
    sourceHeight,
    centerLockHalfSize: getCenterLockHalfSize(sourceWidth, sourceHeight),
    visibleSegments: visibleSegments.map((visibleSegment) => ({
      start: Math.round(visibleSegment.startTime * 1000) / 1000,
      end: Math.round(visibleSegment.endTime * 1000) / 1000,
    })),
    idleRanges: idleRanges.map((range) => ({
      start: Math.round(range.start * 1000) / 1000,
      end: Math.round(range.end * 1000) / 1000,
    })),
    transitions,
  });

  return visibleSegments;
}

/**
 * Pure, deterministic function to compute cursor visibility at a given time.
 * Used identically for preview AND export baking (WYSIWYG).
 *
 * @param time - Current playback time
 * @param segments - Cursor visibility segments (visible periods), or undefined if feature is off
 * @returns { opacity, scale } for the cursor at this time
 */
export function getCursorVisibility(
  time: number,
  segments: CursorVisibilitySegment[] | undefined
): { opacity: number; scale: number } {
  // Feature off — cursor always visible
  if (!segments) {
    return { opacity: 1.0, scale: 1.0 };
  }

  // Feature active but no segments — cursor always hidden
  if (segments.length === 0) {
    return { opacity: 0.0, scale: SCALE_HIDDEN };
  }

  // Check each segment. Fade-in and fade-out happen inside the segment window so
  // the animation can overlap the beginning/end of actual cursor motion.
  for (const seg of segments) {
    if (time < seg.startTime || time > seg.endTime) {
      continue;
    }

    const { fadeIn, fadeOut } = getSegmentFadeDurations(seg.startTime, seg.endTime);
    const fadeInEnd = seg.startTime + fadeIn;
    const fadeOutStart = seg.endTime - fadeOut;

    if (fadeIn > 0 && time < fadeInEnd) {
      const t = (time - seg.startTime) / fadeIn;
      const eased = easeOutCubic(Math.max(0, Math.min(1, t)));
      return {
        opacity: eased,
        scale: SCALE_HIDDEN + (SCALE_VISIBLE - SCALE_HIDDEN) * eased,
      };
    }

    if (fadeOut > 0 && time > fadeOutStart) {
      const t = (time - fadeOutStart) / fadeOut;
      const eased = 1 - easeInCubic(Math.max(0, Math.min(1, t)));
      return {
        opacity: eased,
        scale: SCALE_HIDDEN + (SCALE_VISIBLE - SCALE_HIDDEN) * eased,
      };
    }

    return { opacity: 1.0, scale: 1.0 };
  }

  // Outside all segments — fully hidden
  return { opacity: 0.0, scale: SCALE_HIDDEN };
}
