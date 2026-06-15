import React from "react";
import type { AudioGainPoint } from "@/types/video";
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
import { useAdjustableLineTrack } from "./useAdjustableLineTrack";

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
  const {
    hoveredIdx,
    setHoveredIdx,
    activeDragIdx,
    axisLockMode,
    dragBadge,
    isSegmentDragActive,
    globalDragVisualMode,
    highlightedSegmentIndices,
    handleTrackPointerDown,
    handlePointPointerDown,
    handleTrackPointerMove,
    setHoveredSegmentIndices,
  } = useAdjustableLineTrack<AudioGainPoint>({
    points,
    duration,
    onUpdatePoints,
    getValue: (point) => point.volume,
    createPoint: (time, volume) => ({ time, volume }),
    clampNewValue: (value) => geometry.clampVolume(value),
    resolvePointValue: ({ dy, startPoint }) => {
      const valueRangePx = Math.max(1, rangePx);
      const startVolumeY = volumeToY(startPoint.volume, geometry);
      const newY = Math.max(0, Math.min(1, startVolumeY + dy / valueRangePx));
      return yToVolume(newY, geometry);
    },
    resolveSegmentValue: ({ dy, startValue }) => {
      const valueRangePx = Math.max(1, rangePx);
      const startVolumeY = volumeToY(startValue, geometry);
      const newY = Math.max(0, Math.min(1, startVolumeY + dy / valueRangePx));
      return yToVolume(newY, geometry);
    },
    axisLockEnabled: true,
    makeBadge: (me, value) => ({ x: me.clientX, y: me.clientY - 40, value }),
    beginBatch,
    commitBatch,
  });

  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    if (rect.width <= 0 || duration <= 0) return;
    const clickX = e.clientX - rect.left;
    const time = (clickX / rect.width) * duration;
    e.stopPropagation();
    handleTrackPointerDown(e, rect, time);
  };

  const onPointPointerDown = (e: React.PointerEvent, i: number) => {
    e.stopPropagation();
    const rect = e.currentTarget.parentElement!.getBoundingClientRect();
    handlePointPointerDown(rect, e.clientX, e.clientY, i);
  };

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
              onPointerDown={(e) => onPointPointerDown(e, i)}
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
          {Math.round(dragBadge.value * 100)}%
        </div>
      )}
    </>
  );
};
