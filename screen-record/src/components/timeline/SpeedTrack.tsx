import React from 'react';
import { VideoSegment, SpeedPoint } from '@/types/video';
import { type AdjacentSegmentIndices } from './adjustableLineUtils';
import {
  generateCurveFillPath,
  generateCurvePath,
  getHighlightedCurveSegmentFillPath,
  getHighlightedCurveSegmentPath,
} from './curvePath';
import { useAdjustableLineTrack } from './useAdjustableLineTrack';

// Logarithmic vertical mapping for intuitive dragging:
// 1x in the middle, 16x at top, 0.1x at bottom.
const SPEED_TRACK_TOP_PX = 4;
const SPEED_TRACK_RANGE_PX = 32;
const SPEED_TRACK_VIEWBOX_HEIGHT = 40;

function speedToY(speed: number) {
  if (speed >= 1) {
    return 0.5 - 0.5 * (Math.log2(speed) / 4);
  }
  return 0.5 + 0.5 * Math.abs(Math.log10(speed));
}

function yToSpeed(y: number) {
  if (y <= 0.5) {
    return Math.pow(2, 4 * ((0.5 - y) / 0.5));
  }
  return Math.pow(10, -((y - 0.5) / 0.5));
}

function speedToTrackY(speed: number) {
  return SPEED_TRACK_TOP_PX + speedToY(speed) * SPEED_TRACK_RANGE_PX;
}

function speedToTrackYPercent(speed: number) {
  return `${(speedToTrackY(speed) / SPEED_TRACK_VIEWBOX_HEIGHT) * 100}%`;
}

interface SpeedTrackProps {
  segment: VideoSegment;
  duration: number;
  onUpdateSpeedPoints: (points: SpeedPoint[]) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export const SpeedTrack: React.FC<SpeedTrackProps> = ({
  segment,
  duration,
  onUpdateSpeedPoints,
  beginBatch,
  commitBatch,
}) => {
  const points = segment.speedPoints?.length
    ? segment.speedPoints
    : [{ time: 0, speed: 1 }, { time: duration, speed: 1 }];

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
  } = useAdjustableLineTrack<SpeedPoint>({
    points,
    duration,
    onUpdatePoints: onUpdateSpeedPoints,
    getValue: (point) => point.speed,
    createPoint: (time, speed) => ({ time, speed }),
    resolvePointValue: ({ dy, startPoint, rect }) => {
      const startSpeedY = speedToY(startPoint.speed);
      const newY = Math.max(0, Math.min(1, startSpeedY + (dy * 0.15) / rect.height));
      return Math.max(0.1, Math.min(16, yToSpeed(newY)));
    },
    resolveSegmentValue: ({ dy, startValue, rect }) => {
      const startSpeedY = speedToY(startValue);
      const newY = Math.max(0, Math.min(1, startSpeedY + (dy * 0.15) / rect.height));
      return Math.max(0.1, Math.min(16, yToSpeed(newY)));
    },
    axisLockEnabled: true,
    makeBadge: (me, value) => ({ x: me.clientX, y: me.clientY - 40, value }),
    beginBatch,
    commitBatch,
  });

  const speedPointToY = (p: SpeedPoint) => speedToTrackY(p.speed);

  const generatePath = () =>
    generateCurvePath({ points, duration, toY: speedPointToY, emptyPathY: 20 });

  const generateFillPath = () =>
    generateCurveFillPath({
      points,
      duration,
      toY: speedPointToY,
      baselineY: SPEED_TRACK_VIEWBOX_HEIGHT,
    });

  const getHighlightedSegmentPath = (segmentIndices: AdjacentSegmentIndices | null) =>
    getHighlightedCurveSegmentPath({ points, duration, toY: speedPointToY, segmentIndices });

  const getHighlightedSegmentFillPath = (segmentIndices: AdjacentSegmentIndices | null) =>
    getHighlightedCurveSegmentFillPath({
      points,
      duration,
      toY: speedPointToY,
      baselineY: SPEED_TRACK_VIEWBOX_HEIGHT,
      segmentIndices,
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

  const highlightedSegmentPath = getHighlightedSegmentPath(
    highlightedSegmentIndices,
  );
  const highlightedSegmentFillPath = getHighlightedSegmentFillPath(
    highlightedSegmentIndices,
  );

  return (
    <>
      <div
        className="speed-track timeline-lane timeline-lane-strong relative h-7"
      >
        <div
          className="speed-track-curve-clip absolute inset-0 overflow-hidden"
          style={{ borderRadius: "inherit" }}
        >
          <svg className="speed-track-curve h-full w-full overflow-hidden" preserveAspectRatio="none" viewBox="0 0 100 40">
            <line
              className="speed-track-baseline"
              x1="0"
              y1="20"
              x2="100"
              y2="20"
              stroke="color-mix(in srgb, var(--timeline-speed-color) 24%, transparent)"
              strokeDasharray="2 2"
              vectorEffect="non-scaling-stroke"
            />
            <path
              className="speed-track-fill-path"
              d={generateFillPath()}
              fill="color-mix(in srgb, var(--timeline-speed-color) 12%, transparent)"
            />
            <path
              className="speed-track-main-path"
              d={generatePath()}
              fill="none"
              stroke="var(--timeline-speed-color)"
              strokeWidth="1.5"
              vectorEffect="non-scaling-stroke"
            />
            {highlightedSegmentFillPath && (
              <path
                className="timeline-segment-highlight-fill"
                d={highlightedSegmentFillPath}
                fill="currentColor"
                style={{ color: 'var(--timeline-speed-color)' }}
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
                style={{ color: 'var(--timeline-speed-color)' }}
              />
            )}
          </svg>
        </div>
        <div
          className="speed-influence-layer absolute inset-0 z-10 pointer-events-auto"
          onPointerDown={handlePointerDown}
          onPointerMove={handleTrackPointerMove}
          onPointerLeave={() => {
            if (!isSegmentDragActive) setHoveredSegmentIndices(null);
          }}
        >
          {points.map((p, i) => (
            <div
              key={i}
              className={`speed-influence-point timeline-control-point absolute -translate-x-1/2 -translate-y-1/2 cursor-pointer ${
                hoveredIdx === i ? 'ring-2 ring-[var(--timeline-speed-color)]/40' : 'hover:scale-110'
              }`}
              data-tone="speed"
              data-state={
                hoveredIdx === i || activeDragIdx === i ? "active" : "idle"
              }
              data-lock-mode={
                activeDragIdx === i ? (axisLockMode ?? undefined) : undefined
              }
              style={{
                left: `${(p.time / duration) * 100}%`,
                top: speedToTrackYPercent(p.speed),
                color: 'var(--timeline-speed-color)',
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
          className="speed-track-drag-badge timeline-chip fixed z-[100] px-3 py-1.5 text-white font-bold text-sm pointer-events-none -translate-x-1/2 -translate-y-full"
          data-tone="speed"
          data-active="true"
          style={{ left: dragBadge.x, top: dragBadge.y }}
        >
          {dragBadge.value.toFixed(2)}x
        </div>
      )}
    </>
  );
};
