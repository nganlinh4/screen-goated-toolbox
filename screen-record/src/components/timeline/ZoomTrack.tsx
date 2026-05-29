import React, { useState, useRef } from 'react';
import { VideoSegment, ZoomBlock } from '@/types/video';
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

const ZOOM_TRACK_TOP_PX = 4;
const ZOOM_TRACK_RANGE_PX = 32;
const ZOOM_TRACK_VIEWBOX_HEIGHT = 40;

function valueToTrackY(value: number) {
  return ZOOM_TRACK_TOP_PX + (1 - value) * ZOOM_TRACK_RANGE_PX;
}

function valueToTrackYPercent(value: number) {
  return `${(valueToTrackY(value) / ZOOM_TRACK_VIEWBOX_HEIGHT) * 100}%`;
}

// Smallest block we allow when resizing/creating.
const MIN_BLOCK_WIDTH_SEC = 0.2;

interface ZoomTrackProps {
  segment: VideoSegment;
  duration: number;
  editingKeyframeId: number | null;
  onKeyframeClick: (time: number, index: number) => void;
  onKeyframeDragStart: (index: number) => void;
  onUpdateInfluencePoints: (points: { time: number; value: number }[]) => void;
  onUpdateBlocks: (blocks: ZoomBlock[]) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export const ZoomTrack: React.FC<ZoomTrackProps> = ({
  segment,
  duration,
  editingKeyframeId,
  onKeyframeClick,
  onKeyframeDragStart,
  onUpdateInfluencePoints,
  onUpdateBlocks,
  beginBatch,
  commitBatch,
}) => {
  const blocks = segment.zoomBlocks ?? [];
  const hasInfluenceCurve = segment.smoothMotionPath && segment.smoothMotionPath.length > 0;
  const points = segment.zoomInfluencePoints || [];
  const draggingIdxRef = useRef<number | null>(null);
  const pointsRef = useRef(points);
  pointsRef.current = points;
  const segmentRef = useRef(segment);
  segmentRef.current = segment;
  const callbacksRef = useRef({ onUpdateBlocks });
  callbacksRef.current = { onUpdateBlocks };
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);
  const [hoveredBlockIdx, setHoveredBlockIdx] = useState<number | null>(null);
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

  React.useEffect(() => {
    return subscribeToAdjustableLineDragVisualMode(setGlobalDragVisualMode);
  }, []);

  React.useEffect(() => {
    if (globalDragVisualMode === null) return;
    setHoveredIdx(null);
    setHoveredBlockIdx(null);
    setHoveredSegmentIndices(null);
  }, [globalDragVisualMode]);

  const getHighlightedSegmentPath = (
    segmentIndices: AdjacentSegmentIndices | null,
  ) => {
    if (!segmentIndices) return '';

    const sorted = sortPointsByTime(points);
    const [leftIdx, rightIdx] = segmentIndices;
    const left = sorted[leftIdx];
    const right = sorted[rightIdx];
    if (!left || !right || right.time <= left.time || !isFinite(duration) || duration <= 0) return '';

    const toX = (time: number) => safeNum((time / duration) * 100);
    const toY = (value: number) => valueToTrackY(value);
    const x1 = toX(left.time);
    const y1 = toY(left.value);
    const x2 = toX(right.time);
    const y2 = toY(right.value);
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
    if (!left || !right || right.time <= left.time || !isFinite(duration) || duration <= 0) return '';

    const toX = (time: number) => safeNum((time / duration) * 100);
    const toY = (value: number) => valueToTrackY(value);
    const x1 = toX(left.time);
    const y1 = toY(left.value);
    const x2 = toX(right.time);
    const y2 = toY(right.value);
    const dx = x2 - x1;
    return `M ${x1} 40 L ${x1} ${y1} C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} L ${x2} 40 Z`;
  };

  const startDraggingInfluencePoint = (
    activeIdx: number,
    startClientX: number,
    startClientY: number,
    rect: DOMRect,
    initialPoints: { time: number; value: number }[],
  ) => {
    draggingIdxRef.current = activeIdx;
    pointsRef.current = initialPoints;
    const activePoint = initialPoints[activeIdx];
    if (!activePoint) return;
    const startTime = activePoint.time;
    const startValue = activePoint.value;
    setActiveSegmentIndices(null);
    setActiveDragIdx(activeIdx);
    updateAxisLockMode(null);
    pointAxisLockRef.current = null;
    applyDragVisualMode('free');

    const mm = (me: MouseEvent) => {
      if (draggingIdxRef.current === null) return;

      const mx = me.clientX - rect.left;
      const my = me.clientY - rect.top;
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
      if (draggingIdxRef.current === 0) t = 0;
      if (
        draggingIdxRef.current === pointsRef.current.length - 1 &&
        pointsRef.current.length > 1
      ) {
        t = duration;
      }

      let v = 1 - (my - 4) / 32;
      v = Math.max(0, Math.min(1, v));

      if (lockMode === 'horizontal') v = startValue;
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
        next[draggingIdxRef.current] = { time: t, value: v };
        pointsRef.current = next;
        onUpdateInfluencePoints(next);
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
      const sorted = sortPointsByTime(pointsRef.current);
      pointsRef.current = sorted;
      onUpdateInfluencePoints(sorted);
      commitBatch();
    };

    window.addEventListener('mousemove', mm);
    window.addEventListener('mouseup', mu);
  };

  const startDraggingInfluenceSegment = ({
    activeIndices,
    fixedTimes,
    startClientY,
    rect,
    startValue,
    initialPoints,
  }: {
    activeIndices: number[];
    fixedTimes: number[];
    startClientY: number;
    rect: DOMRect;
    startValue: number;
    initialPoints: { time: number; value: number }[];
  }) => {
    pointsRef.current = initialPoints;
    const valueRangePx = Math.max(1, rect.height - 8);
    const startValueY = 1 - startValue;
    setIsSegmentDragActive(true);
    setActiveSegmentIndices([
      activeIndices[0],
      activeIndices[activeIndices.length - 1],
    ]);
    applyDragVisualMode('vertical');

    const mm = (me: MouseEvent) => {
      const dy = me.clientY - startClientY;
      let newY = startValueY + dy / valueRangePx;
      newY = Math.max(0, Math.min(1, newY));
      const v = 1 - newY;

      const next = [...pointsRef.current];
      activeIndices.forEach((index, activeIndex) => {
        const point = next[index];
        if (!point) return;
        next[index] = {
          time: fixedTimes[activeIndex] ?? point.time,
          value: v,
        };
      });
      pointsRef.current = next;
      onUpdateInfluencePoints(next);
    };

    const mu = () => {
      window.removeEventListener('mousemove', mm);
      window.removeEventListener('mouseup', mu);
      setIsSegmentDragActive(false);
      setActiveSegmentIndices(null);
      applyDragVisualMode(null);
      const sorted = sortPointsByTime(pointsRef.current);
      pointsRef.current = sorted;
      onUpdateInfluencePoints(sorted);
      commitBatch();
    };

    window.addEventListener('mousemove', mm);
    window.addEventListener('mouseup', mu);
  };

  // Resize one edge of a zoom block with self-contained window listeners
  // (mirrors the influence-point drag pattern — independent of the timeline
  // drag coordinator, which only owns whole-block body moves).
  const startResizeBlock = (
    index: number,
    edge: 'start' | 'end',
    rect: DOMRect,
  ) => {
    beginBatch();
    const onMove = (me: MouseEvent) => {
      const current = segmentRef.current.zoomBlocks ?? [];
      const block = current[index];
      if (!block || rect.width <= 0 || duration <= 0) return;
      const t = Math.max(0, Math.min(duration, ((me.clientX - rect.left) / rect.width) * duration));
      const prev = index > 0 ? current[index - 1] : null;
      const next = index < current.length - 1 ? current[index + 1] : null;

      let updated: ZoomBlock;
      if (edge === 'start') {
        const lower = prev ? prev.endTime + MIN_BLOCK_WIDTH_SEC : 0;
        const newStart = Math.max(lower, Math.min(block.endTime - MIN_BLOCK_WIDTH_SEC, t));
        const span = block.endTime - newStart;
        updated = { ...block, startTime: newStart, easeIn: Math.min(block.easeIn, span) };
      } else {
        const upper = next ? next.startTime - MIN_BLOCK_WIDTH_SEC : duration;
        const newEnd = Math.min(upper, Math.max(block.startTime + MIN_BLOCK_WIDTH_SEC, t));
        const span = newEnd - block.startTime;
        updated = { ...block, endTime: newEnd, easeOut: Math.min(block.easeOut, span) };
      }
      callbacksRef.current.onUpdateBlocks(current.map((b, i) => (i === index ? updated : b)));
    };
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      commitBatch();
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  };

  // Handle point deletion
  React.useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.key === 'Delete' || e.key === 'Backspace') && hoveredIdx !== null) {
        if (hoveredIdx === 0 || hoveredIdx === points.length - 1) {
          if (points.length === 2) onUpdateInfluencePoints([]);
          return;
        }
        const newPoints = [...points];
        newPoints.splice(hoveredIdx, 1);
        onUpdateInfluencePoints(newPoints);
        setHoveredIdx(null);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [hoveredIdx, points, onUpdateInfluencePoints]);

  React.useEffect(() => {
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

  // Generate SVG path for influence curve
  const safeNum = (n: number, fallback = 0) => isFinite(n) ? n : fallback;
  const generatePath = () => {
    if (points.length === 0 || !isFinite(duration) || duration <= 0) return 'M 0 20 L 100 20';
    const sorted = [...points].sort((a, b) => a.time - b.time);
    const toX = (time: number) => safeNum((time / duration) * 100);
    const toY = (value: number) => valueToTrackY(value);
    const x0 = toX(sorted[0].time);
    const y0 = toY(sorted[0].value);
    let d = `M 0 ${y0} `;
    if (x0 > 0) d += `L ${x0} ${y0} `;
    for (let i = 1; i < sorted.length; i++) {
      const p1 = sorted[i - 1];
      const p2 = sorted[i];
      const x1 = toX(p1.time);
      const y1 = toY(p1.value);
      const x2 = toX(p2.time);
      const y2 = toY(p2.value);
      const dx = x2 - x1;
      d += `C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} `;
    }
    const xLast = toX(sorted[sorted.length - 1].time);
    const yLast = toY(sorted[sorted.length - 1].value);
    if (xLast < 100) d += `L 100 ${yLast} `;
    return d;
  };

  // Generate fill path (area under curve)
  const generateFillPath = () => {
    if (points.length === 0 || !isFinite(duration) || duration <= 0) return '';
    const sorted = [...points].sort((a, b) => a.time - b.time);
    const toX = (time: number) => safeNum((time / duration) * 100);
    const toY = (value: number) => valueToTrackY(value);
    const x0 = toX(sorted[0].time);
    const y0 = toY(sorted[0].value);
    let d = `M 0 40 L ${x0} 40 L ${x0} ${y0} `;
    for (let i = 1; i < sorted.length; i++) {
      const p1 = sorted[i - 1];
      const p2 = sorted[i];
      const x1 = toX(p1.time);
      const y1 = toY(p1.value);
      const x2 = toX(p2.time);
      const y2 = toY(p2.value);
      const dx = x2 - x1;
      d += `C ${x1 + dx / 2} ${y1}, ${x2 - dx / 2} ${y2}, ${x2} ${y2} `;
    }
    const xLast = toX(sorted[sorted.length - 1].time);
    d += `L ${xLast} 40 L 100 40 Z`;
    return d;
  };

  const handleInfluencePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    if (rect.width <= 0 || duration <= 0) return;
    const clickX = e.clientX - rect.left;
    const clickY = e.clientY - rect.top;
    const time = (clickX / rect.width) * duration;
    const hitThresholdX = 14;
    const activeIdx = points.findIndex((p) => {
      const px = (p.time / duration) * rect.width;
      const py = valueToTrackY(p.value);
      return Math.abs(px - clickX) < hitThresholdX && Math.abs(py - clickY) < hitThresholdX;
    });

    if (activeIdx !== -1) e.stopPropagation();

    if (activeIdx !== -1) {
      beginBatch();
      startDraggingInfluencePoint(
        activeIdx,
        e.clientX,
        e.clientY,
        rect,
        pointsRef.current,
      );
      return;
    }

    let newPoints = [...points];
    if (newPoints.length === 0) {
      newPoints = [
        { time: 0, value: 1 },
        { time: duration, value: 1 },
      ];
    }

    const expectedV = getCosineInterpolatedValueAtTime({
      points: newPoints,
      time,
      getValue: (point) => point.value,
    });
    const expectedY = valueToTrackY(expectedV);
    if (Math.abs(clickY - expectedY) > 10 && newPoints.length > 0) return;

    e.stopPropagation();

    if (e.ctrlKey) {
      const plan = buildSegmentDragPlan({
        points: newPoints,
        time,
        duration,
        trackWidth: rect.width,
        getValue: (point) => point.value,
        createPoint: (pointTime, value) => ({ time: pointTime, value }),
      });
      if (!plan) return;

      beginBatch();
      pointsRef.current = plan.points;
      onUpdateInfluencePoints(plan.points);
      startDraggingInfluenceSegment({
        activeIndices: plan.activeIndices,
        fixedTimes: plan.activeIndices.map(
          (index) => plan.points[index]?.time ?? time,
        ),
        startClientY: e.clientY,
        rect,
        startValue: plan.startValue,
        initialPoints: plan.points,
      });
      return;
    }

    beginBatch();
    const point = { time, value: expectedV };
    newPoints.push(point);
    newPoints = sortPointsByTime(newPoints);
    const newIdx = newPoints.indexOf(point);
    pointsRef.current = newPoints;
    onUpdateInfluencePoints(newPoints);
    startDraggingInfluencePoint(
      newIdx,
      e.clientX,
      e.clientY,
      rect,
      newPoints,
    );
  };

  const handlePointPointerDown = (e: React.PointerEvent, i: number) => {
    e.stopPropagation();
    beginBatch();
    const rect = e.currentTarget.parentElement!.getBoundingClientRect();
    startDraggingInfluencePoint(
      i,
      e.clientX,
      e.clientY,
      rect,
      pointsRef.current,
    );
  };

  const handleInfluencePointerMove = (
    e: React.PointerEvent<HTMLDivElement>,
  ) => {
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
    <div
      className="zoom-track timeline-lane timeline-lane-strong relative h-7"
      onMouseLeave={() => setHoveredBlockIdx(null)}
    >
      {/* Influence curve layer */}
      {hasInfluenceCurve && (
        <>
          <div
            className="zoom-influence-curve-clip absolute inset-0 z-10 overflow-hidden pointer-events-none"
            style={{ borderRadius: "inherit" }}
          >
            <svg className="zoom-influence-curve h-full w-full overflow-hidden" preserveAspectRatio="none" viewBox="0 0 100 40">
              <line className="zoom-track-baseline zoom-track-baseline-top" x1="0" y1="4" x2="100" y2="4" stroke="color-mix(in srgb, var(--timeline-success-color) 18%, transparent)" vectorEffect="non-scaling-stroke" />
              <line className="zoom-track-baseline zoom-track-baseline-bottom" x1="0" y1="36" x2="100" y2="36" stroke="color-mix(in srgb, var(--timeline-success-color) 18%, transparent)" vectorEffect="non-scaling-stroke" />
              {points.length > 0 && (
                <path
                  className="zoom-track-fill-path"
                  d={generateFillPath()}
                  fill="color-mix(in srgb, var(--timeline-success-color) 12%, transparent)"
                />
              )}
              <path
                className="zoom-track-main-path"
                d={generatePath()}
                fill="none"
                stroke="var(--timeline-success-color)"
                strokeWidth="1.5"
                vectorEffect="non-scaling-stroke"
              />
              {highlightedSegmentFillPath && (
                <path
                  className="timeline-segment-highlight-fill"
                  d={highlightedSegmentFillPath}
                  fill="currentColor"
                  style={{ color: 'var(--timeline-success-color)' }}
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
                  style={{ color: 'var(--timeline-success-color)' }}
                />
              )}
            </svg>
          </div>
          <div
            className="zoom-influence-layer absolute inset-0 z-20 pointer-events-auto"
            onPointerDown={handleInfluencePointerDown}
            onPointerMove={handleInfluencePointerMove}
            onPointerLeave={() => {
              if (!isSegmentDragActive) setHoveredSegmentIndices(null);
            }}
          >
            {points.map((p, i) => (
              <div
                key={i}
                className={`zoom-influence-point timeline-control-point absolute -translate-x-1/2 -translate-y-1/2 cursor-pointer ${
                  hoveredIdx === i ? 'ring-2 ring-[var(--timeline-success-color)]/40' : 'hover:scale-110'
                }`}
                data-tone="zoom"
                data-state={
                  hoveredIdx === i || activeDragIdx === i ? "active" : "idle"
                }
                data-lock-mode={
                  activeDragIdx === i ? (axisLockMode ?? undefined) : undefined
                }
                style={{
                  left: `${(p.time / duration) * 100}%`,
                  top: valueToTrackYPercent(p.value),
                  color: 'var(--timeline-success-color)',
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
        </>
      )}

      {/* Zoom block bars layer — each block is a draggable/resizable region.
          z-40 sits above the influence curve so blocks are interactive while the
          curve stays editable in the gaps between them. */}
      <div className="zoom-blocks-layer absolute inset-0 z-40 pointer-events-none">
        {blocks.map((block, index) => {
          if (duration <= 0) return null;
          const active = editingKeyframeId === index;
          const disabled = block.enabled === false;
          const leftPct = (block.startTime / duration) * 100;
          const widthPct = Math.max(0, ((block.endTime - block.startTime) / duration) * 100);
          const fillOpacity = Math.min(0.5, 0.16 + (block.zoomFactor - 1) * 0.18);
          const center = (block.startTime + block.endTime) / 2;

          return (
            <div
              key={block.id}
              className={`zoom-block absolute inset-y-0.5 rounded-md pointer-events-auto cursor-grab group/block ${
                active ? 'ring-1 ring-white/80 shadow-[0_0_8px_rgba(59,130,246,0.45)]' : ''
              } ${disabled ? 'opacity-40' : ''}`}
              data-active={active ? 'true' : 'false'}
              data-disabled={disabled ? 'true' : 'false'}
              style={{
                left: `${leftPct}%`,
                width: `${widthPct}%`,
                background: disabled
                  ? 'repeating-linear-gradient(45deg, rgba(59,130,246,0.10) 0px, rgba(59,130,246,0.10) 4px, transparent 4px, transparent 8px)'
                  : `rgba(59, 130, 246, ${fillOpacity})`,
                border: '1px solid var(--timeline-zoom-color)',
              }}
              onMouseEnter={() => { if (globalDragVisualMode === null) setHoveredBlockIdx(index); }}
              onMouseLeave={() => setHoveredBlockIdx(null)}
              onClick={(e) => { e.stopPropagation(); onKeyframeClick(center, index); }}
              onPointerDown={(e) => { e.stopPropagation(); onKeyframeDragStart(index); }}
            >
              {/* Zoom % label */}
              <div
                className="zoom-block-label timeline-chip absolute left-1/2 -translate-x-1/2 top-1/2 -translate-y-1/2 px-1.5 py-0.5 text-[9px] font-medium whitespace-nowrap pointer-events-none"
                data-tone="accent"
                data-active={active ? 'true' : 'false'}
              >
                {Math.round((block.zoomFactor - 1) * 100)}%
              </div>
              {/* Left resize handle */}
              <div
                className="zoom-block-handle-left absolute inset-y-0 left-0 w-2 -ml-1 cursor-col-resize z-10"
                onClick={(e) => e.stopPropagation()}
                onPointerDown={(e) => {
                  e.stopPropagation();
                  startResizeBlock(index, 'start', e.currentTarget.parentElement!.parentElement!.getBoundingClientRect());
                }}
              >
                <div className={`absolute inset-y-1 left-1/2 -translate-x-1/2 w-0.5 transition-colors ${
                  hoveredBlockIdx === index ? 'bg-[var(--timeline-zoom-color)]' : 'bg-[var(--timeline-zoom-color)]/50'
                }`} />
              </div>
              {/* Right resize handle */}
              <div
                className="zoom-block-handle-right absolute inset-y-0 right-0 w-2 -mr-1 cursor-col-resize z-10"
                onClick={(e) => e.stopPropagation()}
                onPointerDown={(e) => {
                  e.stopPropagation();
                  startResizeBlock(index, 'end', e.currentTarget.parentElement!.parentElement!.getBoundingClientRect());
                }}
              >
                <div className={`absolute inset-y-1 left-1/2 -translate-x-1/2 w-0.5 transition-colors ${
                  hoveredBlockIdx === index ? 'bg-[var(--timeline-zoom-color)]' : 'bg-[var(--timeline-zoom-color)]/50'
                }`} />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
};
