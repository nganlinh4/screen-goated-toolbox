import { CursorVisibilitySegment, KeystrokeEvent, KeystrokeMode, VideoSegment } from '@/types/video';
import { mergePointerSegments } from '@/lib/cursorHiding';

export const KEYSTROKE_VISIBILITY_MARGIN_BEFORE = 0.04;
export const KEYSTROKE_VISIBILITY_MARGIN_AFTER = 0.08;
const MIN_GAP_TO_MERGE = 0.2;
type ActiveKeystrokeMode = Exclude<KeystrokeMode, 'off'>;

function clamp(v: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, v));
}

interface KeystrokeVisibilityBuildOptions {
  mode?: ActiveKeystrokeMode;
  delaySec?: number;
}

function areVisibilitySegmentsEquivalent(
  a: CursorVisibilitySegment[] | undefined,
  b: CursorVisibilitySegment[] | undefined
): boolean {
  const left = a ?? [];
  const right = b ?? [];
  if (left.length !== right.length) return false;
  for (let i = 0; i < left.length; i++) {
    if (Math.abs(left[i].startTime - right[i].startTime) > 0.0005) return false;
    if (Math.abs(left[i].endTime - right[i].endTime) > 0.0005) return false;
  }
  return true;
}

export function filterKeystrokeEventsByMode(
  events: KeystrokeEvent[],
  mode: KeystrokeMode
): KeystrokeEvent[] {
  if (mode === 'keyboard') {
    return events.filter((event) => event.type === 'keyboard');
  }
  if (mode === 'keyboardMouse') {
    return events;
  }
  return [];
}

function withVisibilitySegmentsForExactMode(
  segment: VideoSegment,
  mode: ActiveKeystrokeMode,
  segments: CursorVisibilitySegment[]
): VideoSegment {
  if (mode === 'keyboard') {
    return { ...segment, keyboardVisibilitySegments: mergePointerSegments(segments) };
  }
  return { ...segment, keyboardMouseVisibilitySegments: mergePointerSegments(segments) };
}

export function generateKeystrokeVisibilitySegments(
  events: KeystrokeEvent[],
  duration: number,
  options?: KeystrokeVisibilityBuildOptions
): CursorVisibilitySegment[] {
  const safeDuration = Math.max(duration, 0);
  if (!events.length || safeDuration <= 0) return [];

  const mode = options?.mode ?? 'keyboardMouse';
  const delaySec = clamp(options?.delaySec ?? 0, -1, 1);
  const sorted = [...events].sort((a, b) => a.startTime - b.startTime);
  const effectiveEnds = new Array<number>(sorted.length);
  let nextAnyStart = Number.POSITIVE_INFINITY;
  let nextKeyboardStart = Number.POSITIVE_INFINITY;
  let nextMouseStart = Number.POSITIVE_INFINITY;

  for (let i = sorted.length - 1; i >= 0; i--) {
    const event = sorted[i];
    const nextStart = mode === 'keyboardMouse'
      ? (event.type === 'keyboard' ? nextKeyboardStart : nextMouseStart)
      : nextAnyStart;
    effectiveEnds[i] = Math.min(event.endTime, nextStart, safeDuration);
    nextAnyStart = event.startTime;
    if (event.type === 'keyboard') {
      nextKeyboardStart = event.startTime;
    } else {
      nextMouseStart = event.startTime;
    }
  }

  const raw: CursorVisibilitySegment[] = [];
  for (let i = 0; i < sorted.length; i++) {
    const event = sorted[i];
    const shiftedStart = clamp(event.startTime + delaySec, 0, safeDuration);
    const shiftedEnd = clamp(effectiveEnds[i] + delaySec, 0, safeDuration);
    if (shiftedEnd - shiftedStart <= 0.001) continue;
    const startTime = clamp(shiftedStart - KEYSTROKE_VISIBILITY_MARGIN_BEFORE, 0, safeDuration);
    const endTime = clamp(shiftedEnd + KEYSTROKE_VISIBILITY_MARGIN_AFTER, 0, safeDuration);
    if (endTime - startTime <= 0.001) continue;
    raw.push({
      id: crypto.randomUUID(),
      startTime,
      endTime,
    });
  }
  raw.sort((a, b) => a.startTime - b.startTime);

  if (!raw.length) return [];

  const merged: CursorVisibilitySegment[] = [{ ...raw[0] }];
  for (let i = 1; i < raw.length; i++) {
    const prev = merged[merged.length - 1];
    const current = raw[i];
    if (current.startTime <= prev.endTime + MIN_GAP_TO_MERGE) {
      prev.endTime = Math.max(prev.endTime, current.endTime);
    } else {
      merged.push({ ...current });
    }
  }

  return mergePointerSegments(merged);
}

export function getKeystrokeVisibilitySegmentsForMode(
  segment: VideoSegment
): CursorVisibilitySegment[] {
  const mode = segment.keystrokeMode ?? 'off';
  if (mode === 'keyboard') return segment.keyboardVisibilitySegments ?? [];
  if (mode === 'keyboardMouse') return segment.keyboardMouseVisibilitySegments ?? [];
  return [];
}

export function withKeystrokeVisibilitySegmentsForMode(
  segment: VideoSegment,
  segments: CursorVisibilitySegment[]
): VideoSegment {
  const mode = segment.keystrokeMode ?? 'off';
  if (mode === 'keyboard' || mode === 'keyboardMouse') {
    return withVisibilitySegmentsForExactMode(segment, mode, segments);
  }
  return segment;
}

export function rebuildKeystrokeVisibilitySegmentsForMode(
  segment: VideoSegment,
  mode: ActiveKeystrokeMode,
  duration: number
): VideoSegment {
  const events = filterKeystrokeEventsByMode(segment.keystrokeEvents ?? [], mode);
  const delaySec = clamp(segment.keystrokeDelaySec ?? 0, -1, 1);
  const rebuilt = generateKeystrokeVisibilitySegments(events, duration, { mode, delaySec });
  return withVisibilitySegmentsForExactMode(segment, mode, rebuilt);
}

export function ensureKeystrokeVisibilitySegments(
  segment: VideoSegment,
  duration: number
): VideoSegment {
  const allEvents = segment.keystrokeEvents ?? [];
  const keyboardEvents = filterKeystrokeEventsByMode(allEvents, 'keyboard');
  const keyboardMouseEvents = filterKeystrokeEventsByMode(allEvents, 'keyboardMouse');
  const delaySec = clamp(segment.keystrokeDelaySec ?? 0, -1, 1);
  const keyboardAutoWithDelay = generateKeystrokeVisibilitySegments(
    keyboardEvents,
    duration,
    { mode: 'keyboard', delaySec }
  );
  const keyboardMouseAutoWithDelay = generateKeystrokeVisibilitySegments(
    keyboardMouseEvents,
    duration,
    { mode: 'keyboardMouse', delaySec }
  );
  const keyboardAutoNoDelay = generateKeystrokeVisibilitySegments(
    keyboardEvents,
    duration,
    { mode: 'keyboard', delaySec: 0 }
  );
  const keyboardMouseAutoNoDelay = generateKeystrokeVisibilitySegments(
    keyboardMouseEvents,
    duration,
    { mode: 'keyboardMouse', delaySec: 0 }
  );

  return {
    ...segment,
    keyboardVisibilitySegments: (() => {
      const existing = segment.keyboardVisibilitySegments;
      if (!existing) return keyboardAutoWithDelay;
      const shouldMigrateAutoFromNoDelay = delaySec !== 0
        && areVisibilitySegmentsEquivalent(existing, keyboardAutoNoDelay)
        && !areVisibilitySegmentsEquivalent(existing, keyboardAutoWithDelay);
      return shouldMigrateAutoFromNoDelay ? keyboardAutoWithDelay : existing;
    })(),
    keyboardMouseVisibilitySegments: (() => {
      const existing = segment.keyboardMouseVisibilitySegments;
      if (!existing) return keyboardMouseAutoWithDelay;
      const shouldMigrateAutoFromNoDelay = delaySec !== 0
        && areVisibilitySegmentsEquivalent(existing, keyboardMouseAutoNoDelay)
        && !areVisibilitySegmentsEquivalent(existing, keyboardMouseAutoWithDelay);
      return shouldMigrateAutoFromNoDelay ? keyboardMouseAutoWithDelay : existing;
    })(),
  };
}
