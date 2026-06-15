import React, { useState, useRef } from 'react';
import { VideoSegment, ZoomBlock } from '@/types/video';
import {
  buildSegmentDragPlan,
  getCosineInterpolatedValueAtTime,
  sortPointsByTime,
} from './adjustableLineUtils';
import { useAdjustableLineTrack } from './useAdjustableLineTrack';
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

type ZoomInfluencePoint = { time: number; value: number };

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
  const [hoveredBlockIdx, setHoveredBlockIdx] = useState<number | null>(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const [trackWidth, setTrackWidth] = useState(0);
  const { startResizeBlock, startResizeTransition } =
    useZoomBlockRangeEditing({
      beginBatch,
      blocks,
      commitBatch,
      duration,
      onUpdateBlocks,
    });

  // This lane keeps its own ctrl-aware hit-test (`handleInfluencePointerDown`),
  // renders no drag badge, and has a bespoke Delete handler (collapses a
  // 2-point curve to empty), so those pieces stay inline and drive the shared
  // controller's drag primitives. The point-drag value is an absolute-pointer-Y
  // mapping `1 - (clientY - rect.top - 4) / 32`, and the segment drag uses a
  // `rect.height - 8` range with a `1 - value` baseline — both unchanged.
  const {
    hoveredIdx,
    setHoveredIdx,
    activeDragIdx,
    axisLockMode,
    isSegmentDragActive,
    globalDragVisualMode,
    highlightedSegmentIndices,
    startDraggingPoint,
    startDraggingSegment,
    handlePointPointerDown,
    handleTrackPointerMove,
    setHoveredSegmentIndices,
  } = useAdjustableLineTrack<ZoomInfluencePoint>({
    points,
    duration,
    onUpdatePoints: onUpdateInfluencePoints,
    getValue: (point) => point.value,
    createPoint: (time, value) => ({ time, value }),
    resolvePointValue: ({ rect, event }) => {
      const my = event.clientY - rect.top;
      return Math.max(0, Math.min(1, 1 - (my - 4) / 32));
    },
    resolveSegmentValue: ({ dy, startValue, rect }) => {
      const valueRangePx = Math.max(1, rect.height - 8);
      const startValueY = 1 - startValue;
      const newY = Math.max(0, Math.min(1, startValueY + dy / valueRangePx));
      return 1 - newY;
    },
    axisLockEnabled: true,
    beginBatch,
    commitBatch,
    disableDeleteHandler: true,
    onGlobalDragActive: () => setHoveredBlockIdx(null),
  });

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

  // Bespoke point deletion: deleting an endpoint of a 2-point curve clears it
  // entirely; otherwise the hovered interior point is removed.
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
  }, [hoveredIdx, points, onUpdateInfluencePoints, setHoveredIdx]);

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
      startDraggingPoint(activeIdx, e.clientX, e.clientY, rect, points);
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
      onUpdateInfluencePoints(plan.points);
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

    beginBatch();
    const point = { time, value: expectedV };
    newPoints.push(point);
    newPoints = sortPointsByTime(newPoints);
    const newIdx = newPoints.indexOf(point);
    onUpdateInfluencePoints(newPoints);
    startDraggingPoint(newIdx, e.clientX, e.clientY, rect, newPoints);
  };

  const onInfluencePointPointerDown = (e: React.PointerEvent, i: number) => {
    e.stopPropagation();
    const rect = e.currentTarget.parentElement!.getBoundingClientRect();
    handlePointPointerDown(rect, e.clientX, e.clientY, i);
  };

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
            onPointerMove={handleTrackPointerMove}
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
                onPointerDown={(e) => onInfluencePointPointerDown(e, i)}
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
