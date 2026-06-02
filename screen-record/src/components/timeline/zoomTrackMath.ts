import {
  type AdjacentSegmentIndices,
  sortPointsByTime,
} from './adjustableLineUtils';

export const ZOOM_TRACK_TOP_PX = 4;
export const ZOOM_TRACK_RANGE_PX = 32;
export const ZOOM_TRACK_VIEWBOX_HEIGHT = 40;

export function valueToTrackY(value: number) {
  return ZOOM_TRACK_TOP_PX + (1 - value) * ZOOM_TRACK_RANGE_PX;
}

export function valueToTrackYPercent(value: number) {
  return `${(valueToTrackY(value) / ZOOM_TRACK_VIEWBOX_HEIGHT) * 100}%`;
}

const safeNum = (n: number, fallback = 0) => isFinite(n) ? n : fallback;

export function getHighlightedSegmentPath({
  points,
  duration,
  segmentIndices,
}: {
  points: { time: number; value: number }[];
  duration: number;
  segmentIndices: AdjacentSegmentIndices | null;
}) {
  if (!segmentIndices) return '';

  const sorted = sortPointsByTime(points);
  const [leftIdx, rightIdx] = segmentIndices;
  const left = sorted[leftIdx];
  const right = sorted[rightIdx];
  if (!left || !right || right.time <= left.time || !isFinite(duration) || duration <= 0) return '';

  const toX = (time: number) => safeNum((time / duration) * 100);
  const x1 = toX(left.time);
  const y1 = valueToTrackY(left.value);
  const x2 = toX(right.time);
  const y2 = valueToTrackY(right.value);
  const dx = x2 - x1;
  return `M ${x1} ${y1} C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2}`;
}

export function getHighlightedSegmentFillPath({
  points,
  duration,
  segmentIndices,
}: {
  points: { time: number; value: number }[];
  duration: number;
  segmentIndices: AdjacentSegmentIndices | null;
}) {
  if (!segmentIndices) return '';

  const sorted = sortPointsByTime(points);
  const [leftIdx, rightIdx] = segmentIndices;
  const left = sorted[leftIdx];
  const right = sorted[rightIdx];
  if (!left || !right || right.time <= left.time || !isFinite(duration) || duration <= 0) return '';

  const toX = (time: number) => safeNum((time / duration) * 100);
  const x1 = toX(left.time);
  const y1 = valueToTrackY(left.value);
  const x2 = toX(right.time);
  const y2 = valueToTrackY(right.value);
  const dx = x2 - x1;
  return `M ${x1} 40 L ${x1} ${y1} C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} L ${x2} 40 Z`;
}

export function generateZoomPath(
  points: { time: number; value: number }[],
  duration: number,
) {
  if (points.length === 0 || !isFinite(duration) || duration <= 0) return 'M 0 20 L 100 20';
  const sorted = [...points].sort((a, b) => a.time - b.time);
  const toX = (time: number) => safeNum((time / duration) * 100);
  const x0 = toX(sorted[0].time);
  const y0 = valueToTrackY(sorted[0].value);
  let d = `M 0 ${y0} `;
  if (x0 > 0) d += `L ${x0} ${y0} `;
  for (let i = 1; i < sorted.length; i++) {
    const p1 = sorted[i - 1];
    const p2 = sorted[i];
    const x1 = toX(p1.time);
    const y1 = valueToTrackY(p1.value);
    const x2 = toX(p2.time);
    const y2 = valueToTrackY(p2.value);
    const dx = x2 - x1;
    d += `C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} `;
  }
  const xLast = toX(sorted[sorted.length - 1].time);
  const yLast = valueToTrackY(sorted[sorted.length - 1].value);
  if (xLast < 100) d += `L 100 ${yLast} `;
  return d;
}

export function generateZoomFillPath(
  points: { time: number; value: number }[],
  duration: number,
) {
  if (points.length === 0 || !isFinite(duration) || duration <= 0) return '';
  const sorted = [...points].sort((a, b) => a.time - b.time);
  const toX = (time: number) => safeNum((time / duration) * 100);
  const x0 = toX(sorted[0].time);
  const y0 = valueToTrackY(sorted[0].value);
  let d = `M 0 40 L ${x0} 40 L ${x0} ${y0} `;
  for (let i = 1; i < sorted.length; i++) {
    const p1 = sorted[i - 1];
    const p2 = sorted[i];
    const x1 = toX(p1.time);
    const y1 = valueToTrackY(p1.value);
    const x2 = toX(p2.time);
    const y2 = valueToTrackY(p2.value);
    const dx = x2 - x1;
    d += `C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} `;
  }
  const xLast = toX(sorted[sorted.length - 1].time);
  d += `L ${xLast} 40 L 100 40 Z`;
  return d;
}
