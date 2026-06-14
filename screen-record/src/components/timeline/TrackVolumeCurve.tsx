import React, { useEffect, useRef, useState } from "react";
import type { AudioGainPoint } from "@/types/video";
import {
  type AdjacentSegmentIndices,
  type AdjustableLineDragVisualMode,
  buildSegmentDragPlan,
  getAxisLockMode,
  getAdjustableLineDragVisualMode,
  getAdjacentSegmentIndicesAtTime,
  getCosineInterpolatedValueAtTime,
  setAdjustableLineDragVisualMode,
  sortPointsByTime,
  subscribeToAdjustableLineDragVisualMode,
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
  const draggingIdxRef = useRef<number | null>(null);
  const pointsRef = useRef(effective);
  pointsRef.current = effective;
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);
  const [activeDragIdx, setActiveDragIdx] = useState<number | null>(null);
  const [dragBadge, setDragBadge] = useState<{ x: number; y: number; volume: number } | null>(
    null,
  );
  const [isCtrlPressed, setIsCtrlPressed] = useState(false);
  const [hoveredSegmentIndices, setHoveredSegmentIndices] =
    useState<AdjacentSegmentIndices | null>(null);
  const [activeSegmentIndices, setActiveSegmentIndices] =
    useState<AdjacentSegmentIndices | null>(null);
  const [globalDragVisualMode, setGlobalDragVisualMode] =
    useState<AdjustableLineDragVisualMode | null>(() => getAdjustableLineDragVisualMode());
  const dragVisualModeRef = useRef<AdjustableLineDragVisualMode | null>(null);
  const pointAxisLockRef = useRef<"horizontal" | "vertical" | null>(null);

  const applyDragVisualMode = (mode: AdjustableLineDragVisualMode | null) => {
    if (dragVisualModeRef.current === mode) return;
    dragVisualModeRef.current = mode;
    setAdjustableLineDragVisualMode(mode);
  };

  // Subscribe to the global "is some adjustable line dragging" state so this
  // overlay's hover affordances can stand down while another track is editing.
  useEffect(() => subscribeToAdjustableLineDragVisualMode(setGlobalDragVisualMode), []);
  useEffect(() => {
    if (globalDragVisualMode === null) return;
    setHoveredIdx(null);
    setHoveredSegmentIndices(null);
  }, [globalDragVisualMode]);

  // Track Ctrl key globally so the hover preview ("which segment will move")
  // matches what DeviceAudioTrack does.
  useEffect(() => {
    const sync = (e: KeyboardEvent) => setIsCtrlPressed(e.ctrlKey);
    const clear = () => setIsCtrlPressed(false);
    window.addEventListener("keydown", sync);
    window.addEventListener("keyup", sync);
    window.addEventListener("blur", clear);
    return () => {
      window.removeEventListener("keydown", sync);
      window.removeEventListener("keyup", sync);
      window.removeEventListener("blur", clear);
      setAdjustableLineDragVisualMode(null);
    };
  }, []);

  // Delete / Backspace removes the currently-hovered keyframe (never the two
  // endpoints, to preserve the always-defined-at-edges invariant).
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Delete" && e.key !== "Backspace") return;
      if (hoveredIdx === null) return;
      if (hoveredIdx === 0 || hoveredIdx === pointsRef.current.length - 1) return;
      const next = pointsRef.current.filter((_, i) => i !== hoveredIdx);
      pointsRef.current = next;
      onChange(next);
      onCommit?.();
      setHoveredIdx(null);
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [hoveredIdx, onChange, onCommit]);

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

  const startDraggingPoint = (
    activeIdx: number,
    startClientX: number,
    startClientY: number,
    rect: DOMRect,
    initialPoints: AudioGainPoint[],
  ) => {
    draggingIdxRef.current = activeIdx;
    pointsRef.current = initialPoints;
    const start = initialPoints[activeIdx];
    if (!start) return;
    const startTime = start.time;
    const startVolume = start.volume;
    const startVolumeY = volumeToY(startVolume, TRACK_GEOMETRY);
    const valueRangePx = Math.max(1, TRACK_RANGE_PX);
    setActiveSegmentIndices(null);
    setActiveDragIdx(activeIdx);
    pointAxisLockRef.current = null;
    applyDragVisualMode("free");

    const onMove = (e: MouseEvent) => {
      if (draggingIdxRef.current === null) return;
      const mx = e.clientX - rect.left;
      const dy = e.clientY - startClientY;
      const lockMode = e.shiftKey
        ? pointAxisLockRef.current
          ?? (() => {
              const next = getAxisLockMode(
                e.clientX - startClientX,
                e.clientY - startClientY,
              );
              if (next === "horizontal" || next === "vertical") {
                pointAxisLockRef.current = next;
              }
              return next;
            })()
        : null;

      let t = (mx / rect.width) * safeDuration;
      t = Math.max(0, Math.min(safeDuration, t));
      let newY = startVolumeY + dy / valueRangePx;
      newY = Math.max(0, Math.min(1, newY));
      let volume = yToVolume(newY, TRACK_GEOMETRY);

      if (lockMode === "horizontal") volume = startVolume;
      if (lockMode === "vertical") t = startTime;

      applyDragVisualMode(
        lockMode === null
          ? "free"
          : lockMode === "armed"
            ? "armed"
            : lockMode,
      );

      if (!e.shiftKey) pointAxisLockRef.current = null;

      const next = [...pointsRef.current];
      if (next[draggingIdxRef.current]) {
        if (draggingIdxRef.current === 0) t = 0;
        if (draggingIdxRef.current === next.length - 1 && next.length > 1) {
          t = safeDuration;
        }
        next[draggingIdxRef.current] = { time: t, volume };
        pointsRef.current = next;
        onChange(next);
        setDragBadge({ x: e.clientX, y: e.clientY - 40, volume });
      }
    };

    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      draggingIdxRef.current = null;
      setActiveDragIdx(null);
      pointAxisLockRef.current = null;
      applyDragVisualMode(null);
      setDragBadge(null);
      pointsRef.current = sortPointsByTime(pointsRef.current);
      onChange(pointsRef.current);
      commitBatch?.();
      onCommit?.();
    };

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  const startDraggingSegment = (
    activeIndices: number[],
    fixedTimes: number[],
    startClientY: number,
    startVolume: number,
    initialPoints: AudioGainPoint[],
  ) => {
    pointsRef.current = initialPoints;
    const valueRangePx = Math.max(1, TRACK_RANGE_PX);
    const startVolumeY = volumeToY(startVolume, TRACK_GEOMETRY);
    setActiveSegmentIndices([
      activeIndices[0],
      activeIndices[activeIndices.length - 1],
    ]);
    applyDragVisualMode("vertical");

    const onMove = (e: MouseEvent) => {
      const dy = e.clientY - startClientY;
      let newY = startVolumeY + dy / valueRangePx;
      newY = Math.max(0, Math.min(1, newY));
      const volume = yToVolume(newY, TRACK_GEOMETRY);

      const next = [...pointsRef.current];
      activeIndices.forEach((index, i) => {
        const point = next[index];
        if (!point) return;
        next[index] = { time: fixedTimes[i] ?? point.time, volume };
      });
      pointsRef.current = next;
      onChange(next);
      setDragBadge({ x: e.clientX, y: e.clientY - 40, volume });
    };

    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      setActiveSegmentIndices(null);
      applyDragVisualMode(null);
      setDragBadge(null);
      pointsRef.current = sortPointsByTime(pointsRef.current);
      onChange(pointsRef.current);
      commitBatch?.();
      onCommit?.();
    };

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

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
      pointsRef.current = plan.points;
      onChange(plan.points);
      startDraggingSegment(
        plan.activeIndices,
        plan.activeIndices.map((index) => plan.points[index]?.time ?? time),
        e.clientY,
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
    pointsRef.current = next;
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

  const handlePointPointerDown = (
    e: React.PointerEvent<HTMLDivElement>,
    idx: number,
  ) => {
    if (e.button !== 0) return;
    e.stopPropagation();
    const rect = e.currentTarget.parentElement?.getBoundingClientRect();
    if (!rect) return;
    beginBatch?.();
    startDraggingPoint(idx, e.clientX, e.clientY, rect, pointsRef.current);
  };

  const highlightedSegmentIndices =
    activeSegmentIndices
    ?? (globalDragVisualMode === null && isCtrlPressed ? hoveredSegmentIndices : null);
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
          onPointerDown={(e) => handlePointPointerDown(e, i)}
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
          {Math.round(dragBadge.volume * 100)}%
        </div>
      )}
    </div>
  );
};
