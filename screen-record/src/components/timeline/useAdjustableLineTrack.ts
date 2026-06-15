import { useEffect, useRef, useState } from "react";
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

/**
 * Shared "adjustable line / control-point curve" drag controller used by the
 * audio volume, speed, zoom-influence and track-volume timeline lanes.
 *
 * This hook owns the invariant scaffolding that each of those lanes used to
 * hand-duplicate: the drag refs, the hover/active/badge/ctrl/segment state, the
 * three global drag-visual-mode lifecycle effects, the Delete/Backspace
 * point-removal effect, and the point/segment drag controllers plus their
 * pointer-down/move handlers.
 *
 * Each lane differs only along a handful of seams (the value field name, the
 * y<->value mapping, the per-move vertical math, the drag badge, whether
 * shift-axis-lock is enabled, the ctrl hit-test, and the batch/commit
 * callbacks). Those are threaded in through {@link AdjustableLineTrackParams}
 * so the lanes keep their exact current behaviour while sharing the controller.
 */

export type AxisLockMode = "armed" | "horizontal" | "vertical";

/** Per-move vertical resolution for a single dragged point. */
export interface PointMoveContext<TPoint> {
  /** Pointer delta from drag start, in client px. */
  dy: number;
  /** The point the drag started from (pre-drag snapshot). */
  startPoint: TPoint;
  /** The drag-target bounding rect captured at pointer-down. */
  rect: DOMRect;
  /** The raw MouseEvent for the current move (for absolute-Y mapping). */
  event: MouseEvent;
}

/** Per-move vertical resolution for a segment (plateau) drag. */
export interface SegmentMoveContext {
  /** Pointer delta from drag start, in client px. */
  dy: number;
  /** The interpolated value sampled at segment-drag start. */
  startValue: number;
  /** The drag-target bounding rect captured at pointer-down. */
  rect: DOMRect;
}

export interface DragBadge {
  x: number;
  y: number;
  value: number;
}

export interface AdjustableLineTrackParams<TPoint extends { time: number }> {
  /** Current control points (already in their effective/sorted form). */
  points: TPoint[];
  /** Curve x-domain in seconds. */
  duration: number;
  /** Push a new points array to the owner. */
  onUpdatePoints: (points: TPoint[]) => void;
  /** Read a point's scalar value (volume / speed / value). */
  getValue: (point: TPoint) => number;
  /** Build a point from a (time, value) pair. */
  createPoint: (time: number, value: number) => TPoint;
  /**
   * Optional clamp applied only to a freshly-inserted point's interpolated
   * value (volume lanes clamp here; the speed lane does not). Inner segment
   * handles and dragged writes are never re-clamped through this.
   */
  clampNewValue?: (value: number) => number;
  /**
   * Resolve the new value for a single dragged point. Implementations own their
   * own vertical mapping, sensitivity and clamping; the controller handles the
   * time axis (including endpoint pinning) and the axis-lock overrides.
   */
  resolvePointValue: (ctx: PointMoveContext<TPoint>) => number;
  /** Resolve the new value for every point in a segment drag. */
  resolveSegmentValue: (ctx: SegmentMoveContext) => number;
  /** Whether shift-drag axis locking is active for this lane. */
  axisLockEnabled: boolean;
  /**
   * Optional drag-badge factory. Returns the badge payload (or null to suppress
   * a badge for this lane, e.g. the zoom-influence lane). Called on every move
   * for both point and segment drags. Omit entirely to disable badges.
   */
  makeBadge?: (event: MouseEvent, value: number) => DragBadge | null;
  /** Optional begin-batch hook (called at pointer-down before mutating). */
  beginBatch?: () => void;
  /** Optional commit-batch hook (called on mouse-up). */
  commitBatch?: () => void;
  /** Optional extra commit callback (TrackVolumeCurve's onCommit). */
  onCommit?: () => void;
  /**
   * Set true to suppress the controller's built-in Delete/Backspace handler so
   * the lane can keep a bespoke one inline (the zoom lane collapses a 2-point
   * curve to empty and pins hover differently). Defaults to false.
   */
  disableDeleteHandler?: boolean;
  /** Extra hovered-state to clear whenever a global drag begins. */
  onGlobalDragActive?: () => void;
}

export interface AdjustableLineTrackController<TPoint extends { time: number }> {
  hoveredIdx: number | null;
  setHoveredIdx: (idx: number | null) => void;
  activeDragIdx: number | null;
  axisLockMode: AxisLockMode | null;
  dragBadge: DragBadge | null;
  isCtrlPressed: boolean;
  isSegmentDragActive: boolean;
  globalDragVisualMode: AdjustableLineDragVisualMode | null;
  highlightedSegmentIndices: AdjacentSegmentIndices | null;
  /** Begin dragging an existing point by index (control-point pointer-down). */
  startDraggingPoint: (
    activeIdx: number,
    startClientX: number,
    startClientY: number,
    rect: DOMRect,
    initialPoints: TPoint[],
  ) => void;
  /** Begin a segment (plateau) drag from a pre-built plan. */
  startDraggingSegment: (
    activeIndices: number[],
    fixedTimes: number[],
    startClientY: number,
    rect: DOMRect,
    startValue: number,
    initialPoints: TPoint[],
  ) => void;
  /**
   * Default track pointer-down: ctrl => segment drag, otherwise insert a point
   * at the click and drag it. Lanes with a bespoke hit-test (zoom) drive the
   * controller primitives directly instead of calling this.
   */
  handleTrackPointerDown: (
    e: React.PointerEvent<HTMLDivElement>,
    rect: DOMRect,
    time: number,
  ) => void;
  /** Control-point pointer-down: begin dragging that point. */
  handlePointPointerDown: (
    rect: DOMRect,
    clientX: number,
    clientY: number,
    idx: number,
  ) => void;
  /** Track pointer-move: update the hovered-segment highlight. */
  handleTrackPointerMove: (e: React.PointerEvent<HTMLDivElement>) => void;
  setHoveredSegmentIndices: (indices: AdjacentSegmentIndices | null) => void;
}

export function useAdjustableLineTrack<TPoint extends { time: number }>(
  params: AdjustableLineTrackParams<TPoint>,
): AdjustableLineTrackController<TPoint> {
  const {
    points,
    duration,
    onUpdatePoints,
    getValue,
    createPoint,
    clampNewValue,
    resolvePointValue,
    resolveSegmentValue,
    axisLockEnabled,
    makeBadge,
    beginBatch,
    commitBatch,
    onCommit,
    disableDeleteHandler,
    onGlobalDragActive,
  } = params;

  const draggingIdxRef = useRef<number | null>(null);
  const pointsRef = useRef(points);
  pointsRef.current = points;
  const [hoveredIdx, setHoveredIdx] = useState<number | null>(null);
  const [activeDragIdx, setActiveDragIdx] = useState<number | null>(null);
  const [dragBadge, setDragBadge] = useState<DragBadge | null>(null);
  const [isCtrlPressed, setIsCtrlPressed] = useState(false);
  const [axisLockMode, setAxisLockMode] = useState<AxisLockMode | null>(null);
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
  // Hold the latest extra-clear callback in a ref so the global-drag effect can
  // run once per `globalDragVisualMode` transition (matching each lane's prior
  // behaviour) without re-subscribing on every render.
  const onGlobalDragActiveRef = useRef(onGlobalDragActive);
  onGlobalDragActiveRef.current = onGlobalDragActive;

  const applyDragVisualMode = (mode: AdjustableLineDragVisualMode | null) => {
    if (dragVisualModeRef.current === mode) return;
    dragVisualModeRef.current = mode;
    setAdjustableLineDragVisualMode(mode);
  };

  const updateAxisLockMode = (mode: AxisLockMode | null) => {
    setAxisLockMode((current) => (current === mode ? current : mode));
  };

  useEffect(() => {
    return subscribeToAdjustableLineDragVisualMode(setGlobalDragVisualMode);
  }, []);

  useEffect(() => {
    if (globalDragVisualMode === null) return;
    setHoveredIdx(null);
    setHoveredSegmentIndices(null);
    onGlobalDragActiveRef.current?.();
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

  useEffect(() => {
    if (disableDeleteHandler) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Delete" && e.key !== "Backspace") return;
      if (hoveredIdx === null) return;
      const current = pointsRef.current;
      if (hoveredIdx === 0 || hoveredIdx === current.length - 1) return;
      const next = current.filter((_, i) => i !== hoveredIdx);
      pointsRef.current = next;
      onUpdatePoints(next);
      onCommit?.();
      setHoveredIdx(null);
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [hoveredIdx, onUpdatePoints, onCommit, disableDeleteHandler]);

  const startDraggingPoint = (
    activeIdx: number,
    startClientX: number,
    startClientY: number,
    rect: DOMRect,
    initialPoints: TPoint[],
  ) => {
    draggingIdxRef.current = activeIdx;
    pointsRef.current = initialPoints;
    const startPoint = initialPoints[activeIdx];
    if (!startPoint) return;
    const startTime = startPoint.time;
    const startValue = getValue(startPoint);
    setActiveSegmentIndices(null);
    setActiveDragIdx(activeIdx);
    if (axisLockEnabled) updateAxisLockMode(null);
    pointAxisLockRef.current = null;
    applyDragVisualMode("free");

    const mm = (me: MouseEvent) => {
      if (draggingIdxRef.current === null) return;

      const mx = me.clientX - rect.left;
      const dy = me.clientY - startClientY;
      const lockMode = axisLockEnabled
        ? me.shiftKey
          ? pointAxisLockRef.current ??
            (() => {
              const nextLockMode = getAxisLockMode(
                me.clientX - startClientX,
                me.clientY - startClientY,
              );
              if (
                nextLockMode === "horizontal" ||
                nextLockMode === "vertical"
              ) {
                pointAxisLockRef.current = nextLockMode;
              }
              return nextLockMode;
            })()
          : null
        : null;

      let t = (mx / rect.width) * duration;
      t = Math.max(0, Math.min(duration, t));

      let value = resolvePointValue({ dy, startPoint, rect, event: me });

      if (lockMode === "horizontal") value = startValue;
      if (lockMode === "vertical") t = startTime;

      if (axisLockEnabled) {
        updateAxisLockMode(lockMode);
        applyDragVisualMode(
          lockMode === null
            ? "free"
            : lockMode === "armed"
              ? "armed"
              : lockMode,
        );
        if (!me.shiftKey) pointAxisLockRef.current = null;
      } else {
        applyDragVisualMode("free");
      }

      const next = [...pointsRef.current];
      if (next[draggingIdxRef.current]) {
        if (draggingIdxRef.current === 0) t = 0;
        if (draggingIdxRef.current === next.length - 1 && next.length > 1) {
          t = duration;
        }
        next[draggingIdxRef.current] = createPoint(t, value);
        pointsRef.current = next;
        onUpdatePoints(next);
        if (makeBadge) {
          const badge = makeBadge(me, value);
          if (badge) setDragBadge(badge);
        }
      }
    };

    const mu = () => {
      window.removeEventListener("mousemove", mm);
      window.removeEventListener("mouseup", mu);
      draggingIdxRef.current = null;
      setActiveDragIdx(null);
      if (axisLockEnabled) updateAxisLockMode(null);
      pointAxisLockRef.current = null;
      applyDragVisualMode(null);
      if (makeBadge) setDragBadge(null);
      const sorted = sortPointsByTime(pointsRef.current);
      pointsRef.current = sorted;
      onUpdatePoints(sorted);
      commitBatch?.();
      onCommit?.();
    };

    window.addEventListener("mousemove", mm);
    window.addEventListener("mouseup", mu);
  };

  const startDraggingSegment = (
    activeIndices: number[],
    fixedTimes: number[],
    startClientY: number,
    rect: DOMRect,
    startValue: number,
    initialPoints: TPoint[],
  ) => {
    pointsRef.current = initialPoints;
    setIsSegmentDragActive(true);
    setActiveSegmentIndices([
      activeIndices[0],
      activeIndices[activeIndices.length - 1],
    ]);
    applyDragVisualMode("vertical");

    const mm = (me: MouseEvent) => {
      const dy = me.clientY - startClientY;
      const value = resolveSegmentValue({ dy, startValue, rect });

      const next = [...pointsRef.current];
      activeIndices.forEach((index, activeIndex) => {
        const point = next[index];
        if (!point) return;
        next[index] = createPoint(fixedTimes[activeIndex] ?? point.time, value);
      });
      pointsRef.current = next;
      onUpdatePoints(next);
      if (makeBadge) {
        const badge = makeBadge(me, value);
        if (badge) setDragBadge(badge);
      }
    };

    const mu = () => {
      window.removeEventListener("mousemove", mm);
      window.removeEventListener("mouseup", mu);
      setIsSegmentDragActive(false);
      setActiveSegmentIndices(null);
      applyDragVisualMode(null);
      if (makeBadge) setDragBadge(null);
      const sorted = sortPointsByTime(pointsRef.current);
      pointsRef.current = sorted;
      onUpdatePoints(sorted);
      commitBatch?.();
      onCommit?.();
    };

    window.addEventListener("mousemove", mm);
    window.addEventListener("mouseup", mu);
  };

  const handleTrackPointerDown = (
    e: React.PointerEvent<HTMLDivElement>,
    rect: DOMRect,
    time: number,
  ) => {
    if (e.ctrlKey) {
      const plan = buildSegmentDragPlan({
        points: pointsRef.current,
        time,
        duration,
        trackWidth: rect.width,
        getValue,
        createPoint,
      });
      if (!plan) return;

      beginBatch?.();
      pointsRef.current = plan.points;
      onUpdatePoints(plan.points);
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

    let nextPoints = [...pointsRef.current];
    beginBatch?.();

    const expected = getCosineInterpolatedValueAtTime({
      points: nextPoints,
      time,
      getValue,
    });

    const point = createPoint(
      time,
      clampNewValue ? clampNewValue(expected) : expected,
    );
    nextPoints.push(point);
    nextPoints = sortPointsByTime(nextPoints);
    const activeIdx = nextPoints.indexOf(point);
    pointsRef.current = nextPoints;
    onUpdatePoints(nextPoints);

    startDraggingPoint(activeIdx, e.clientX, e.clientY, rect, nextPoints);
  };

  const handlePointPointerDown = (
    rect: DOMRect,
    clientX: number,
    clientY: number,
    idx: number,
  ) => {
    beginBatch?.();
    startDraggingPoint(idx, clientX, clientY, rect, pointsRef.current);
  };

  const handleTrackPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (globalDragVisualMode !== null) {
      setHoveredSegmentIndices(null);
      return;
    }

    if (duration <= 0 || pointsRef.current.length < 2) {
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
      getAdjacentSegmentIndicesAtTime({
        points: pointsRef.current,
        time,
        duration,
      }),
    );
  };

  const highlightedSegmentIndices =
    activeSegmentIndices ??
    (globalDragVisualMode === null && isCtrlPressed
      ? hoveredSegmentIndices
      : null);

  return {
    hoveredIdx,
    setHoveredIdx,
    activeDragIdx,
    axisLockMode,
    dragBadge,
    isCtrlPressed,
    isSegmentDragActive,
    globalDragVisualMode,
    highlightedSegmentIndices,
    startDraggingPoint,
    startDraggingSegment,
    handleTrackPointerDown,
    handlePointPointerDown,
    handleTrackPointerMove,
    setHoveredSegmentIndices,
  };
}
