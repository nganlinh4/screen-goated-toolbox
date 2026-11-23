/**
 * Pure range math for the compact (block) view of an audio gain envelope.
 * Edits keep each source range's interior automation (ducking, fades drawn in
 * volume view); only ranges with no source, such as newly added blocks, are flat.
 */
export interface SoftAudioPoint {
  time: number;
  volume: number;
}

export interface SoftAudioRange {
  id: string;
  startTime: number;
  endTime: number;
  volume: number;
  /** Range in the pre-edit envelope whose interior points this range carries. */
  sourceId?: string;
  /** Time shift applied to the carried points (body drags). */
  offset?: number;
}

export interface ResolvedSoftAudioRange extends SoftAudioRange {
  parts: SoftAudioRange[];
}

export const AUDIO_SEGMENT_MIN_SEC = 0.05;
export const AUDIO_SEGMENT_DEFAULT_SEC = 2;
const AUDIO_SEGMENT_EDGE_EPS_SEC = 0.001;
export const AUDIO_SEGMENT_SPLIT_GAP_SEC = 0.04;
const AUDIO_SEGMENT_FADE_SEC = 0.12;
const AUDIBLE_VOLUME_EPS = 0.0001;

export function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

function normalizePoints(points: readonly SoftAudioPoint[], duration: number) {
  const safeDuration = Math.max(duration, AUDIO_SEGMENT_MIN_SEC);
  return [...points]
    .filter((point) => Number.isFinite(point.time) && Number.isFinite(point.volume))
    .map((point) => ({
      time: clamp(point.time, 0, safeDuration),
      volume: clamp(point.volume, 0, 1),
    }))
    .sort((a, b) => a.time - b.time);
}

export function getRangesFromPoints(
  points: readonly SoftAudioPoint[],
  duration: number,
): SoftAudioRange[] {
  const safeDuration = Math.max(duration, AUDIO_SEGMENT_MIN_SEC);
  const sorted = normalizePoints(points, safeDuration);
  if (sorted.length === 0) return [];

  const ranges: SoftAudioRange[] = [];
  let startTime: number | null = null;
  let volume = 0;

  for (let index = 0; index < sorted.length; index += 1) {
    const point = sorted[index];
    const isAudible = point.volume > AUDIBLE_VOLUME_EPS;
    if (isAudible && startTime === null) {
      const previous = sorted[index - 1];
      startTime = previous && previous.volume <= AUDIBLE_VOLUME_EPS
        ? previous.time
        : point.time;
      volume = point.volume;
      continue;
    }

    if (isAudible) {
      volume = Math.max(volume, point.volume);
      continue;
    }

    if (startTime !== null) {
      const endTime = point.time;
      if (endTime - startTime >= AUDIO_SEGMENT_MIN_SEC) {
        ranges.push({
          id: `${startTime.toFixed(4)}-${endTime.toFixed(4)}-${ranges.length}`,
          startTime,
          endTime,
          volume: clamp(volume || 1, AUDIBLE_VOLUME_EPS, 1),
        });
      }
      startTime = null;
      volume = 0;
    }
  }

  if (startTime !== null && safeDuration - startTime >= AUDIO_SEGMENT_MIN_SEC) {
    ranges.push({
      id: `${startTime.toFixed(4)}-${safeDuration.toFixed(4)}-${ranges.length}`,
      startTime,
      endTime: safeDuration,
      volume: clamp(volume || 1, AUDIBLE_VOLUME_EPS, 1),
    });
  }

  return ranges;
}

function pushPoint(points: SoftAudioPoint[], duration: number, time: number, volume: number) {
  const clampedTime = clamp(time, 0, duration);
  const last = points[points.length - 1];
  if (last && Math.abs(last.time - clampedTime) < AUDIO_SEGMENT_EDGE_EPS_SEC / 2) {
    last.volume = volume;
    return;
  }
  points.push({ time: clampedTime, volume });
}

function interpolateVolume(points: readonly SoftAudioPoint[], time: number): number {
  if (time <= points[0].time) return points[0].volume;
  for (let index = 1; index < points.length; index += 1) {
    const next = points[index];
    if (time > next.time) continue;
    const previous = points[index - 1];
    const span = next.time - previous.time;
    return span <= 0
      ? next.volume
      : previous.volume + ((time - previous.time) / span) * (next.volume - previous.volume);
  }
  return points[points.length - 1].volume;
}

/** Interior points of a part's source range, shifted and clipped to the part. */
function carriedPoints(
  part: SoftAudioRange,
  sourcePoints: readonly SoftAudioPoint[],
  sourceRanges: ReadonlyMap<string, SoftAudioRange>,
): SoftAudioPoint[] {
  const source = part.sourceId ? sourceRanges.get(part.sourceId) : undefined;
  if (!source) return [];
  const offset = part.offset ?? 0;
  return sourcePoints
    .filter((point) => point.time > source.startTime && point.time < source.endTime)
    .map((point) => ({ time: point.time + offset, volume: clamp(point.volume, 0, 1) }))
    .filter((point) => point.time > part.startTime && point.time < part.endTime);
}

/**
 * Builds envelope points for resolved ranges. A range with carried source points
 * keeps them between its fades and enters/exits at the carried curve's level;
 * a range without them gets a flat plateau at its volume.
 */
export function buildPointsFromRanges(
  duration: number,
  ranges: readonly ResolvedSoftAudioRange[],
  sourcePoints: readonly SoftAudioPoint[] = [],
  sourceRanges: readonly SoftAudioRange[] = [],
): SoftAudioPoint[] {
  const safeDuration = Math.max(duration, AUDIO_SEGMENT_MIN_SEC);
  const sourcesById = new Map(sourceRanges.map((range) => [range.id, range]));
  const points: SoftAudioPoint[] = [];
  pushPoint(points, safeDuration, 0, 0);
  for (const range of ranges) {
    const start = range.startTime;
    const end = range.endTime;
    const fade = Math.min(AUDIO_SEGMENT_FADE_SEC, Math.max(0, (end - start) / 2));
    const carried = range.parts
      .flatMap((part) => carriedPoints(part, sourcePoints, sourcesById))
      .sort((a, b) => a.time - b.time);
    const volume = clamp(range.volume, 0, 1);
    const entryVolume = carried.length > 0 ? interpolateVolume(carried, start + fade) : volume;
    const exitVolume = carried.length > 0 ? interpolateVolume(carried, end - fade) : volume;
    if (start > AUDIO_SEGMENT_EDGE_EPS_SEC) {
      pushPoint(points, safeDuration, start - AUDIO_SEGMENT_EDGE_EPS_SEC, 0);
    }
    pushPoint(points, safeDuration, start, 0);
    pushPoint(points, safeDuration, start + fade, entryVolume);
    for (const point of carried) {
      if (point.time > start + fade && point.time < end - fade) {
        pushPoint(points, safeDuration, point.time, point.volume);
      }
    }
    pushPoint(points, safeDuration, end - fade, exitVolume);
    pushPoint(points, safeDuration, end, 0);
    if (end < safeDuration - AUDIO_SEGMENT_EDGE_EPS_SEC) {
      pushPoint(points, safeDuration, end + AUDIO_SEGMENT_EDGE_EPS_SEC, 0);
    }
  }
  pushPoint(points, safeDuration, safeDuration, 0);
  return points.sort((a, b) => a.time - b.time);
}

/** Clamps, sorts and merges overlapping ranges, remembering each merged part. */
export function resolveRanges(
  ranges: readonly SoftAudioRange[],
  duration: number,
): ResolvedSoftAudioRange[] {
  const safeDuration = Math.max(duration, AUDIO_SEGMENT_MIN_SEC);
  const sorted = [...ranges]
    .map((range) => ({
      ...range,
      startTime: clamp(range.startTime, 0, safeDuration - AUDIO_SEGMENT_MIN_SEC),
      endTime: clamp(range.endTime, AUDIO_SEGMENT_MIN_SEC, safeDuration),
      volume: clamp(range.volume, 0, 1),
    }))
    .filter((range) => range.endTime - range.startTime >= AUDIO_SEGMENT_MIN_SEC)
    .sort((a, b) => a.startTime - b.startTime);

  const resolved: ResolvedSoftAudioRange[] = [];
  for (const range of sorted) {
    const last = resolved[resolved.length - 1];
    if (!last || range.startTime > last.endTime) {
      resolved.push({ ...range, parts: [range] });
      continue;
    }
    last.endTime = Math.max(last.endTime, range.endTime);
    last.volume = Math.max(last.volume, range.volume);
    last.parts.push(range);
  }
  return resolved;
}
