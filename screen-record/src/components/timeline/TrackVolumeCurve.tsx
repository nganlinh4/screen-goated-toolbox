import React from "react";
import type { AudioGainPoint } from "@/types/video";
import {
  buildSegmentDragPlan,
  getAdjacentSegmentIndicesAtTime,
  getCosineInterpolatedValueAtTime,
  sortPointsByTime,
} from "./adjustableLineUtils";
import {
  generateVolumeTrackPath,
  getHighlightedVolumeSegmentPath,
  type VolumeTrackGeometry,
  volumeToTrackY,
  volumeToTrackYPercent,
  volumeToY,
  yToVolume,
} from "./audioVolumeTrackGeometry";
import { useAdjustableLineTrack } from "./useAdjustableLineTrack";
import { useSettings } from "@/hooks/useSettings";

const TRACK_TOP_PX = 5;
const TRACK_BOTTOM_PX = 35;
const TRACK_RANGE_PX = TRACK_BOTTOM_PX - TRACK_TOP_PX;
const TRACK_VIEWBOX_HEIGHT = 40;

function clampVolume(value: number) {
  if (!Number.isFinite(value)) return 0;
  if (value <= 0) return 0;
  if (value >= 1) return 1;
  return value;
}

const TRACK_GEOMETRY = {
  topPx: TRACK_TOP_PX,
  bottomPx: TRACK_BOTTOM_PX,
  viewBoxHeight: TRACK_VIEWBOX_HEIGHT,
  emptyPathY: TRACK_TOP_PX,
  clampVolume,
} satisfies VolumeTrackGeometry;

function buildFlatPoints(duration: number): AudioGainPoint[] {
  const safe = Math.max(duration, 0.0001);
  return [
    { time: 0, volume: 1 },
    { time: safe, volume: 1 },
  ];
}

interface TrackVolumeCurveProps {
  /** Project-relative track duration in seconds (curve x-domain). */
  duration: number;
  points: AudioGainPoint[];
  /** CSS variable name like "--primary-color" or "--secondary-color". */
  colorVar: string;
  onChange: (points: AudioGainPoint[]) => void;
  beginBatch?: () => void;
  commitBatch?: () => void;
  onCommit?: () => void;
}

/**
 * Track-global volume envelope curve. Behaviour mirrors `DeviceAudioTrack`'s
 * curve interactions point-for-point — shared util calls (`buildSegmentDragPlan`,
 * `getAxisLockMode`, `setAdjustableLineDragVisualMode`, …) are imported from
 * the same `adjustableLineUtils` module, so cursor classes / axis lock /
 * segment-range hover all behave identically across device, audio and
 * narration tracks. The only difference is no waveform background.
 */
export const TrackVolumeCurve: React.FC<TrackVolumeCurveProps> = ({
  duration,
  points: rawPoints,
  colorVar,
  onChange,
  beginBatch,
  commitBatch,
  onCommit,
}) => {
  const { t } = useSettings();
  const safeDuration = Math.max(duration, 0.0001);
  const effective = rawPoints.length > 0 ? rawPoints : buildFlatPoints(safeDuration);
  const sorted = sortPointsByTime(effective);

  // Shift-axis-lock math is active (as before), but this lane never rendered a
  // `data-lock-mode` attribute, so the controller's `axisLockMode` state is
  // simply not read here. It threads an extra `onCommit`. The Delete/Backspace
  // handler runs against `pointsRef.current` (= `effective`), matching the
  // always-defined-at-edges invariant, so `effective` is fed to the controller.
  // The pointer-down/move handlers below operate on `sorted` (the rendered
  // order), driving the controller's drag primitives directly.
  const {
    hoveredIdx,
    setHoveredIdx,
    activeDragIdx,
    dragBadge,
    globalDragVisualMode,
    highlightedSegmentIndices,
    startDraggingPoint,
    startDraggingSegment,
    handlePointPointerDown,
    setHoveredSegmentIndices,
  } = useAdjustableLineTrack<AudioGainPoint>({
    points: effective,
    duration: safeDuration,
    onUpdatePoints: onChange,
    getValue: (point) => point.volume,
    createPoint: (time, volume) => ({ time, volume }),
    resolvePointValue: ({ dy, startPoint }) => {
      const valueRangePx = Math.max(1, TRACK_RANGE_PX);
      const startVolumeY = volumeToY(startPoint.volume, TRACK_GEOMETRY);
      const newY = Math.max(0, Math.min(1, startVolumeY + dy / valueRangePx));
      return yToVolume(newY, TRACK_GEOMETRY);
    },
    resolveSegmentValue: ({ dy, startValue }) => {
      const valueRangePx = Math.max(1, TRACK_RANGE_PX);
      const startVolumeY = volumeToY(startValue, TRACK_GEOMETRY);
      const newY = Math.max(0, Math.min(1, startVolumeY + dy / valueRangePx));
      return yToVolume(newY, TRACK_GEOMETRY);
    },
    axisLockEnabled: true,
    makeBadge: (me, value) => ({ x: me.clientX, y: me.clientY - 40, value }),
    beginBatch,
    commitBatch,
    onCommit,
  });

  const toX = (time: number) => Math.max(0, Math.min(100, (time / safeDuration) * 100));

  // NOTE: the fill baseline here is `TRACK_BOTTOM_PX` (35), which differs from
  // the shared `generateVolumeTrackFillPath` baseline (`viewBoxHeight`, 40) used
  // by MicTrack/DeviceAudioTrack. The fill path is therefore kept local to
  // preserve this track's exact rendered output (WYSIWYG). The stroke path and
  // highlighted-segment path are byte-identical to the shared helpers, so those
  // are imported.
  const generateFillPath = () => {
    if (sorted.length === 0) return "";
    const x0 = toX(sorted[0].time);
    const y0 = volumeToTrackY(sorted[0].volume, TRACK_GEOMETRY);
    let d = `M 0 ${TRACK_BOTTOM_PX} L ${x0} ${TRACK_BOTTOM_PX} L ${x0} ${y0} `;
    for (let i = 1; i < sorted.length; i += 1) {
      const left = sorted[i - 1];
      const right = sorted[i];
      const x1 = toX(left.time);
      const y1 = volumeToTrackY(left.volume, TRACK_GEOMETRY);
      const x2 = toX(right.time);
      const y2 = volumeToTrackY(right.volume, TRACK_GEOMETRY);
      const dx = x2 - x1;
      d += `C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} `;
    }
    const last = sorted[sorted.length - 1];
    const xLast = toX(last.time);
    d += `L ${xLast} ${TRACK_BOTTOM_PX} L 100 ${TRACK_BOTTOM_PX} Z`;
    return d;
  };

  // The track-area pointer handlers operate on `sorted` (matching the rendered
  // control-point order) and drive the controller's drag primitives directly,
  // so this lane keeps its bespoke `e.button !== 0` guards and `sorted`-based
  // segment/insert logic verbatim.
  const handleAreaPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    const rect = e.currentTarget.getBoundingClientRect();
    if (rect.width <= 0 || safeDuration <= 0) return;
    const clickX = e.clientX - rect.left;
    const time = (clickX / rect.width) * safeDuration;
    e.stopPropagation();

    if (e.ctrlKey) {
      const plan = buildSegmentDragPlan({
        points: sorted,
        time,
        duration: safeDuration,
        trackWidth: rect.width,
        getValue: (point) => point.volume,
        createPoint: (pointTime, volume) => ({ time: pointTime, volume }),
      });
      if (!plan) return;
      beginBatch?.();
      onChange(plan.points);
      startDraggingSegment(
        plan.activeIndices,
        plan.activeIndices.map((index) => plan.points[index]?.time ?? time),
        e.clientY,
        rect,
        plan.startValue,
        plan.points,
      );
      return;
    }

    let next = [...sorted];
    beginBatch?.();
    const expectedVolume = getCosineInterpolatedValueAtTime({
      points: next,
      time,
      getValue: (point) => point.volume,
    });
    const newPoint = { time, volume: clampVolume(expectedVolume) };
    next.push(newPoint);
    next = sortPointsByTime(next);
    const newIdx = next.indexOf(newPoint);
    onChange(next);
    startDraggingPoint(newIdx, e.clientX, e.clientY, rect, next);
  };

  const handleAreaPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (globalDragVisualMode !== null) {
      setHoveredSegmentIndices(null);
      return;
    }
    if (safeDuration <= 0 || sorted.length < 2) {
      setHoveredSegmentIndices(null);
      return;
    }
    const rect = e.currentTarget.getBoundingClientRect();
    if (rect.width <= 0) {
      setHoveredSegmentIndices(null);
      return;
    }
    const time = ((e.clientX - rect.left) / rect.width) * safeDuration;
    setHoveredSegmentIndices(
      getAdjacentSegmentIndicesAtTime({ points: sorted, time, duration: safeDuration }),
    );
  };

  const onPointPointerDown = (
    e: React.PointerEvent<HTMLDivElement>,
    idx: number,
  ) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    const rect = e.currentTarget.parentElement?.getBoundingClientRect();
    if (!rect) return;
    handlePointPointerDown(rect, e.clientX, e.clientY, idx);
  };

  const highlightedSegmentPath = getHighlightedVolumeSegmentPath({
    points: sorted,
    duration: safeDuration,
    geometry: TRACK_GEOMETRY,
    segmentIndices: highlightedSegmentIndices,
  });

  return (
    <div
      className="track-volume-curve absolute inset-0 z-[3]"
      onPointerDown={handleAreaPointerDown}
      onPointerMove={handleAreaPointerMove}
      onPointerLeave={() => setHoveredSegmentIndices(null)}
    >
      <svg
        className="track-volume-curve-svg pointer-events-none absolute inset-0 h-full w-full"
        preserveAspectRatio="none"
        viewBox={`0 0 100 ${TRACK_VIEWBOX_HEIGHT}`}
      >
        <line
          x1="0"
          y1={TRACK_TOP_PX}
          x2="100"
          y2={TRACK_TOP_PX}
          stroke={`color-mix(in srgb, var(${colorVar}) 24%, transparent)`}
          vectorEffect="non-scaling-stroke"
        />
        <line
          x1="0"
          y1={TRACK_BOTTOM_PX}
          x2="100"
          y2={TRACK_BOTTOM_PX}
          stroke={`color-mix(in srgb, var(${colorVar}) 18%, transparent)`}
          vectorEffect="non-scaling-stroke"
        />
        <path
          d={generateFillPath()}
          fill={`color-mix(in srgb, var(${colorVar}) 12%, transparent)`}
        />
        <path
          d={generateVolumeTrackPath({
            points: sorted,
            duration: safeDuration,
            geometry: TRACK_GEOMETRY,
          })}
          fill="none"
          stroke={`var(${colorVar})`}
          strokeWidth="1.5"
          vectorEffect="non-scaling-stroke"
        />
        {highlightedSegmentPath && (
          <path
            className="timeline-segment-highlight-path"
            d={highlightedSegmentPath}
            fill="none"
            stroke="currentColor"
            strokeWidth="4"
            strokeDasharray="3 4"
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
            style={{ color: `var(${colorVar})` }}
          />
        )}
      </svg>
      {sorted.map((point, i) => (
        <div
          key={i}
          className={`track-volume-point timeline-control-point absolute -translate-x-1/2 -translate-y-1/2 cursor-pointer ${
            hoveredIdx === i ? "ring-2 ring-[currentColor]/40" : "hover:scale-110"
          }`}
          data-state={hoveredIdx === i || activeDragIdx === i ? "active" : "idle"}
          style={{
            left: `${toX(point.time)}%`,
            top: volumeToTrackYPercent(point.volume, TRACK_GEOMETRY),
            background: `var(${colorVar})`,
            color: `var(${colorVar})`,
          }}
          onMouseEnter={() => setHoveredIdx(i)}
          onMouseLeave={() => setHoveredIdx(null)}
          onPointerDown={(e) => onPointPointerDown(e, i)}
          title={`${point.time.toFixed(2)}s · ${(point.volume * 100).toFixed(0)}% (${t.volumePointRemoveHint})`}
        />
      ))}
      {dragBadge && (
        <div
          className="track-volume-drag-badge timeline-chip fixed z-[100] -translate-x-1/2 -translate-y-full px-3 py-1.5 text-sm font-bold text-white pointer-events-none"
          data-active="true"
          style={{
            left: dragBadge.x,
            top: dragBadge.y,
            background: `var(${colorVar})`,
            borderColor: `var(${colorVar})`,
            color: "#ffffff",
          }}
        >
          {Math.round(dragBadge.value * 100)}%
        </div>
      )}
    </div>
  );
};
