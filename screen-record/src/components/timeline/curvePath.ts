import {
  type AdjacentSegmentIndices,
  sortPointsByTime,
} from "./adjustableLineUtils";

// Shared cubic-bezier SVG path builders for the timeline value-curve tracks
// (zoom, speed, and audio/mic/device volume). Each track supplies a `toY`
// point→y mapping plus its `baselineY` (fill floor) and `emptyPathY` (flat line
// when there are no points); the curve algorithm is identical across all of them.

export interface CurvePoint {
  time: number;
}

interface CurvePathParams<T extends CurvePoint> {
  points: T[];
  duration: number;
  toY: (point: T) => number;
  emptyPathY: number;
}

interface CurveFillParams<T extends CurvePoint> {
  points: T[];
  duration: number;
  toY: (point: T) => number;
  baselineY: number;
}

interface HighlightedSegmentParams<T extends CurvePoint> {
  points: T[];
  duration: number;
  toY: (point: T) => number;
  segmentIndices: AdjacentSegmentIndices | null;
}

interface HighlightedSegmentFillParams<T extends CurvePoint>
  extends HighlightedSegmentParams<T> {
  baselineY: number;
}

function timeToX(time: number, duration: number) {
  return isFinite(duration) && duration > 0 ? (time / duration) * 100 : 0;
}

/** Smooth cubic-bezier segment between two track points (horizontal tangents). */
export function getCurveSegment(x1: number, y1: number, x2: number, y2: number) {
  const dx = x2 - x1;
  return `C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2}`;
}

export function generateCurvePath<T extends CurvePoint>({
  points,
  duration,
  toY,
  emptyPathY,
}: CurvePathParams<T>) {
  if (points.length === 0 || !isFinite(duration) || duration <= 0) {
    return `M 0 ${emptyPathY} L 100 ${emptyPathY}`;
  }
  const sorted = sortPointsByTime(points);
  const x0 = timeToX(sorted[0].time, duration);
  const y0 = toY(sorted[0]);
  let d = `M 0 ${y0} `;
  if (x0 > 0) d += `L ${x0} ${y0} `;
  for (let i = 1; i < sorted.length; i++) {
    const x1 = timeToX(sorted[i - 1].time, duration);
    const y1 = toY(sorted[i - 1]);
    const x2 = timeToX(sorted[i].time, duration);
    const y2 = toY(sorted[i]);
    d += `${getCurveSegment(x1, y1, x2, y2)} `;
  }
  const xLast = timeToX(sorted[sorted.length - 1].time, duration);
  const yLast = toY(sorted[sorted.length - 1]);
  if (xLast < 100) d += `L 100 ${yLast} `;
  return d;
}

export function generateCurveFillPath<T extends CurvePoint>({
  points,
  duration,
  toY,
  baselineY,
}: CurveFillParams<T>) {
  if (points.length === 0 || !isFinite(duration) || duration <= 0) return "";
  const sorted = sortPointsByTime(points);
  const x0 = timeToX(sorted[0].time, duration);
  const y0 = toY(sorted[0]);
  let d = `M 0 ${baselineY} L ${x0} ${baselineY} L ${x0} ${y0} `;
  for (let i = 1; i < sorted.length; i++) {
    const x1 = timeToX(sorted[i - 1].time, duration);
    const y1 = toY(sorted[i - 1]);
    const x2 = timeToX(sorted[i].time, duration);
    const y2 = toY(sorted[i]);
    d += `${getCurveSegment(x1, y1, x2, y2)} `;
  }
  const xLast = timeToX(sorted[sorted.length - 1].time, duration);
  d += `L ${xLast} ${baselineY} L 100 ${baselineY} Z`;
  return d;
}

export function getHighlightedCurveSegmentPath<T extends CurvePoint>({
  points,
  duration,
  toY,
  segmentIndices,
}: HighlightedSegmentParams<T>) {
  if (!segmentIndices) return "";
  const sorted = sortPointsByTime(points);
  const [leftIdx, rightIdx] = segmentIndices;
  const left = sorted[leftIdx];
  const right = sorted[rightIdx];
  if (!left || !right || right.time <= left.time) return "";
  const x1 = timeToX(left.time, duration);
  const y1 = toY(left);
  const x2 = timeToX(right.time, duration);
  const y2 = toY(right);
  return `M ${x1} ${y1} ${getCurveSegment(x1, y1, x2, y2)}`;
}

export function getHighlightedCurveSegmentFillPath<T extends CurvePoint>({
  points,
  duration,
  toY,
  baselineY,
  segmentIndices,
}: HighlightedSegmentFillParams<T>) {
  if (!segmentIndices) return "";
  const sorted = sortPointsByTime(points);
  const [leftIdx, rightIdx] = segmentIndices;
  const left = sorted[leftIdx];
  const right = sorted[rightIdx];
  if (!left || !right || right.time <= left.time) return "";
  const x1 = timeToX(left.time, duration);
  const y1 = toY(left);
  const x2 = timeToX(right.time, duration);
  const y2 = toY(right);
  return `M ${x1} ${baselineY} L ${x1} ${y1} ${getCurveSegment(x1, y1, x2, y2)} L ${x2} ${baselineY} Z`;
}
