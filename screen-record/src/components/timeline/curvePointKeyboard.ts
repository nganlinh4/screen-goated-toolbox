import { getCosineInterpolatedValueAtTime } from "./adjustableLineUtils";
import { timelinePointKeyProps } from "./timelineKeyboard";

/** How a lane's point value is stepped and announced from the keyboard. */
export interface CurvePointValueScale {
  min: number;
  max: number;
  step: number;
  coarseStep: number;
  format: (value: number) => string;
}

/** Keeps a keyboard-moved point strictly between its neighbours so point order never changes. */
const NEIGHBOUR_GAP_SEC = 0.01;

interface CurvePointKeyArgs<TPoint extends { time: number }> {
  idx: number;
  label: string;
  scale: CurvePointValueScale;
  points: TPoint[];
  getValue: (point: TPoint) => number;
  createPoint: (time: number, value: number) => TPoint;
  clampNewValue?: (value: number) => number;
  /** Writes a finished keyboard edit (one undo step each). */
  commit: (points: TPoint[]) => void;
}

/**
 * Keyboard actions for one curve control point: Left/Right move it in time
 * between its neighbours (endpoints stay pinned), Up/Down change its value,
 * Delete removes an inner point and Enter adds a point halfway to the next.
 */
export function curvePointKeyProps<TPoint extends { time: number }>({
  idx,
  label,
  scale,
  points,
  getValue,
  createPoint,
  clampNewValue,
  commit,
}: CurvePointKeyArgs<TPoint>) {
  const point = points[idx];
  const value = getValue(point);
  const isEndpoint = idx === 0 || idx === points.length - 1;
  const replaceAt = (next: TPoint) => commit(points.map((item, i) => (i === idx ? next : item)));

  return timelinePointKeyProps(
    label,
    { now: value, min: scale.min, max: scale.max, text: scale.format(value) },
    {
      onNudgeTime: (deltaSec) => {
        if (isEndpoint) return;
        const earliest = points[idx - 1].time + NEIGHBOUR_GAP_SEC;
        const latest = points[idx + 1].time - NEIGHBOUR_GAP_SEC;
        if (latest < earliest) return;
        const time = Math.min(latest, Math.max(earliest, point.time + deltaSec));
        replaceAt(createPoint(time, value));
      },
      onNudgeValue: (direction, coarse) => {
        const stepped = value + direction * (coarse ? scale.coarseStep : scale.step);
        replaceAt(createPoint(point.time, Math.min(scale.max, Math.max(scale.min, stepped))));
      },
      onDelete: isEndpoint ? undefined : () => commit(points.filter((_, i) => i !== idx)),
      onAdd: idx < points.length - 1
        ? () => {
            const time = (point.time + points[idx + 1].time) / 2;
            const expected = getCosineInterpolatedValueAtTime({ points, time, getValue });
            const added = createPoint(time, clampNewValue ? clampNewValue(expected) : expected);
            commit([...points.slice(0, idx + 1), added, ...points.slice(idx + 1)]);
          }
        : undefined,
    },
  );
}

const percent = (value: number) => `${Math.round(value * 100)}%`;

export const SPEED_POINT_SCALE: CurvePointValueScale = {
  min: 0.1, max: 16, step: 0.1, coarseStep: 1, format: (value) => `${value.toFixed(2)}x`,
};
export const VOLUME_POINT_SCALE: CurvePointValueScale = {
  min: 0, max: 1, step: 0.05, coarseStep: 0.25, format: percent,
};
export const ZOOM_INFLUENCE_POINT_SCALE: CurvePointValueScale = {
  min: 0, max: 1, step: 0.05, coarseStep: 0.25, format: percent,
};
