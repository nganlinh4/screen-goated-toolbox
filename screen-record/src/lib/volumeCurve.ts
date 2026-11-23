import { clamp } from "@/lib/mathUtils";

export interface VolumeCurvePoint {
  time: number;
  volume: number;
}

// Point arrays are immutable segment state, so each array is sorted at most once.
const sortedPointsCache = new WeakMap<readonly VolumeCurvePoint[], readonly VolumeCurvePoint[]>();

function getSortedPoints(points: readonly VolumeCurvePoint[]): readonly VolumeCurvePoint[] {
  const cached = sortedPointsCache.get(points);
  if (cached) return cached;
  let isSorted = true;
  for (let index = 1; index < points.length; index += 1) {
    if (points[index].time < points[index - 1].time) {
      isSorted = false;
      break;
    }
  }
  const sorted = isSorted ? points : [...points].sort((a, b) => a.time - b.time);
  sortedPointsCache.set(points, sorted);
  return sorted;
}

/** Index of the first point at or after `time`, or -1 when every point is earlier. */
function findFirstPointAtOrAfter(sorted: readonly VolumeCurvePoint[], time: number): number {
  let low = 0;
  let high = sorted.length;
  while (low < high) {
    const mid = (low + high) >>> 1;
    if (sorted[mid].time >= time) high = mid;
    else low = mid + 1;
  }
  return low < sorted.length ? low : -1;
}

/**
 * Cosine-eased volume at `time` for a mic or device volume curve. Callers supply
 * their track's default (used when there are no points) and clamping rule.
 */
export function getCurveVolumeAtTime(
  time: number,
  points: readonly VolumeCurvePoint[] | undefined | null,
  defaultVolume: number,
  clampVolume: (volume: number) => number,
): number {
  if (!points || points.length === 0) return defaultVolume;
  const sorted = getSortedPoints(points);
  const index = findFirstPointAtOrAfter(sorted, time);
  if (index === -1) return clampVolume(sorted[sorted.length - 1].volume);
  if (index === 0) return clampVolume(sorted[0].volume);

  const left = sorted[index - 1];
  const right = sorted[index];
  const ratio = clamp((time - left.time) / Math.max(0.0001, right.time - left.time), 0, 1);
  const eased = (1 - Math.cos(ratio * Math.PI)) / 2;
  return clampVolume(left.volume + (right.volume - left.volume) * eased);
}
