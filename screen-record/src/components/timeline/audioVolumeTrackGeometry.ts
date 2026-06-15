import { type AdjacentSegmentIndices } from "./adjustableLineUtils";
import {
  generateCurveFillPath,
  generateCurvePath,
  getHighlightedCurveSegmentFillPath,
  getHighlightedCurveSegmentPath,
} from "./curvePath";

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
  return generateCurvePath({
    points,
    duration,
    toY: (p) => volumeToTrackY(p.volume, geometry),
    emptyPathY: geometry.emptyPathY,
  });
}

export function generateVolumeTrackFillPath<T extends VolumeTrackPoint>({
  points,
  duration,
  geometry,
}: VolumeTrackPathParams<T>) {
  return generateCurveFillPath({
    points,
    duration,
    toY: (p) => volumeToTrackY(p.volume, geometry),
    baselineY: geometry.viewBoxHeight,
  });
}

export function getHighlightedVolumeSegmentPath<T extends VolumeTrackPoint>({
  points,
  duration,
  geometry,
  segmentIndices,
}: HighlightedSegmentPathParams<T>) {
  return getHighlightedCurveSegmentPath({
    points,
    duration,
    toY: (p) => volumeToTrackY(p.volume, geometry),
    segmentIndices,
  });
}

export function getHighlightedVolumeSegmentFillPath<T extends VolumeTrackPoint>({
  points,
  duration,
  geometry,
  segmentIndices,
}: HighlightedSegmentPathParams<T>) {
  return getHighlightedCurveSegmentFillPath({
    points,
    duration,
    toY: (p) => volumeToTrackY(p.volume, geometry),
    baselineY: geometry.viewBoxHeight,
    segmentIndices,
  });
}
