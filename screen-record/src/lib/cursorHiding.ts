import { CursorVisibilitySegment, MousePosition, VideoSegment } from '@/types/video';

// --- Configuration ---
const IDLE_VELOCITY_THRESHOLD = 2;    // px/s — below this is considered idle
const IDLE_DURATION_THRESHOLD = 1.5;  // seconds of low velocity to trigger idle
const ANCHORED_RADIUS = 18;           // px — sustained drift within this radius counts as anchored idle
const ANCHORED_DURATION = 0.9;        // seconds to confirm anchored idle (faster for center-lock gameplay)
const VELOCITY_WINDOW = 0.1;          // seconds — sliding window for velocity calc
const ACTIVE_NET_DISTANCE_MIN = 3.5;  // px — min first→last displacement in the velocity window to count as active
const ACTIVE_PATH_DISTANCE_MIN = 6;   // px — min accumulated path distance in window to count as active
const MARGIN_BEFORE = 0.3;            // seconds — show cursor before movement starts
const MARGIN_AFTER = 0.2;             // seconds — keep cursor visible after movement stops
const MIN_GAP_TO_MERGE = 0.5;        // seconds — merge visible segments closer than this

// --- Animation ---
const FADE_IN_DURATION = 0.2;         // seconds — entrance animation
const FADE_OUT_DURATION = 0.25;       // seconds — dismissal animation
const SCALE_HIDDEN = 0.5;             // scale when fully hidden
const SCALE_VISIBLE = 1.0;            // scale when fully visible

// Easing functions
function easeOutCubic(t: number): number {
  return 1 - Math.pow(1 - t, 3);
}

function easeInCubic(t: number): number {
  return t * t * t;
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
  timelineDuration?: number
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

  if (positions.length < 2) {
    // Not enough data — return one segment covering whole timeline (always visible)
    return [{
      id: crypto.randomUUID(),
      startTime: timelineStart,
      endTime: timelineEnd,
    }];
  }

  // Build activity timeline: for each position, determine if cursor is "active"
  const activeFlags: { time: number; active: boolean; clicked: boolean }[] = [];

  for (let i = 0; i < positions.length; i++) {
    const t = positions[i].timestamp;

    // Sliding-window velocity check
    const windowStart = t - VELOCITY_WINDOW / 2;
    const windowEnd = t + VELOCITY_WINDOW / 2;
    const windowPositions = positions.filter(
      p => p.timestamp >= windowStart && p.timestamp <= windowEnd
    );

    let velocity = 0;
    let netDistance = 0;
    let pathDistance = 0;
    const clicked = windowPositions.some(p => !!p.isClicked);

    if (windowPositions.length >= 2) {
      const first = windowPositions[0];
      const last = windowPositions[windowPositions.length - 1];
      const dt = last.timestamp - first.timestamp;
      if (dt > 0) {
        const dx = last.x - first.x;
        const dy = last.y - first.y;
        velocity = Math.sqrt(dx * dx + dy * dy) / dt;
        netDistance = Math.sqrt(dx * dx + dy * dy);
      }

      for (let j = 1; j < windowPositions.length; j++) {
        const dx = windowPositions[j].x - windowPositions[j - 1].x;
        const dy = windowPositions[j].y - windowPositions[j - 1].y;
        pathDistance += Math.sqrt(dx * dx + dy * dy);
      }
    }

    const meaningfulMovement =
      velocity >= IDLE_VELOCITY_THRESHOLD &&
      (netDistance >= ACTIVE_NET_DISTANCE_MIN || pathDistance >= ACTIVE_PATH_DISTANCE_MIN);

    activeFlags.push({ time: t, active: clicked || meaningfulMovement, clicked });
  }

  // Anchored detection: sustained micro-drift (common in locked-center gameplay) should be treated as idle.
  const anchoredRanges: { start: number; end: number }[] = [];
  for (let i = 0; i < positions.length; i++) {
    const t = positions[i].timestamp;

    // Check if positions over ANCHORED_DURATION all stay within ANCHORED_RADIUS
    const windowPositions = positions.filter(
      p => p.timestamp >= t && p.timestamp <= t + ANCHORED_DURATION
    );

    if (windowPositions.length < 2) continue;

    const first = windowPositions[0];
    const last = windowPositions[windowPositions.length - 1];
    const windowDuration = last.timestamp - first.timestamp;
    if (windowDuration < ANCHORED_DURATION * 0.8) continue;

    const cx = windowPositions.reduce((sum, p) => sum + p.x, 0) / windowPositions.length;
    const cy = windowPositions.reduce((sum, p) => sum + p.y, 0) / windowPositions.length;
    const maxDist = windowPositions.reduce((max, p) => {
      const dx = p.x - cx;
      const dy = p.y - cy;
      return Math.max(max, Math.sqrt(dx * dx + dy * dy));
    }, 0);

    if (maxDist <= ANCHORED_RADIUS) {
      anchoredRanges.push({ start: first.timestamp, end: last.timestamp });
    }
  }

  // Merge anchored ranges to make membership checks stable.
  const mergedAnchored: { start: number; end: number }[] = [];
  for (const range of anchoredRanges) {
    if (
      mergedAnchored.length > 0 &&
      range.start <= mergedAnchored[mergedAnchored.length - 1].end + 0.05
    ) {
      mergedAnchored[mergedAnchored.length - 1].end = Math.max(
        mergedAnchored[mergedAnchored.length - 1].end,
        range.end
      );
    } else {
      mergedAnchored.push({ ...range });
    }
  }

  // Force anchored ranges to idle unless there is an actual click signal.
  if (mergedAnchored.length > 0) {
    for (const flag of activeFlags) {
      if (flag.clicked) continue;
      const inAnchoredRange = mergedAnchored.some(
        range => flag.time >= range.start && flag.time <= range.end
      );
      if (inAnchoredRange) {
        flag.active = false;
      }
    }
  }

  // Find consecutive idle runs exceeding IDLE_DURATION_THRESHOLD
  // Then build visible intervals from the active runs
  const idleRanges: { start: number; end: number }[] = [];
  let idleStart: number | null = null;

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
    const idleEnd = positions[positions.length - 1].timestamp;
    if (idleEnd - idleStart >= IDLE_DURATION_THRESHOLD) {
      idleRanges.push({ start: idleStart, end: idleEnd });
    }
  }

  if (idleRanges.length === 0) {
    // No idle detected — cursor visible the whole time
    return [{
      id: crypto.randomUUID(),
      startTime: timelineStart,
      endTime: timelineEnd,
    }];
  }

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

  // Clip to full timeline range and assign UUIDs
  return merged
    .map(r => ({
      id: crypto.randomUUID(),
      startTime: Math.max(timelineStart, r.start),
      endTime: Math.min(timelineEnd, r.end),
    }))
    .filter(s => s.endTime > s.startTime);
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

  // Check each segment for containment or proximity
  for (const seg of segments) {
    // Inside segment — fully visible
    if (time >= seg.startTime && time <= seg.endTime) {
      // Check entrance fade
      const entranceStart = seg.startTime - FADE_IN_DURATION;
      if (time < seg.startTime) {
        // In entrance zone (shouldn't reach here since time >= startTime, but guard)
        const t = (time - entranceStart) / FADE_IN_DURATION;
        const eased = easeOutCubic(Math.max(0, Math.min(1, t)));
        return {
          opacity: eased,
          scale: SCALE_HIDDEN + (SCALE_VISIBLE - SCALE_HIDDEN) * eased,
        };
      }
      return { opacity: 1.0, scale: 1.0 };
    }

    // Entrance zone: [startTime - FADE_IN_DURATION, startTime]
    if (time >= seg.startTime - FADE_IN_DURATION && time < seg.startTime) {
      const t = (time - (seg.startTime - FADE_IN_DURATION)) / FADE_IN_DURATION;
      const eased = easeOutCubic(Math.max(0, Math.min(1, t)));
      return {
        opacity: eased,
        scale: SCALE_HIDDEN + (SCALE_VISIBLE - SCALE_HIDDEN) * eased,
      };
    }

    // Dismissal zone: [endTime, endTime + FADE_OUT_DURATION]
    if (time > seg.endTime && time <= seg.endTime + FADE_OUT_DURATION) {
      const t = (time - seg.endTime) / FADE_OUT_DURATION;
      const eased = 1 - easeInCubic(Math.max(0, Math.min(1, t)));
      return {
        opacity: eased,
        scale: SCALE_HIDDEN + (SCALE_VISIBLE - SCALE_HIDDEN) * eased,
      };
    }
  }

  // Outside all segments — fully hidden
  return { opacity: 0.0, scale: SCALE_HIDDEN };
}
