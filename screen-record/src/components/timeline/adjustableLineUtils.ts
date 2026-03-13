interface TimePoint {
  time: number;
}

const INNER_SEGMENT_HANDLE_OFFSET_PX = 6;
const MIN_SEGMENT_PLATEAU_WIDTH_PX = 4;
const AXIS_LOCK_THRESHOLD_PX = 4;
const ADJUSTABLE_LINE_DRAG_BODY_CLASSES = [
  'dragging-adjustable-line',
  'dragging-adjustable-line-free',
  'dragging-adjustable-line-armed',
  'dragging-adjustable-line-axis-x',
  'dragging-adjustable-line-axis-y',
];
const adjustableLineDragListeners = new Set<
  (mode: AdjustableLineDragVisualMode | null) => void
>();
let currentAdjustableLineDragMode: AdjustableLineDragVisualMode | null = null;

export interface SegmentDragPlan<T extends TimePoint> {
  points: T[];
  activeIndices: number[];
  startValue: number;
}

export type AdjustableLineDragVisualMode =
  | 'free'
  | 'armed'
  | 'horizontal'
  | 'vertical';

export type AdjacentSegmentIndices = [number, number];

function clamp(value: number, min: number, max: number) {
  return Math.max(min, Math.min(max, value));
}

export function getAxisLockMode(
  dx: number,
  dy: number,
): 'armed' | 'horizontal' | 'vertical' {
  if (
    Math.max(Math.abs(dx), Math.abs(dy)) <
    AXIS_LOCK_THRESHOLD_PX
  ) {
    return 'armed';
  }

  return Math.abs(dx) >= Math.abs(dy) ? 'horizontal' : 'vertical';
}

export function setAdjustableLineDragVisualMode(
  mode: AdjustableLineDragVisualMode | null,
) {
  if (currentAdjustableLineDragMode === mode) return;
  currentAdjustableLineDragMode = mode;

  adjustableLineDragListeners.forEach((listener) => {
    listener(mode);
  });

  if (typeof document === 'undefined') return;

  const body = document.body;
  ADJUSTABLE_LINE_DRAG_BODY_CLASSES.forEach((className) => {
    body.classList.remove(className);
  });

  if (mode === null) return;

  body.classList.add('dragging-adjustable-line');
  if (mode === 'free') body.classList.add('dragging-adjustable-line-free');
  if (mode === 'armed') body.classList.add('dragging-adjustable-line-armed');
  if (mode === 'horizontal') body.classList.add('dragging-adjustable-line-axis-x');
  if (mode === 'vertical') body.classList.add('dragging-adjustable-line-axis-y');
}

export function getAdjustableLineDragVisualMode() {
  return currentAdjustableLineDragMode;
}

export function subscribeToAdjustableLineDragVisualMode(
  listener: (mode: AdjustableLineDragVisualMode | null) => void,
) {
  adjustableLineDragListeners.add(listener);
  return () => {
    adjustableLineDragListeners.delete(listener);
  };
}

export function sortPointsByTime<T extends TimePoint>(points: T[]): T[] {
  return [...points].sort((a, b) => a.time - b.time);
}

export function getCosineInterpolatedValueAtTime<T extends TimePoint>({
  points,
  time,
  getValue,
}: {
  points: T[];
  time: number;
  getValue: (point: T) => number;
}): number {
  if (points.length === 0) return 0;

  const sorted = sortPointsByTime(points);
  if (sorted.length === 1) return getValue(sorted[0]);

  const idx = sorted.findIndex((point) => point.time >= time);
  if (idx === -1) return getValue(sorted[sorted.length - 1]);
  if (idx === 0) return getValue(sorted[0]);

  const left = sorted[idx - 1];
  const right = sorted[idx];
  const span = Math.max(0.0001, right.time - left.time);
  const ratio = clamp((time - left.time) / span, 0, 1);
  const cosT = (1 - Math.cos(ratio * Math.PI)) / 2;
  return getValue(left) + (getValue(right) - getValue(left)) * cosT;
}

export function getAdjacentSegmentIndicesAtTime<T extends TimePoint>({
  points,
  time,
  duration,
}: {
  points: T[];
  time: number;
  duration: number;
}): AdjacentSegmentIndices | null {
  if (points.length < 2 || duration <= 0) return null;

  const sorted = sortPointsByTime(points);
  const clampedTime = clamp(time, 0, duration);
  let rightIdx = sorted.findIndex((point) => point.time >= clampedTime);
  if (rightIdx === -1) rightIdx = sorted.length - 1;

  let leftIdx = Math.max(0, rightIdx - 1);
  if (rightIdx === 0) {
    rightIdx = Math.min(sorted.length - 1, 1);
    leftIdx = 0;
  }

  if (leftIdx === rightIdx) return null;
  return [leftIdx, rightIdx];
}

export function buildSegmentDragPlan<T extends TimePoint>({
  points,
  time,
  duration,
  trackWidth,
  getValue,
  createPoint,
}: {
  points: T[];
  time: number;
  duration: number;
  trackWidth: number;
  getValue: (point: T) => number;
  createPoint: (time: number, value: number) => T;
}): SegmentDragPlan<T> | null {
  if (points.length === 0 || duration <= 0 || trackWidth <= 0) return null;

  const sorted = sortPointsByTime(points);
  if (sorted.length === 1) {
    return {
      points: sorted,
      activeIndices: [0],
      startValue: getValue(sorted[0]),
    };
  }

  const clampedTime = clamp(time, 0, duration);
  let rightIdx = sorted.findIndex((point) => point.time >= clampedTime);
  if (rightIdx === -1) rightIdx = sorted.length - 1;

  let leftIdx = Math.max(0, rightIdx - 1);
  if (rightIdx === 0) {
    rightIdx = Math.min(sorted.length - 1, 1);
    leftIdx = 0;
  }

  if (leftIdx === rightIdx) {
    return {
      points: sorted,
      activeIndices: [leftIdx],
      startValue: getValue(sorted[leftIdx]),
    };
  }

  const left = sorted[leftIdx];
  const right = sorted[rightIdx];
  const segmentWidthPx = ((right.time - left.time) / duration) * trackWidth;
  const pinLeftBoundary = leftIdx === 0;
  const pinRightBoundary = rightIdx === sorted.length - 1;
  const leftInsetPx = pinLeftBoundary ? 0 : INNER_SEGMENT_HANDLE_OFFSET_PX;
  const rightInsetPx = pinRightBoundary ? 0 : INNER_SEGMENT_HANDLE_OFFSET_PX;
  const canCreateInnerHandles =
    segmentWidthPx >=
    leftInsetPx + rightInsetPx + MIN_SEGMENT_PLATEAU_WIDTH_PX;
  const startValue = getCosineInterpolatedValueAtTime({
    points: sorted,
    time: clampedTime,
    getValue,
  });

  if (!canCreateInnerHandles) {
    return {
      points: sorted,
      activeIndices: Array.from(new Set([leftIdx, rightIdx])),
      startValue,
    };
  }

  const nextPoints = [...sorted];
  const activeIndices: number[] = [];
  let adjustedRightIdx = rightIdx;

  if (pinLeftBoundary) {
    activeIndices.push(leftIdx);
  } else {
    const leftTime =
      left.time + (INNER_SEGMENT_HANDLE_OFFSET_PX / trackWidth) * duration;
    const leftValue = getCosineInterpolatedValueAtTime({
      points: sorted,
      time: leftTime,
      getValue,
    });
    nextPoints.splice(leftIdx + 1, 0, createPoint(leftTime, leftValue));
    activeIndices.push(leftIdx + 1);
    adjustedRightIdx += 1;
  }

  if (pinRightBoundary) {
    activeIndices.push(adjustedRightIdx);
  } else {
    const rightTime =
      right.time - (INNER_SEGMENT_HANDLE_OFFSET_PX / trackWidth) * duration;
    const rightValue = getCosineInterpolatedValueAtTime({
      points: sorted,
      time: rightTime,
      getValue,
    });
    nextPoints.splice(adjustedRightIdx, 0, createPoint(rightTime, rightValue));
    activeIndices.push(adjustedRightIdx);
  }

  return {
    points: nextPoints,
    activeIndices: Array.from(new Set(activeIndices)),
    startValue,
  };
}
