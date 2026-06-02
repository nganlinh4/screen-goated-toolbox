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
import { useZoomBlockRangeEditing } from './useZoomBlockRangeEditing';
import { ZoomBlockLayer } from './ZoomBlockLayer';
import {
  generateZoomFillPath,
  generateZoomPath,
  getHighlightedSegmentFillPath,
  getHighlightedSegmentPath,
  valueToTrackY,
  valueToTrackYPercent,
} from './zoomTrackMath';

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
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);
  const [hoveredBlockIdx, setHoveredBlockIdx] = useState<number | null>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const [trackWidth, setTrackWidth] = useState(0);
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
  const { startResizeBlock, startResizeTransition } =
    useZoomBlockRangeEditing({
      beginBatch,
      blocks,
      commitBatch,
      duration,
      onUpdateBlocks,
    });

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

  // Track the lane's pixel width so we can hide the inline badge on blocks that
  // are too narrow to fit it (then it floats above on hover instead).
  React.useEffect(() => {
    const el = trackRef.current;
    if (!el) return;
    const update = () => setTrackWidth(el.getBoundingClientRect().width);
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

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
  const highlightedSegmentPath = getHighlightedSegmentPath({
    points,
    duration,
    segmentIndices: highlightedSegmentIndices,
  });
  const highlightedSegmentFillPath = getHighlightedSegmentFillPath({
    points,
    duration,
    segmentIndices: highlightedSegmentIndices,
  });

  return (
    <div
      ref={trackRef}
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
                  d={generateZoomFillPath(points, duration)}
                  fill="color-mix(in srgb, var(--timeline-success-color) 12%, transparent)"
                />
              )}
              <path
                className="zoom-track-main-path"
                d={generateZoomPath(points, duration)}
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
      <ZoomBlockLayer
        blocks={blocks}
        duration={duration}
        editingKeyframeId={editingKeyframeId}
        globalDragVisualMode={globalDragVisualMode}
        hoveredBlockIdx={hoveredBlockIdx}
        onHoverBlock={setHoveredBlockIdx}
        onKeyframeClick={onKeyframeClick}
        onKeyframeDragStart={onKeyframeDragStart}
        startResizeBlock={startResizeBlock}
        startResizeTransition={startResizeTransition}
        trackRef={trackRef}
        trackWidth={trackWidth}
      />
    </div>
  );
};
