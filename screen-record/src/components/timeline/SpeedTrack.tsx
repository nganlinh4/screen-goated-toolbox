import React, { useEffect, useRef, useState } from 'react';
import { VideoSegment, SpeedPoint } from '@/types/video';
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
} from './adjustableLineUtils';

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
  const draggingIdxRef = useRef<number | null>(null);
  const pointsRef = useRef(points);
  pointsRef.current = points;
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);
  const [dragBadge, setDragBadge] = useState<{ x: number; y: number; speed: number } | null>(null);
  const [isCtrlPressed, setIsCtrlPressed] = useState(false);
  const [activeDragIdx, setActiveDragIdx] = useState<number | null>(null);
  const [axisLockMode, setAxisLockMode] = useState<'armed' | 'horizontal' | 'vertical' | null>(null);
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
  const pointAxisLockRef = useRef<'horizontal' | 'vertical' | null>(null);

  const applyDragVisualMode = (mode: AdjustableLineDragVisualMode | null) => {
    if (dragVisualModeRef.current === mode) return;
    dragVisualModeRef.current = mode;
    setAdjustableLineDragVisualMode(mode);
  };

  const updateAxisLockMode = (
    mode: 'armed' | 'horizontal' | 'vertical' | null,
  ) => {
    setAxisLockMode((current) => (current === mode ? current : mode));
  };

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.key === 'Delete' || e.key === 'Backspace') && hoveredIdx !== null) {
        // Prevent deleting the anchor points
        if (hoveredIdx === 0 || hoveredIdx === points.length - 1) return;
        const next = [...points];
        next.splice(hoveredIdx, 1);
        onUpdateSpeedPoints(next);
        setHoveredIdx(null);
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [hoveredIdx, points, onUpdateSpeedPoints]);

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

    window.addEventListener('keydown', syncCtrlKey);
    window.addEventListener('keyup', syncCtrlKey);
    window.addEventListener('blur', clearCtrlKey);

    return () => {
      window.removeEventListener('keydown', syncCtrlKey);
      window.removeEventListener('keyup', syncCtrlKey);
      window.removeEventListener('blur', clearCtrlKey);
      setAdjustableLineDragVisualMode(null);
    };
  }, []);

  const generatePath = () => {
    if (points.length === 0) return 'M 0 20 L 100 20';
    const sorted = [...points].sort((a, b) => a.time - b.time);
    const toX = (time: number) => (isFinite(duration) && duration > 0 ? (time / duration) * 100 : 0);
    const toY = (speed: number) => speedToTrackY(speed);
    const x0 = toX(sorted[0].time);
    const y0 = toY(sorted[0].speed);
    let d = `M 0 ${y0} `;
    if (x0 > 0) d += `L ${x0} ${y0} `;

    for (let i = 1; i < sorted.length; i++) {
      const p1 = sorted[i - 1];
      const p2 = sorted[i];
      const x1 = toX(p1.time);
      const y1 = toY(p1.speed);
      const x2 = toX(p2.time);
      const y2 = toY(p2.speed);
      const dx = x2 - x1;
      d += `C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} `;
    }

    const xLast = toX(sorted[sorted.length - 1].time);
    const yLast = toY(sorted[sorted.length - 1].speed);
    if (xLast < 100) d += `L 100 ${yLast} `;
    return d;
  };

  const generateFillPath = () => {
    if (points.length === 0) return '';
    const sorted = [...points].sort((a, b) => a.time - b.time);
    const toX = (time: number) => (isFinite(duration) && duration > 0 ? (time / duration) * 100 : 0);
    const toY = (speed: number) => speedToTrackY(speed);
    const x0 = toX(sorted[0].time);
    const y0 = toY(sorted[0].speed);
    let d = `M 0 40 L ${x0} 40 L ${x0} ${y0} `;

    for (let i = 1; i < sorted.length; i++) {
      const p1 = sorted[i - 1];
      const p2 = sorted[i];
      const x1 = toX(p1.time);
      const y1 = toY(p1.speed);
      const x2 = toX(p2.time);
      const y2 = toY(p2.speed);
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
    if (!segmentIndices) return '';

    const sorted = sortPointsByTime(points);
    const [leftIdx, rightIdx] = segmentIndices;
    const left = sorted[leftIdx];
    const right = sorted[rightIdx];
    if (!left || !right || right.time <= left.time) return '';

    const toX = (time: number) => (isFinite(duration) && duration > 0 ? (time / duration) * 100 : 0);
    const toY = (speed: number) => speedToTrackY(speed);
    const x1 = toX(left.time);
    const y1 = toY(left.speed);
    const x2 = toX(right.time);
    const y2 = toY(right.speed);
    const dx = x2 - x1;
    return `M ${x1} ${y1} C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2}`;
  };

  const getHighlightedSegmentFillPath = (
    segmentIndices: AdjacentSegmentIndices | null,
  ) => {
    if (!segmentIndices) return '';

    const sorted = sortPointsByTime(points);
    const [leftIdx, rightIdx] = segmentIndices;
    const left = sorted[leftIdx];
    const right = sorted[rightIdx];
    if (!left || !right || right.time <= left.time) return '';

    const toX = (time: number) => (isFinite(duration) && duration > 0 ? (time / duration) * 100 : 0);
    const toY = (speed: number) => speedToTrackY(speed);
    const x1 = toX(left.time);
    const y1 = toY(left.speed);
    const x2 = toX(right.time);
    const y2 = toY(right.speed);
    const dx = x2 - x1;
    return `M ${x1} 40 L ${x1} ${y1} C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} L ${x2} 40 Z`;
  };

  const startDraggingPoint = (
    activeIdx: number,
    startClientX: number,
    startClientY: number,
    rect: DOMRect,
    initialPoints: SpeedPoint[],
  ) => {
    draggingIdxRef.current = activeIdx;
    pointsRef.current = initialPoints;
    const activePoint = initialPoints[activeIdx];
    if (!activePoint) return;
    const startTime = activePoint.time;
    const startSpeedY = speedToY(activePoint.speed);
    const startSpeed = activePoint.speed;
    setActiveSegmentIndices(null);
    setActiveDragIdx(activeIdx);
    updateAxisLockMode(null);
    pointAxisLockRef.current = null;
    applyDragVisualMode('free');

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
            if (nextLockMode === 'horizontal' || nextLockMode === 'vertical') {
              pointAxisLockRef.current = nextLockMode;
            }
            return nextLockMode;
          })()
        : null;

      let t = (mx / rect.width) * duration;
      t = Math.max(0, Math.min(duration, t));

      // Lower vertical sensitivity for fine-grained speed tuning.
      let newY = startSpeedY + (dy * 0.15) / rect.height;
      newY = Math.max(0, Math.min(1, newY));

      let v = yToSpeed(newY);
      v = Math.max(0.1, Math.min(16, v));

      if (lockMode === 'horizontal') v = startSpeed;
      if (lockMode === 'vertical') t = startTime;

      updateAxisLockMode(lockMode);
      applyDragVisualMode(
        lockMode === null
          ? 'free'
          : lockMode === 'armed'
            ? 'armed'
            : lockMode,
      );

      if (!me.shiftKey) {
        pointAxisLockRef.current = null;
      }

      const next = [...pointsRef.current];
      if (next[draggingIdxRef.current]) {
        if (draggingIdxRef.current === 0) t = 0;
        if (draggingIdxRef.current === next.length - 1 && next.length > 1) t = duration;
        next[draggingIdxRef.current] = { time: t, speed: v };
        pointsRef.current = next;
        onUpdateSpeedPoints(next);
        setDragBadge({
          x: me.clientX,
          y: me.clientY - 40,
          speed: v,
        });
      }
    };

    const mu = () => {
      window.removeEventListener('mousemove', mm);
      window.removeEventListener('mouseup', mu);
      draggingIdxRef.current = null;
      setActiveDragIdx(null);
      updateAxisLockMode(null);
      pointAxisLockRef.current = null;
      applyDragVisualMode(null);
      setDragBadge(null);
      const sorted = sortPointsByTime(pointsRef.current);
      pointsRef.current = sorted;
      onUpdateSpeedPoints(sorted);
      commitBatch();
    };
    window.addEventListener('mousemove', mm);
    window.addEventListener('mouseup', mu);
  };

  const startDraggingSegment = (
    activeIndices: number[],
    fixedTimes: number[],
    startClientY: number,
    rect: DOMRect,
    startSpeed: number,
    initialPoints: SpeedPoint[],
  ) => {
    pointsRef.current = initialPoints;
    const startSpeedY = speedToY(startSpeed);
    setIsSegmentDragActive(true);
    setActiveSegmentIndices([
      activeIndices[0],
      activeIndices[activeIndices.length - 1],
    ]);
    applyDragVisualMode('vertical');

    const mm = (me: MouseEvent) => {
      const dy = me.clientY - startClientY;

      let newY = startSpeedY + (dy * 0.15) / rect.height;
      newY = Math.max(0, Math.min(1, newY));

      let v = yToSpeed(newY);
      v = Math.max(0.1, Math.min(16, v));

      const next = [...pointsRef.current];
      activeIndices.forEach((index, activeIndex) => {
        const point = next[index];
        if (!point) return;
        next[index] = {
          time: fixedTimes[activeIndex] ?? point.time,
          speed: v,
        };
      });
      pointsRef.current = next;
      onUpdateSpeedPoints(next);
      setDragBadge({
        x: me.clientX,
        y: me.clientY - 40,
        speed: v,
      });
    };

    const mu = () => {
      window.removeEventListener('mousemove', mm);
      window.removeEventListener('mouseup', mu);
      setIsSegmentDragActive(false);
      setActiveSegmentIndices(null);
      applyDragVisualMode(null);
      setDragBadge(null);
      const sorted = sortPointsByTime(pointsRef.current);
      pointsRef.current = sorted;
      onUpdateSpeedPoints(sorted);
      commitBatch();
    };
    window.addEventListener('mousemove', mm);
    window.addEventListener('mouseup', mu);
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
        getValue: (point) => point.speed,
        createPoint: (pointTime, speed) => ({ time: pointTime, speed }),
      });
      if (!plan) return;

      beginBatch();
      pointsRef.current = plan.points;
      onUpdateSpeedPoints(plan.points);
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

    let nextPoints = [...points];
    beginBatch();

    const expectedV = getCosineInterpolatedValueAtTime({
      points: nextPoints,
      time,
      getValue: (point) => point.speed,
    });

    const point = { time, speed: expectedV };
    nextPoints.push(point);
    nextPoints = sortPointsByTime(nextPoints);
    const activeIdx = nextPoints.indexOf(point);
    pointsRef.current = nextPoints;
    onUpdateSpeedPoints(nextPoints);

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
      <div
        className="speed-track timeline-lane timeline-lane-strong relative h-10"
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
              onPointerDown={(e) => handlePointPointerDown(e, i)}
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
          {dragBadge.speed.toFixed(2)}x
        </div>
      )}
    </>
  );
};
