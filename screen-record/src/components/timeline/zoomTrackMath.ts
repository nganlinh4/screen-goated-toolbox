import { type AdjacentSegmentIndices } from './adjustableLineUtils';
import {
  generateCurveFillPath,
  generateCurvePath,
  getHighlightedCurveSegmentFillPath,
  getHighlightedCurveSegmentPath,
} from './curvePath';

export const ZOOM_TRACK_TOP_PX = 4;
export const ZOOM_TRACK_RANGE_PX = 32;
export const ZOOM_TRACK_VIEWBOX_HEIGHT = 40;

export function valueToTrackY(value: number) {
  return ZOOM_TRACK_TOP_PX + (1 - value) * ZOOM_TRACK_RANGE_PX;
}

export function valueToTrackYPercent(value: number) {
  return `${(valueToTrackY(value) / ZOOM_TRACK_VIEWBOX_HEIGHT) * 100}%`;
}

type ZoomPoint = { time: number; value: number };

const zoomToY = (p: ZoomPoint) => valueToTrackY(p.value);

export function getHighlightedSegmentPath({
  points,
  duration,
  segmentIndices,
}: {
  points: ZoomPoint[];
  duration: number;
  segmentIndices: AdjacentSegmentIndices | null;
}) {
  return getHighlightedCurveSegmentPath({ points, duration, toY: zoomToY, segmentIndices });
}

export function getHighlightedSegmentFillPath({
  points,
  duration,
  segmentIndices,
}: {
  points: ZoomPoint[];
  duration: number;
  segmentIndices: AdjacentSegmentIndices | null;
}) {
  return getHighlightedCurveSegmentFillPath({
    points,
    duration,
    toY: zoomToY,
    baselineY: ZOOM_TRACK_VIEWBOX_HEIGHT,
    segmentIndices,
  });
}

export function generateZoomPath(points: ZoomPoint[], duration: number) {
  return generateCurvePath({ points, duration, toY: zoomToY, emptyPathY: 20 });
}

export function generateZoomFillPath(points: ZoomPoint[], duration: number) {
  return generateCurveFillPath({ points, duration, toY: zoomToY, baselineY: ZOOM_TRACK_VIEWBOX_HEIGHT });
}
