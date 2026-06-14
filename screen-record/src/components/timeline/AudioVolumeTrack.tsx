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
import { AudioWaveformLayer } from "./AudioWaveformLayer";
import {
  generateVolumeTrackFillPath,
  generateVolumeTrackPath,
  getHighlightedVolumeSegmentFillPath,
  getHighlightedVolumeSegmentPath,
  type VolumeTrackGeometry,
  volumeToTrackYPercent,
  volumeToY,
  yToVolume,
} from "./audioVolumeTrackGeometry";
import { SoftAudioSegmentBlock } from "./SoftAudioSegmentBlock";

/**
 * Shared volume-curve track used by both the mic and device audio lanes.
 *
 * The two lanes are pixel-identical apart from a handful of values
 * (className prefix, tone, color variable, geometry baseline, the volume
 * helpers and an optional waveform offset). Those are passed in via props so
 * MicTrack / DeviceAudioTrack stay thin wrappers.
 */
export interface AudioVolumeTrackProps {
  points: AudioGainPoint[];
  onUpdatePoints: (points: AudioGainPoint[]) => void;
  duration: number;
  isAvailable: boolean;
  sourcePath?: string | null;
  viewMode?: "compact" | "volume";
  geometry: VolumeTrackGeometry;
  rangePx: number;
  classNamePrefix: string;
  tone: string;
  colorVariable: string;
  /**
   * The exact Tailwind class string applied to a hovered control point's ring.
   * Passed in (rather than built from `colorVariable`) so the arbitrary-value
   * class literal stays present in the wrapper source for the JIT scanner.
   */
  hoveredRingClass: string;
  getVolumeAtTime: (
    time: number,
    points: AudioGainPoint[] | undefined | null,
  ) => number;
  offsetSec?: number;
  beginBatch: () => void;
  commitBatch: () => void;
}

export const AudioVolumeTrack: React.FC<AudioVolumeTrackProps> = ({
  points,
  onUpdatePoints,
  duration,
  isAvailable,
  sourcePath,
  viewMode = "volume",
  geometry,
  rangePx,
  classNamePrefix,
  tone,
  colorVariable,
  hoveredRingClass,
  getVolumeAtTime,
  offsetSec,
  beginBatch,
  commitBatch,
}) => {
  const draggingIdxRef = useRef<number | null>(null);
  const pointsRef = useRef(points);
  pointsRef.current = points;
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);
  const [dragBadge, setDragBadge] = useState<{ x: number; y: number; volume: number } | null>(null);
  const [isCtrlPressed, setIsCtrlPressed] = useState(false);
  const [activeDragIdx, setActiveDragIdx] = useState<number | null>(null);
  const [axisLockMode, setAxisLockMode] = useState<"armed" | "horizontal" | "vertical" | null>(null);
  const [isSegmentDragActive, setIsSegmentDragActive] = useState(false);
  const [hoveredSegmentIndices, setHoveredSegmentIndices] =
    useState<AdjacentSegmentIndices | null>(null);
  const [activeSegmentIndices, setActiveSegmentIndices] =
    useState<AdjacentSegmentIndices | null>(null);
  const [globalDragVisualMode, setGlobalDragVisualMode] =
    useState<AdjustableLineDragVisualMode | null>(() =>
      getAdjustableLineDragVisualMode(),
    );
  const dragVisualModeRef = useRef<AdjustableLineDragVisualMode | null>(null);
  const pointAxisLockRef = useRef<"horizontal" | "vertical" | null>(null);

  const applyDragVisualMode = (mode: AdjustableLineDragVisualMode | null) => {
    if (dragVisualModeRef.current === mode) return;
    dragVisualModeRef.current = mode;
    setAdjustableLineDragVisualMode(mode);
  };

  const updateAxisLockMode = (
    mode: "armed" | "horizontal" | "vertical" | null,
  ) => {
    setAxisLockMode((current) => (current === mode ? current : mode));
  };

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.key === "Delete" || e.key === "Backspace") && hoveredIdx !== null) {
        if (hoveredIdx === 0 || hoveredIdx === points.length - 1) return;
        const next = [...points];
        next.splice(hoveredIdx, 1);
        onUpdatePoints(next);
        setHoveredIdx(null);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [hoveredIdx, onUpdatePoints, points]);

  useEffect(() => {
    return subscribeToAdjustableLineDragVisualMode(setGlobalDragVisualMode);
  }, []);

  useEffect(() => {
    if (globalDragVisualMode === null) return;
    setHoveredIdx(null);
    setHoveredSegmentIndices(null);
  }, [globalDragVisualMode]);

  useEffect(() => {
    const syncCtrlKey = (event: KeyboardEvent) => {
      setIsCtrlPressed(event.ctrlKey);
    };

    const clearCtrlKey = () => {
      setIsCtrlPressed(false);
    };

    window.addEventListener("keydown", syncCtrlKey);
    window.addEventListener("keyup", syncCtrlKey);
    window.addEventListener("blur", clearCtrlKey);

    return () => {
      window.removeEventListener("keydown", syncCtrlKey);
      window.removeEventListener("keyup", syncCtrlKey);
      window.removeEventListener("blur", clearCtrlKey);
      setAdjustableLineDragVisualMode(null);
    };
  }, []);

  const startDraggingPoint = (
    activeIdx: number,
    startClientX: number,
    startClientY: number,
    rect: DOMRect,
    initialPoints: AudioGainPoint[],
  ) => {
    draggingIdxRef.current = activeIdx;
    pointsRef.current = initialPoints;
    const activePoint = initialPoints[activeIdx];
    if (!activePoint) return;
    const startTime = activePoint.time;
    const startVolumeY = volumeToY(activePoint.volume, geometry);
    const startVolume = activePoint.volume;
    const valueRangePx = Math.max(1, rangePx);
    setActiveSegmentIndices(null);
    setActiveDragIdx(activeIdx);
    updateAxisLockMode(null);
    pointAxisLockRef.current = null;
    applyDragVisualMode("free");

    const mm = (me: MouseEvent) => {
      if (draggingIdxRef.current === null) return;

      const mx = me.clientX - rect.left;
      const dy = me.clientY - startClientY;
      const lockMode = me.shiftKey
        ? pointAxisLockRef.current ??
          (() => {
            const nextLockMode = getAxisLockMode(
              me.clientX - startClientX,
              me.clientY - startClientY,
            );
            if (nextLockMode === "horizontal" || nextLockMode === "vertical") {
              pointAxisLockRef.current = nextLockMode;
            }
            return nextLockMode;
          })()
        : null;

      let t = (mx / rect.width) * duration;
      t = Math.max(0, Math.min(duration, t));

      let newY = startVolumeY + dy / valueRangePx;
      newY = Math.max(0, Math.min(1, newY));

      let volume = yToVolume(newY, geometry);
      if (lockMode === "horizontal") volume = startVolume;
      if (lockMode === "vertical") t = startTime;

      updateAxisLockMode(lockMode);
      applyDragVisualMode(
        lockMode === null
          ? "free"
          : lockMode === "armed"
            ? "armed"
            : lockMode,
      );

      if (!me.shiftKey) {
        pointAxisLockRef.current = null;
      }

      const next = [...pointsRef.current];
      if (next[draggingIdxRef.current]) {
        if (draggingIdxRef.current === 0) t = 0;
        if (draggingIdxRef.current === next.length - 1 && next.length > 1) {
          t = duration;
        }
        next[draggingIdxRef.current] = { time: t, volume };
        pointsRef.current = next;
        onUpdatePoints(next);
        setDragBadge({
          x: me.clientX,
          y: me.clientY - 40,
          volume,
        });
      }
    };

    const mu = () => {
      window.removeEventListener("mousemove", mm);
      window.removeEventListener("mouseup", mu);
      draggingIdxRef.current = null;
      setActiveDragIdx(null);
      updateAxisLockMode(null);
      pointAxisLockRef.current = null;
      applyDragVisualMode(null);
      setDragBadge(null);
      const sorted = sortPointsByTime(pointsRef.current);
      pointsRef.current = sorted;
      onUpdatePoints(sorted);
      commitBatch();
    };

    window.addEventListener("mousemove", mm);
    window.addEventListener("mouseup", mu);
  };

  const startDraggingSegment = (
    activeIndices: number[],
    fixedTimes: number[],
    startClientY: number,
    startVolume: number,
    initialPoints: AudioGainPoint[],
  ) => {
    pointsRef.current = initialPoints;
    const valueRangePx = Math.max(1, rangePx);
    const startVolumeY = volumeToY(startVolume, geometry);
    setIsSegmentDragActive(true);
    setActiveSegmentIndices([
      activeIndices[0],
      activeIndices[activeIndices.length - 1],
    ]);
    applyDragVisualMode("vertical");

    const mm = (me: MouseEvent) => {
      const dy = me.clientY - startClientY;
      let newY = startVolumeY + dy / valueRangePx;
      newY = Math.max(0, Math.min(1, newY));
      const volume = yToVolume(newY, geometry);

      const next = [...pointsRef.current];
      activeIndices.forEach((index, activeIndex) => {
        const point = next[index];
        if (!point) return;
        next[index] = {
          time: fixedTimes[activeIndex] ?? point.time,
          volume,
        };
      });
      pointsRef.current = next;
      onUpdatePoints(next);
      setDragBadge({
        x: me.clientX,
        y: me.clientY - 40,
        volume,
      });
    };

    const mu = () => {
      window.removeEventListener("mousemove", mm);
      window.removeEventListener("mouseup", mu);
      setIsSegmentDragActive(false);
      setActiveSegmentIndices(null);
      applyDragVisualMode(null);
      setDragBadge(null);
      const sorted = sortPointsByTime(pointsRef.current);
      pointsRef.current = sorted;
      onUpdatePoints(sorted);
      commitBatch();
    };

    window.addEventListener("mousemove", mm);
    window.addEventListener("mouseup", mu);
  };

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    if (rect.width <= 0 || duration <= 0) return;
    const clickX = e.clientX - rect.left;
    const time = (clickX / rect.width) * duration;
    e.stopPropagation();

    if (e.ctrlKey) {
      const plan = buildSegmentDragPlan({
        points,
        time,
        duration,
        trackWidth: rect.width,
        getValue: (point) => point.volume,
        createPoint: (pointTime, volume) => ({ time: pointTime, volume }),
      });
      if (!plan) return;

      beginBatch();
      pointsRef.current = plan.points;
      onUpdatePoints(plan.points);
      startDraggingSegment(
        plan.activeIndices,
        plan.activeIndices.map((index) => plan.points[index]?.time ?? time),
        e.clientY,
        plan.startValue,
        plan.points,
      );
      return;
    }

    let nextPoints = [...points];
    beginBatch();

    const expectedVolume = getCosineInterpolatedValueAtTime({
      points: nextPoints,
      time,
      getValue: (point) => point.volume,
    });

    const point = { time, volume: geometry.clampVolume(expectedVolume) };
    nextPoints.push(point);
    nextPoints = sortPointsByTime(nextPoints);
    const activeIdx = nextPoints.indexOf(point);
    pointsRef.current = nextPoints;
    onUpdatePoints(nextPoints);

    startDraggingPoint(activeIdx, e.clientX, e.clientY, rect, nextPoints);
  };

  const handlePointPointerDown = (e: React.PointerEvent, i: number) => {
    e.stopPropagation();
    beginBatch();
    const rect = e.currentTarget.parentElement!.getBoundingClientRect();
    startDraggingPoint(i, e.clientX, e.clientY, rect, pointsRef.current);
  };

  const handleTrackPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (globalDragVisualMode !== null) {
      setHoveredSegmentIndices(null);
      return;
    }

    if (duration <= 0 || points.length < 2) {
      setHoveredSegmentIndices(null);
      return;
    }

    const rect = e.currentTarget.getBoundingClientRect();
    if (rect.width <= 0) {
      setHoveredSegmentIndices(null);
      return;
    }

    const time = ((e.clientX - rect.left) / rect.width) * duration;
    setHoveredSegmentIndices(
      getAdjacentSegmentIndicesAtTime({ points, time, duration }),
    );
  };

  const highlightedSegmentIndices =
    activeSegmentIndices ??
    (globalDragVisualMode === null && isCtrlPressed
      ? hoveredSegmentIndices
      : null);
  const highlightedSegmentPath = getHighlightedVolumeSegmentPath({
    points,
    duration,
    geometry,
    segmentIndices: highlightedSegmentIndices,
  });
  const highlightedSegmentFillPath = getHighlightedVolumeSegmentFillPath({
    points,
    duration,
    geometry,
    segmentIndices: highlightedSegmentIndices,
  });

  if (viewMode === "compact") {
    return (
      <SoftAudioSegmentBlock
        classNamePrefix={classNamePrefix}
        duration={duration}
        points={points}
        isAvailable={isAvailable}
        tone={tone}
        colorVariable={colorVariable}
        onUpdatePoints={onUpdatePoints}
        beginBatch={beginBatch}
        commitBatch={commitBatch}
      />
    );
  }

  return (
    <>
      <div
        className={`${classNamePrefix}-track timeline-lane timeline-lane-strong relative h-7 ${
          isAvailable ? "" : "timeline-lane-unavailable"
        }`}
      >
        <div
          className={`${classNamePrefix}-track-curve-clip absolute inset-0 overflow-hidden`}
          style={{ borderRadius: "inherit" }}
        >
          <AudioWaveformLayer
            sourcePath={sourcePath}
            duration={duration}
            gainPoints={points}
            getVolumeAtTime={getVolumeAtTime}
            colorVariable={colorVariable}
            topPx={geometry.topPx}
            bottomPx={geometry.bottomPx}
            offsetSec={offsetSec}
          />
          <svg
            className={`${classNamePrefix}-track-curve h-full w-full overflow-hidden`}
            preserveAspectRatio="none"
            viewBox="0 0 100 40"
          >
            <line
              className={`${classNamePrefix}-track-baseline ${classNamePrefix}-track-baseline-top`}
              x1="0"
              y1={geometry.topPx}
              x2="100"
              y2={geometry.topPx}
              stroke={`color-mix(in srgb, var(${colorVariable}) 24%, transparent)`}
              vectorEffect="non-scaling-stroke"
            />
            <line
              className={`${classNamePrefix}-track-baseline ${classNamePrefix}-track-baseline-bottom`}
              x1="0"
              y1={geometry.bottomPx}
              x2="100"
              y2={geometry.bottomPx}
              stroke={`color-mix(in srgb, var(${colorVariable}) 18%, transparent)`}
              vectorEffect="non-scaling-stroke"
            />
            <path
              className={`${classNamePrefix}-track-fill-path`}
              d={generateVolumeTrackFillPath({
                points,
                duration,
                geometry,
              })}
              fill={`color-mix(in srgb, var(${colorVariable}) 12%, transparent)`}
            />
            <path
              className={`${classNamePrefix}-track-main-path`}
              d={generateVolumeTrackPath({
                points,
                duration,
                geometry,
              })}
              fill="none"
              stroke={`var(${colorVariable})`}
              strokeWidth="1.5"
              vectorEffect="non-scaling-stroke"
            />
            {highlightedSegmentFillPath && (
              <path
                className="timeline-segment-highlight-fill"
                d={highlightedSegmentFillPath}
                fill="currentColor"
                style={{ color: `var(${colorVariable})` }}
              />
            )}
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
                style={{ color: `var(${colorVariable})` }}
              />
            )}
          </svg>
        </div>
        <div
          className={`${classNamePrefix}-layer absolute inset-0 z-10 ${
            isAvailable ? "pointer-events-auto" : "pointer-events-none"
          }`}
          onPointerDown={handlePointerDown}
          onPointerMove={handleTrackPointerMove}
          onPointerLeave={() => {
            if (!isSegmentDragActive) setHoveredSegmentIndices(null);
          }}
        >
          {points.map((point, i) => (
            <div
              key={i}
              className={`${classNamePrefix}-point timeline-control-point absolute -translate-x-1/2 -translate-y-1/2 cursor-pointer ${
                hoveredIdx === i
                  ? hoveredRingClass
                  : "hover:scale-110"
              }`}
              data-tone={classNamePrefix}
              data-state={
                hoveredIdx === i || activeDragIdx === i ? "active" : "idle"
              }
              data-lock-mode={
                activeDragIdx === i ? (axisLockMode ?? undefined) : undefined
              }
              style={{
                left: `${(point.time / duration) * 100}%`,
                top: volumeToTrackYPercent(point.volume, geometry),
                color: `var(${colorVariable})`,
              }}
              onMouseEnter={() => {
                if (globalDragVisualMode !== null) return;
                setHoveredIdx(i);
              }}
              onMouseLeave={() => setHoveredIdx(null)}
              onPointerDown={(e) => handlePointPointerDown(e, i)}
            />
          ))}
        </div>
      </div>

      {dragBadge && (
        <div
          className={`${classNamePrefix}-track-drag-badge timeline-chip fixed z-[100] px-3 py-1.5 text-white font-bold text-sm pointer-events-none -translate-x-1/2 -translate-y-full`}
          data-tone={classNamePrefix}
          data-active="true"
          style={{ left: dragBadge.x, top: dragBadge.y }}
        >
          {Math.round(dragBadge.volume * 100)}%
        </div>
      )}
    </>
  );
};
