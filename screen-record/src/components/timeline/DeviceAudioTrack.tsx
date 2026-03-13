import React, { useEffect, useRef, useState } from "react";
import { DeviceAudioPoint, VideoSegment } from "@/types/video";
import { buildFlatDeviceAudioPoints, clampDeviceAudioVolume } from "@/lib/deviceAudio";
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

const DEVICE_AUDIO_TRACK_TOP_PX = 5;
const DEVICE_AUDIO_TRACK_BOTTOM_PX = 35;
const DEVICE_AUDIO_TRACK_RANGE_PX =
  DEVICE_AUDIO_TRACK_BOTTOM_PX - DEVICE_AUDIO_TRACK_TOP_PX;
const DEVICE_AUDIO_TRACK_VIEWBOX_HEIGHT = 40;

function volumeToY(volume: number) {
  return 1 - clampDeviceAudioVolume(volume);
}

function yToVolume(y: number) {
  return clampDeviceAudioVolume(1 - y);
}

function volumeToTrackY(volume: number) {
  return DEVICE_AUDIO_TRACK_TOP_PX + volumeToY(volume) * DEVICE_AUDIO_TRACK_RANGE_PX;
}

function volumeToTrackYPercent(volume: number) {
  return `${(volumeToTrackY(volume) / DEVICE_AUDIO_TRACK_VIEWBOX_HEIGHT) * 100}%`;
}

interface DeviceAudioTrackProps {
  segment: VideoSegment;
  duration: number;
  onUpdateDeviceAudioPoints: (points: DeviceAudioPoint[]) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export const DeviceAudioTrack: React.FC<DeviceAudioTrackProps> = ({
  segment,
  duration,
  onUpdateDeviceAudioPoints,
  beginBatch,
  commitBatch,
}) => {
  const points = segment.deviceAudioPoints?.length
    ? segment.deviceAudioPoints
    : buildFlatDeviceAudioPoints(duration);
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
        onUpdateDeviceAudioPoints(next);
        setHoveredIdx(null);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [hoveredIdx, onUpdateDeviceAudioPoints, points]);

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

  const generatePath = () => {
    if (points.length === 0) {
      return `M 0 ${DEVICE_AUDIO_TRACK_TOP_PX} L 100 ${DEVICE_AUDIO_TRACK_TOP_PX}`;
    }
    const sorted = [...points].sort((a, b) => a.time - b.time);
    const toX = (time: number) => (duration > 0 ? (time / duration) * 100 : 0);
    const toY = (volume: number) => volumeToTrackY(volume);
    const x0 = toX(sorted[0].time);
    const y0 = toY(sorted[0].volume);
    let d = `M 0 ${y0} `;
    if (x0 > 0) d += `L ${x0} ${y0} `;

    for (let i = 1; i < sorted.length; i++) {
      const left = sorted[i - 1];
      const right = sorted[i];
      const x1 = toX(left.time);
      const y1 = toY(left.volume);
      const x2 = toX(right.time);
      const y2 = toY(right.volume);
      const dx = x2 - x1;
      d += `C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} `;
    }

    const xLast = toX(sorted[sorted.length - 1].time);
    const yLast = toY(sorted[sorted.length - 1].volume);
    if (xLast < 100) d += `L 100 ${yLast} `;
    return d;
  };

  const generateFillPath = () => {
    if (points.length === 0) return "";
    const sorted = [...points].sort((a, b) => a.time - b.time);
    const toX = (time: number) => (duration > 0 ? (time / duration) * 100 : 0);
    const toY = (volume: number) => volumeToTrackY(volume);
    const x0 = toX(sorted[0].time);
    const y0 = toY(sorted[0].volume);
    let d = `M 0 40 L ${x0} 40 L ${x0} ${y0} `;

    for (let i = 1; i < sorted.length; i++) {
      const left = sorted[i - 1];
      const right = sorted[i];
      const x1 = toX(left.time);
      const y1 = toY(left.volume);
      const x2 = toX(right.time);
      const y2 = toY(right.volume);
      const dx = x2 - x1;
      d += `C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} `;
    }

    const xLast = toX(sorted[sorted.length - 1].time);
    d += `L ${xLast} 40 L 100 40 Z`;
    return d;
  };

  const getHighlightedSegmentPath = (
    segmentIndices: AdjacentSegmentIndices | null,
  ) => {
    if (!segmentIndices) return "";

    const sorted = sortPointsByTime(points);
    const [leftIdx, rightIdx] = segmentIndices;
    const left = sorted[leftIdx];
    const right = sorted[rightIdx];
    if (!left || !right || right.time <= left.time) return "";

    const toX = (time: number) => (duration > 0 ? (time / duration) * 100 : 0);
    const toY = (volume: number) => volumeToTrackY(volume);
    const x1 = toX(left.time);
    const y1 = toY(left.volume);
    const x2 = toX(right.time);
    const y2 = toY(right.volume);
    const dx = x2 - x1;
    return `M ${x1} ${y1} C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2}`;
  };

  const getHighlightedSegmentFillPath = (
    segmentIndices: AdjacentSegmentIndices | null,
  ) => {
    if (!segmentIndices) return "";

    const sorted = sortPointsByTime(points);
    const [leftIdx, rightIdx] = segmentIndices;
    const left = sorted[leftIdx];
    const right = sorted[rightIdx];
    if (!left || !right || right.time <= left.time) return "";

    const toX = (time: number) => (duration > 0 ? (time / duration) * 100 : 0);
    const toY = (volume: number) => volumeToTrackY(volume);
    const x1 = toX(left.time);
    const y1 = toY(left.volume);
    const x2 = toX(right.time);
    const y2 = toY(right.volume);
    const dx = x2 - x1;
    return `M ${x1} 40 L ${x1} ${y1} C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} L ${x2} 40 Z`;
  };

  const startDraggingPoint = (
    activeIdx: number,
    startClientX: number,
    startClientY: number,
    rect: DOMRect,
    initialPoints: DeviceAudioPoint[],
  ) => {
    draggingIdxRef.current = activeIdx;
    pointsRef.current = initialPoints;
    const activePoint = initialPoints[activeIdx];
    if (!activePoint) return;
    const startTime = activePoint.time;
    const startVolumeY = volumeToY(activePoint.volume);
    const startVolume = activePoint.volume;
    const valueRangePx = Math.max(1, DEVICE_AUDIO_TRACK_RANGE_PX);
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

      let volume = yToVolume(newY);
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
        onUpdateDeviceAudioPoints(next);
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
      onUpdateDeviceAudioPoints(sorted);
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
    initialPoints: DeviceAudioPoint[],
  ) => {
    pointsRef.current = initialPoints;
    const valueRangePx = Math.max(1, DEVICE_AUDIO_TRACK_RANGE_PX);
    const startVolumeY = volumeToY(startVolume);
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
      const volume = yToVolume(newY);

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
      onUpdateDeviceAudioPoints(next);
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
      onUpdateDeviceAudioPoints(sorted);
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
      onUpdateDeviceAudioPoints(plan.points);
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

    const point = { time, volume: clampDeviceAudioVolume(expectedVolume) };
    nextPoints.push(point);
    nextPoints = sortPointsByTime(nextPoints);
    const activeIdx = nextPoints.indexOf(point);
    pointsRef.current = nextPoints;
    onUpdateDeviceAudioPoints(nextPoints);

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
  const highlightedSegmentPath = getHighlightedSegmentPath(
    highlightedSegmentIndices,
  );
  const highlightedSegmentFillPath = getHighlightedSegmentFillPath(
    highlightedSegmentIndices,
  );

  return (
    <>
      <div className="device-audio-track timeline-lane timeline-lane-strong relative h-10">
        <div
          className="device-audio-track-curve-clip absolute inset-0 overflow-hidden"
          style={{ borderRadius: "inherit" }}
        >
          <svg
            className="device-audio-track-curve h-full w-full overflow-hidden"
            preserveAspectRatio="none"
            viewBox="0 0 100 40"
          >
            <line
              className="device-audio-track-baseline device-audio-track-baseline-top"
              x1="0"
              y1={DEVICE_AUDIO_TRACK_TOP_PX}
              x2="100"
              y2={DEVICE_AUDIO_TRACK_TOP_PX}
              stroke="color-mix(in srgb, var(--timeline-device-audio-color) 24%, transparent)"
              vectorEffect="non-scaling-stroke"
            />
            <line
              className="device-audio-track-baseline device-audio-track-baseline-bottom"
              x1="0"
              y1={DEVICE_AUDIO_TRACK_BOTTOM_PX}
              x2="100"
              y2={DEVICE_AUDIO_TRACK_BOTTOM_PX}
              stroke="color-mix(in srgb, var(--timeline-device-audio-color) 18%, transparent)"
              vectorEffect="non-scaling-stroke"
            />
            <path
              className="device-audio-track-fill-path"
              d={generateFillPath()}
              fill="color-mix(in srgb, var(--timeline-device-audio-color) 12%, transparent)"
            />
            <path
              className="device-audio-track-main-path"
              d={generatePath()}
              fill="none"
              stroke="var(--timeline-device-audio-color)"
              strokeWidth="1.5"
              vectorEffect="non-scaling-stroke"
            />
            {highlightedSegmentFillPath && (
              <path
                className="timeline-segment-highlight-fill"
                d={highlightedSegmentFillPath}
                fill="currentColor"
                style={{ color: "var(--timeline-device-audio-color)" }}
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
                style={{ color: "var(--timeline-device-audio-color)" }}
              />
            )}
          </svg>
        </div>
        <div
          className="device-audio-layer absolute inset-0 z-10 pointer-events-auto"
          onPointerDown={handlePointerDown}
          onPointerMove={handleTrackPointerMove}
          onPointerLeave={() => {
            if (!isSegmentDragActive) setHoveredSegmentIndices(null);
          }}
        >
          {points.map((point, i) => (
            <div
              key={i}
              className={`device-audio-point timeline-control-point absolute -translate-x-1/2 -translate-y-1/2 cursor-pointer ${
                hoveredIdx === i
                  ? "ring-2 ring-[var(--timeline-device-audio-color)]/40"
                  : "hover:scale-110"
              }`}
              data-tone="device-audio"
              data-state={
                hoveredIdx === i || activeDragIdx === i ? "active" : "idle"
              }
                data-lock-mode={
                activeDragIdx === i ? (axisLockMode ?? undefined) : undefined
              }
              style={{
                left: `${(point.time / duration) * 100}%`,
                top: volumeToTrackYPercent(point.volume),
                color: "var(--timeline-device-audio-color)",
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
          className="device-audio-track-drag-badge timeline-chip fixed z-[100] px-3 py-1.5 text-white font-bold text-sm pointer-events-none -translate-x-1/2 -translate-y-full"
          data-tone="device-audio"
          data-active="true"
          style={{ left: dragBadge.x, top: dragBadge.y }}
        >
          {Math.round(dragBadge.volume * 100)}%
        </div>
      )}
    </>
  );
};
