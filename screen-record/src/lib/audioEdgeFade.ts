// Implicit edge fade for recorded device/mic audio, matching export.
//
// Export fades each source over the start and end of its audible footprint on
// the OUTPUT timeline: the kept trim regions that the source covers, mapped
// through the speed curve (native_export/audio_mix stretch_mix + time_map).
// Preview plays in project time, so it measures the output-time distance from
// the playhead to those same footprint edges with the same integration rule.

import type { SpeedPoint, TrimSegment } from '@/types/video';
import { IMPLICIT_AUDIO_EDGE_FADE_SEC } from './deviceAudio';
import { createSpeedSampler, type SpeedSampler } from './speedCurve';

/** Same step and midpoint rule as the export mixer's OutputTimeMapper. */
const INTEGRATION_STEP_SEC = 0.005;
const MIN_SPEED = 0.1;
const MAX_SPEED = 16;
const TIME_EPSILON = 1e-9;

export interface AudioFootprintOptions {
  /** Kept regions in project time, sorted and non-overlapping. */
  trimSegments: readonly TrimSegment[];
  speedPoints: readonly SpeedPoint[] | null | undefined;
  /** Where the source starts on the project timeline (its offset). */
  sourceStartSec: number;
  fadeSec?: number;
}

interface Span {
  start: number;
  end: number;
}

function footprintSpans(trimSegments: readonly TrimSegment[], sourceStartSec: number): Span[] {
  const spans: Span[] = [];
  for (const segment of trimSegments) {
    const start = Math.max(segment.startTime, sourceStartSec);
    if (segment.endTime > start + TIME_EPSILON) spans.push({ start, end: segment.endTime });
  }
  return spans;
}

/** Output seconds spent playing kept spans within [from, to], stopping once `cap` is reached. */
function outputDurationWithin(
  spans: readonly Span[],
  from: number,
  to: number,
  speedAt: SpeedSampler,
  cap: number,
): number {
  let total = 0;
  for (const span of spans) {
    const start = Math.max(span.start, from);
    const end = Math.min(span.end, to);
    let cursor = start;
    while (cursor < end - TIME_EPSILON) {
      const stepEnd = Math.min(cursor + INTEGRATION_STEP_SEC, end);
      const speed = Math.min(MAX_SPEED, Math.max(MIN_SPEED, speedAt((cursor + stepEnd) * 0.5)));
      total += (stepEnd - cursor) / speed;
      if (total >= cap) return cap;
      cursor = stepEnd;
    }
  }
  return total;
}

function raisedCosine(t: number): number {
  return (1 - Math.cos(t * Math.PI)) / 2;
}

/** Volume multiplier for the implicit fade at project `time`; 0 outside the footprint. */
export function getAudioFootprintEdgeFadeMultiplier(
  time: number,
  { trimSegments, speedPoints, sourceStartSec, fadeSec = IMPLICIT_AUDIO_EDGE_FADE_SEC }: AudioFootprintOptions,
): number {
  if (!Number.isFinite(time)) return 0;
  const spans = footprintSpans(trimSegments, Number.isFinite(sourceStartSec) ? sourceStartSec : 0);
  if (spans.length === 0) return 0;
  const footprintStart = spans[0].start;
  const footprintEnd = spans[spans.length - 1].end;
  if (time <= footprintStart || time >= footprintEnd) return 0;
  if (fadeSec <= 0) return 1;

  const speedAt = createSpeedSampler(speedPoints);
  // Capping each side at twice the fade still yields the exact fade length:
  // a capped side alone makes the footprint at least 2 * fadeSec long.
  const cap = fadeSec * 2;
  const elapsed = outputDurationWithin(spans, footprintStart, time, speedAt, cap);
  const remaining = outputDurationWithin(spans, time, footprintEnd, speedAt, cap);
  const fade = Math.min(fadeSec, (elapsed + remaining) / 2);
  if (fade <= 0) return 1;
  const fadeIn = elapsed < fade ? raisedCosine(elapsed / fade) : 1;
  const fadeOut = remaining < fade ? raisedCosine(remaining / fade) : 1;
  return Math.min(1, Math.max(0, fadeIn * fadeOut));
}
