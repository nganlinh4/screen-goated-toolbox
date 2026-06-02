import React, { useEffect, useRef, useState } from "react";
import { MicAudioPoint, VideoSegment } from "@/types/video";
import {
  buildFlatMicAudioPoints,
  clampMicAudioVolume,
  getMicAudioVolumeAtTime,
} from "@/lib/micAudio";
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

const MIC_TRACK_TOP_PX = 5;
const MIC_TRACK_BOTTOM_PX = 35;
const MIC_TRACK_RANGE_PX = MIC_TRACK_BOTTOM_PX - MIC_TRACK_TOP_PX;
const MIC_TRACK_VIEWBOX_HEIGHT = 40;
const MIC_TRACK_GEOMETRY = {
  topPx: MIC_TRACK_TOP_PX,
  bottomPx: MIC_TRACK_BOTTOM_PX,
  viewBoxHeight: MIC_TRACK_VIEWBOX_HEIGHT,
  emptyPathY: MIC_TRACK_BOTTOM_PX,
  clampVolume: clampMicAudioVolume,
} satisfies VolumeTrackGeometry;

interface MicTrackProps {
  segment: VideoSegment;
  duration: number;
  isAvailable: boolean;
  sourcePath?: string | null;
  viewMode?: "compact" | "volume";
  onUpdateMicAudioPoints: (points: MicAudioPoint[]) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export const MicTrack: React.FC<MicTrackProps> = ({
  segment,
  duration,
  isAvailable,
  sourcePath,
  viewMode = "volume",
  onUpdateMicAudioPoints,
  beginBatch,
  commitBatch,
}) => {
  const points = segment.micAudioPoints?.length
    ? segment.micAudioPoints
    : buildFlatMicAudioPoints(duration);
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
        onUpdateMicAudioPoints(next);
        setHoveredIdx(null);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [hoveredIdx, onUpdateMicAudioPoints, points]);

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
    initialPoints: MicAudioPoint[],
  ) => {
    draggingIdxRef.current = activeIdx;
    pointsRef.current = initialPoints;
    const activePoint = initialPoints[activeIdx];
    if (!activePoint) return;
    const startTime = activePoint.time;
    const startVolumeY = volumeToY(activePoint.volume, MIC_TRACK_GEOMETRY);
    const startVolume = activePoint.volume;
    const valueRangePx = Math.max(1, MIC_TRACK_RANGE_PX);
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

      let volume = yToVolume(newY, MIC_TRACK_GEOMETRY);
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
        onUpdateMicAudioPoints(next);
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
      onUpdateMicAudioPoints(sorted);
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
    initialPoints: MicAudioPoint[],
  ) => {
    pointsRef.current = initialPoints;
    const valueRangePx = Math.max(1, MIC_TRACK_RANGE_PX);
    const startVolumeY = volumeToY(startVolume, MIC_TRACK_GEOMETRY);
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
      const volume = yToVolume(newY, MIC_TRACK_GEOMETRY);

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
      onUpdateMicAudioPoints(next);
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
      onUpdateMicAudioPoints(sorted);
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
      onUpdateMicAudioPoints(plan.points);
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

    const point = { time, volume: clampMicAudioVolume(expectedVolume) };
    nextPoints.push(point);
    nextPoints = sortPointsByTime(nextPoints);
    const activeIdx = nextPoints.indexOf(point);
    pointsRef.current = nextPoints;
    onUpdateMicAudioPoints(nextPoints);

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
    geometry: MIC_TRACK_GEOMETRY,
    segmentIndices: highlightedSegmentIndices,
  });
  const highlightedSegmentFillPath = getHighlightedVolumeSegmentFillPath({
    points,
    duration,
    geometry: MIC_TRACK_GEOMETRY,
    segmentIndices: highlightedSegmentIndices,
  });

  if (viewMode === "compact") {
    return (
      <SoftAudioSegmentBlock
        classNamePrefix="mic-audio"
        duration={duration}
        points={points}
        isAvailable={isAvailable}
        tone="info"
        colorVariable="--timeline-mic-audio-color"
        onUpdatePoints={onUpdateMicAudioPoints}
        beginBatch={beginBatch}
        commitBatch={commitBatch}
      />
    );
  }

  return (
    <>
      <div
        className={`mic-audio-track timeline-lane timeline-lane-strong relative h-7 ${
          isAvailable ? "" : "timeline-lane-unavailable"
        }`}
      >
        <div
          className="mic-audio-track-curve-clip absolute inset-0 overflow-hidden"
          style={{ borderRadius: "inherit" }}
        >
          <AudioWaveformLayer
            sourcePath={sourcePath}
            duration={duration}
            gainPoints={points}
            getVolumeAtTime={getMicAudioVolumeAtTime}
            colorVariable="--timeline-mic-audio-color"
            topPx={MIC_TRACK_TOP_PX}
            bottomPx={MIC_TRACK_BOTTOM_PX}
            offsetSec={segment.micAudioOffsetSec ?? 0}
          />
          <svg
            className="mic-audio-track-curve h-full w-full overflow-hidden"
            preserveAspectRatio="none"
            viewBox="0 0 100 40"
          >
            <line
              className="mic-audio-track-baseline mic-audio-track-baseline-top"
              x1="0"
              y1={MIC_TRACK_TOP_PX}
              x2="100"
              y2={MIC_TRACK_TOP_PX}
              stroke="color-mix(in srgb, var(--timeline-mic-audio-color) 24%, transparent)"
              vectorEffect="non-scaling-stroke"
            />
            <line
              className="mic-audio-track-baseline mic-audio-track-baseline-bottom"
              x1="0"
              y1={MIC_TRACK_BOTTOM_PX}
              x2="100"
              y2={MIC_TRACK_BOTTOM_PX}
              stroke="color-mix(in srgb, var(--timeline-mic-audio-color) 18%, transparent)"
              vectorEffect="non-scaling-stroke"
            />
            <path
              className="mic-audio-track-fill-path"
              d={generateVolumeTrackFillPath({
                points,
                duration,
                geometry: MIC_TRACK_GEOMETRY,
              })}
              fill="color-mix(in srgb, var(--timeline-mic-audio-color) 12%, transparent)"
            />
            <path
              className="mic-audio-track-main-path"
              d={generateVolumeTrackPath({
                points,
                duration,
                geometry: MIC_TRACK_GEOMETRY,
              })}
              fill="none"
              stroke="var(--timeline-mic-audio-color)"
              strokeWidth="1.5"
              vectorEffect="non-scaling-stroke"
            />
            {highlightedSegmentFillPath && (
              <path
                className="timeline-segment-highlight-fill"
                d={highlightedSegmentFillPath}
                fill="currentColor"
                style={{ color: "var(--timeline-mic-audio-color)" }}
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
                style={{ color: "var(--timeline-mic-audio-color)" }}
              />
            )}
          </svg>
        </div>
        <div
          className={`mic-audio-layer absolute inset-0 z-10 ${
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
              className={`mic-audio-point timeline-control-point absolute -translate-x-1/2 -translate-y-1/2 cursor-pointer ${
                hoveredIdx === i
                  ? "ring-2 ring-[var(--timeline-mic-audio-color)]/40"
                  : "hover:scale-110"
              }`}
              data-tone="mic-audio"
              data-state={
                hoveredIdx === i || activeDragIdx === i ? "active" : "idle"
              }
              data-lock-mode={
                activeDragIdx === i ? (axisLockMode ?? undefined) : undefined
              }
              style={{
                left: `${(point.time / duration) * 100}%`,
                top: volumeToTrackYPercent(point.volume, MIC_TRACK_GEOMETRY),
                color: "var(--timeline-mic-audio-color)",
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
          className="mic-audio-track-drag-badge timeline-chip fixed z-[100] px-3 py-1.5 text-white font-bold text-sm pointer-events-none -translate-x-1/2 -translate-y-full"
          data-tone="mic-audio"
          data-active="true"
          style={{ left: dragBadge.x, top: dragBadge.y }}
        >
          {Math.round(dragBadge.volume * 100)}%
        </div>
      )}
    </>
  );
};
