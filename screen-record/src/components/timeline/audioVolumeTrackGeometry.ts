import {
  type AdjacentSegmentIndices,
  sortPointsByTime,
} from "./adjustableLineUtils";

export interface VolumeTrackPoint {
  time: number;
  volume: number;
}

export interface VolumeTrackGeometry {
  topPx: number;
  bottomPx: number;
  viewBoxHeight: number;
  emptyPathY: number;
  clampVolume: (volume: number) => number;
}

interface VolumeTrackPathParams<T extends VolumeTrackPoint> {
  points: T[];
  duration: number;
  geometry: VolumeTrackGeometry;
}

interface HighlightedSegmentPathParams<T extends VolumeTrackPoint>
  extends VolumeTrackPathParams<T> {
  segmentIndices: AdjacentSegmentIndices | null;
}

function timeToX(time: number, duration: number) {
  return isFinite(duration) && duration > 0 ? (time / duration) * 100 : 0;
}

function getCurveSegment(
  x1: number,
  y1: number,
  x2: number,
  y2: number,
) {
  const dx = x2 - x1;
  return `C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2}`;
}

export function volumeToY(
  volume: number,
  geometry: VolumeTrackGeometry,
) {
  return 1 - geometry.clampVolume(volume);
}

export function yToVolume(y: number, geometry: VolumeTrackGeometry) {
  return geometry.clampVolume(1 - y);
}

export function volumeToTrackY(
  volume: number,
  geometry: VolumeTrackGeometry,
) {
  return geometry.topPx + volumeToY(volume, geometry) * (geometry.bottomPx - geometry.topPx);
}

export function volumeToTrackYPercent(
  volume: number,
  geometry: VolumeTrackGeometry,
) {
  return `${(volumeToTrackY(volume, geometry) / geometry.viewBoxHeight) * 100}%`;
}

export function generateVolumeTrackPath<T extends VolumeTrackPoint>({
  points,
  duration,
  geometry,
}: VolumeTrackPathParams<T>) {
  if (points.length === 0) {
    return `M 0 ${geometry.emptyPathY} L 100 ${geometry.emptyPathY}`;
  }

  const sorted = sortPointsByTime(points);
  const x0 = timeToX(sorted[0].time, duration);
  const y0 = volumeToTrackY(sorted[0].volume, geometry);
  let d = `M 0 ${y0} `;
  if (x0 > 0) d += `L ${x0} ${y0} `;

  for (let i = 1; i < sorted.length; i++) {
    const left = sorted[i - 1];
    const right = sorted[i];
    const x1 = timeToX(left.time, duration);
    const y1 = volumeToTrackY(left.volume, geometry);
    const x2 = timeToX(right.time, duration);
    const y2 = volumeToTrackY(right.volume, geometry);
    d += `${getCurveSegment(x1, y1, x2, y2)} `;
  }

  const xLast = timeToX(sorted[sorted.length - 1].time, duration);
  const yLast = volumeToTrackY(sorted[sorted.length - 1].volume, geometry);
  if (xLast < 100) d += `L 100 ${yLast} `;
  return d;
}

export function generateVolumeTrackFillPath<T extends VolumeTrackPoint>({
  points,
  duration,
  geometry,
}: VolumeTrackPathParams<T>) {
  if (points.length === 0) return "";

  const sorted = sortPointsByTime(points);
  const x0 = timeToX(sorted[0].time, duration);
  const y0 = volumeToTrackY(sorted[0].volume, geometry);
  let d = `M 0 40 L ${x0} 40 L ${x0} ${y0} `;

  for (let i = 1; i < sorted.length; i++) {
    const left = sorted[i - 1];
    const right = sorted[i];
    const x1 = timeToX(left.time, duration);
    const y1 = volumeToTrackY(left.volume, geometry);
    const x2 = timeToX(right.time, duration);
    const y2 = volumeToTrackY(right.volume, geometry);
    d += `${getCurveSegment(x1, y1, x2, y2)} `;
  }

  const xLast = timeToX(sorted[sorted.length - 1].time, duration);
  d += `L ${xLast} 40 L 100 40 Z`;
  return d;
}

export function getHighlightedVolumeSegmentPath<T extends VolumeTrackPoint>({
  points,
  duration,
  geometry,
  segmentIndices,
}: HighlightedSegmentPathParams<T>) {
  if (!segmentIndices) return "";

  const sorted = sortPointsByTime(points);
  const [leftIdx, rightIdx] = segmentIndices;
  const left = sorted[leftIdx];
  const right = sorted[rightIdx];
  if (!left || !right || right.time <= left.time) return "";

  const x1 = timeToX(left.time, duration);
  const y1 = volumeToTrackY(left.volume, geometry);
  const x2 = timeToX(right.time, duration);
  const y2 = volumeToTrackY(right.volume, geometry);
  return `M ${x1} ${y1} ${getCurveSegment(x1, y1, x2, y2)}`;
}

export function getHighlightedVolumeSegmentFillPath<T extends VolumeTrackPoint>({
  points,
  duration,
  geometry,
  segmentIndices,
}: HighlightedSegmentPathParams<T>) {
  if (!segmentIndices) return "";

  const sorted = sortPointsByTime(points);
  const [leftIdx, rightIdx] = segmentIndices;
  const left = sorted[leftIdx];
  const right = sorted[rightIdx];
  if (!left || !right || right.time <= left.time) return "";

  const x1 = timeToX(left.time, duration);
  const y1 = volumeToTrackY(left.volume, geometry);
  const x2 = timeToX(right.time, duration);
  const y2 = volumeToTrackY(right.volume, geometry);
  return `M ${x1} 40 L ${x1} ${y1} ${getCurveSegment(x1, y1, x2, y2)} L ${x2} 40 Z`;
}
