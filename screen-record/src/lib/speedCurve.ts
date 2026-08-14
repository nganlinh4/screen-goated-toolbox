import type { SpeedPoint } from "@/types/video";

export type SpeedSampler = (time: number) => number;

const preparedCurveCache = new WeakMap<
  readonly SpeedPoint[],
  readonly SpeedPoint[]
>();

function isSorted(points: readonly SpeedPoint[]): boolean {
  for (let index = 1; index < points.length; index += 1) {
    if (points[index - 1].time > points[index].time) return false;
  }
  return true;
}

export function prepareSpeedCurve(
  points: readonly SpeedPoint[] | null | undefined,
): readonly SpeedPoint[] {
  if (!points || points.length === 0) return [];
  const cached = preparedCurveCache.get(points);
  if (cached) return cached;

  const prepared = isSorted(points)
    ? points
    : [...points].sort((left, right) => left.time - right.time);
  preparedCurveCache.set(points, prepared);
  return prepared;
}

export function samplePreparedSpeedCurve(
  time: number,
  points: readonly SpeedPoint[],
): number {
  if (points.length === 0) return 1;

  let low = 0;
  let high = points.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (points[middle].time < time) low = middle + 1;
    else high = middle;
  }

  if (low === 0) return points[0].speed;
  if (low >= points.length) return points[points.length - 1].speed;
  const previous = points[low - 1];
  const next = points[low];
  const ratio =
    (time - previous.time) / Math.max(1e-9, next.time - previous.time);
  const cosineRatio = (1 - Math.cos(ratio * Math.PI)) / 2;
  return previous.speed + (next.speed - previous.speed) * cosineRatio;
}

export function createSpeedSampler(
  points: readonly SpeedPoint[] | null | undefined,
): SpeedSampler {
  const prepared = prepareSpeedCurve(points);
  return (time) => samplePreparedSpeedCurve(time, prepared);
}

export function getSpeedAtTime(
  time: number,
  points: readonly SpeedPoint[] | null | undefined,
): number {
  return samplePreparedSpeedCurve(time, prepareSpeedCurve(points));
}
