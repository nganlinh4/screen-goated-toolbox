import { CursorVisibilitySegment, KeystrokeEvent, KeystrokeMode, VideoSegment } from '@/types/video';
import { mergePointerSegments } from '@/lib/cursorHiding';

export const KEYSTROKE_VISIBILITY_MARGIN_BEFORE = 0.04;
export const KEYSTROKE_VISIBILITY_MARGIN_AFTER = 0.08;
const MIN_GAP_TO_MERGE = 0.2;
type ActiveKeystrokeMode = Exclude<KeystrokeMode, 'off'>;

function clamp(v: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, v));
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
  duration: number
): CursorVisibilitySegment[] {
  const safeDuration = Math.max(duration, 0);
  if (!events.length || safeDuration <= 0) return [];

  const raw = events
    .map((event) => ({
      id: crypto.randomUUID(),
      startTime: clamp(event.startTime - KEYSTROKE_VISIBILITY_MARGIN_BEFORE, 0, safeDuration),
      endTime: clamp(event.endTime + KEYSTROKE_VISIBILITY_MARGIN_AFTER, 0, safeDuration),
    }))
    .filter((segment) => segment.endTime - segment.startTime > 0.001)
    .sort((a, b) => a.startTime - b.startTime);

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
  const rebuilt = generateKeystrokeVisibilitySegments(events, duration);
  return withVisibilitySegmentsForExactMode(segment, mode, rebuilt);
}

export function ensureKeystrokeVisibilitySegments(
  segment: VideoSegment,
  duration: number
): VideoSegment {
  const allEvents = segment.keystrokeEvents ?? [];
  const keyboardEvents = filterKeystrokeEventsByMode(allEvents, 'keyboard');
  const keyboardMouseEvents = filterKeystrokeEventsByMode(allEvents, 'keyboardMouse');

  return {
    ...segment,
    keyboardVisibilitySegments: segment.keyboardVisibilitySegments
      ?? generateKeystrokeVisibilitySegments(keyboardEvents, duration),
    keyboardMouseVisibilitySegments: segment.keyboardMouseVisibilitySegments
      ?? generateKeystrokeVisibilitySegments(keyboardMouseEvents, duration),
  };
}
