import React, { useState, useMemo } from 'react';
import { Keyboard, MousePointer2, Scissors } from '@/components/ui/MaterialIcon';
import type { VideoSegment } from '@/types/video';
import { clampVisibilitySegmentsToDuration } from '@/lib/cursorHiding';
import { useSettings } from '@/hooks/useSettings';
import {
  analyzeKeystrokeSegments,
  buildRawEventIndex,
  getRawEventRanges,
  getSegmentsForMode,
  getVisualActivityRanges,
} from './keystrokeTrackRanges';
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from "./trackHoverUtils";
import { useTrackRangeSelect } from './useTrackRangeSelect';
import { rangeLaneKeys, type TimelineKeyboardEdits } from './useTimelineKeyboardEdits';

interface KeystrokeTrackProps {
  segment: VideoSegment;
  duration: number;
  editingKeystrokeSegmentId: string | null;
  onKeystrokeClick: (id: string, splitTime: number) => void;
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
  keyboardEdits?: TimelineKeyboardEdits;
  onAddKeystrokeSegment?: (atTime?: number) => void;
  onKeystrokeHover?: (id: string | null) => void;
  onDeleteKeystrokeSegments?: (ids: string[]) => void;
  onSelectionChange?: (ids: string[]) => void;
  clearSignal?: number;
  onEmptyClick?: (time: number) => void;
}

export const KeystrokeTrack: React.FC<KeystrokeTrackProps> = ({
  segment,
  duration,
  editingKeystrokeSegmentId,
  onKeystrokeClick,
  onHandleDragStart,
  keyboardEdits,
  onAddKeystrokeSegment,
  onKeystrokeHover,
  onDeleteKeystrokeSegments,
  onSelectionChange,
  clearSignal,
  onEmptyClick,
}) => {
  const { t } = useSettings();
  const [hoverState, setHoverState] = useState<
    | { type: 'split'; x: number; time: number; segId: string }
    | { type: 'add'; x: number }
    | null
  >(null);
  const mode = segment.keystrokeMode ?? 'off';
  const safeDuration = Math.max(duration, 0.001);
  const modeSegments = getSegmentsForMode(segment);
  const segments = useMemo(
    () => clampVisibilitySegmentsToDuration(modeSegments, safeDuration),
    [modeSegments, safeDuration],
  );

  const {
    selectedIds, rangeSelect, trackRef, isDraggingRange,
    onSegmentPointerDown, addSegmentSelection,
    handleTrackPointerDown, handleTrackPointerMove, handleTrackPointerUp,
  } = useTrackRangeSelect(
    segments,
    duration,
    onSelectionChange,
    undefined,
    onDeleteKeystrokeSegments,
    clearSignal,
    { onEmptyClick },
  );
  const rawEventRanges = useMemo(
    () => getRawEventRanges(segment, safeDuration),
    [segment.keystrokeEvents, segment.keystrokeMode, segment.keystrokeDelaySec, safeDuration]
  );
  const visualActivityRanges = useMemo(
    () => getVisualActivityRanges(rawEventRanges, safeDuration),
    [rawEventRanges, safeDuration]
  );
  // Per-segment overlap analysis is recomputed only when segments or events change.
  const segmentAnalysis = useMemo(
    () => analyzeKeystrokeSegments(segments, visualActivityRanges, buildRawEventIndex(rawEventRanges)),
    [rawEventRanges, segments, visualActivityRanges],
  );

  const segmentKeys = rangeLaneKeys({
    lane: 'keystroke', name: t.trackKeystrokes, startLabel: t.timelineSegmentStart, endLabel: t.timelineSegmentEnd,
    duration: safeDuration, keyboardEdits, selectedIds, onSelect: (id) => addSegmentSelection(id),
    onDelete: onDeleteKeystrokeSegments, onSplit: onKeystrokeClick, onAdd: onAddKeystrokeSegment,
  });

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (mode === 'off' || isDraggingRange.current) {
      setHoverState(null);
      return;
    }
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const thresholdTime = getHandlePriorityThresholdTime(safeDuration, rect.width);

    const containing = segments.find(
      (seg) => time >= seg.startTime && time <= seg.endTime
    );
    if (containing) {
      const canSplit = time > containing.startTime + 0.15 && time < containing.endTime - 0.15;
      setHoverState(canSplit ? { type: 'split', x, time, segId: containing.id } : null);
      return;
    }
    if (isTimeNearRangeBoundary(time, segments, thresholdTime)) {
      setHoverState(null);
      return;
    }
    setHoverState({ type: 'add', x });
  };

  return (
    <div
      className="keystroke-track timeline-lane relative h-7"
      title={t.trackKeystrokes}
      aria-label={t.trackKeystrokes}
      ref={trackRef}
      onMouseMove={handleMouseMove}
      onMouseLeave={() => { if (!isDraggingRange.current) setHoverState(null); }}
      onPointerDown={handleTrackPointerDown}
      onPointerMove={handleTrackPointerMove}
      onPointerUp={handleTrackPointerUp}
    >
      {mode !== 'off' && visualActivityRanges.map((range, idx) => (
        <div
          key={`keystroke-activity-${idx}`}
          className="keystroke-activity-range absolute inset-y-0 rounded-xs pointer-events-none"
          style={{
            left: `${(range.startTime / safeDuration) * 100}%`,
            width: `${((range.endTime - range.startTime) / safeDuration) * 100}%`,
            backgroundColor: "color-mix(in srgb, var(--timeline-success-color) 18%, transparent)",
          }}
        />
      ))}

      {segments.map((seg, segIndex) => (
        (() => {
          const { indicatorRanges, emptyRanges, kinds: segmentKinds } = segmentAnalysis[segIndex];
          const hasIndicators = indicatorRanges.length > 0;
          const segmentDuration = Math.max(seg.endTime - seg.startTime, 0.001);
          const segmentIcon = (() => {
            if (!hasIndicators) return 'none' as const;
            if (mode === 'keyboard') return 'keyboard' as const;
            if (segmentKinds.hasKeyboard && segmentKinds.hasMouse) return 'both' as const;
            if (segmentKinds.hasMouse) return 'mouse' as const;
            return 'keyboard' as const;
          })();

          const keys = segmentKeys(seg, segIndex);
          return (
            <div
              key={seg.id}
              {...keys.body}
              onPointerDown={(e) => {
                e.stopPropagation();
                onSegmentPointerDown();
                const rect = e.currentTarget.parentElement!.getBoundingClientRect();
                const clickX = e.clientX - rect.left;
                const clickTime = (clickX / rect.width) * safeDuration;
                onHandleDragStart(seg.id, 'body', clickTime - seg.startTime);
              }}
              onMouseEnter={() => onKeystrokeHover?.(seg.id)}
              onMouseLeave={() => onKeystrokeHover?.(null)}
              className="keystroke-segment timeline-block absolute h-full cursor-move group"
              data-tone={hasIndicators ? "success" : "neutral"}
              data-style={hasIndicators ? "filled" : "quiet"}
              data-active={editingKeystrokeSegmentId === seg.id ? "true" : "false"}
              data-selected={selectedIds.has(seg.id) ? "true" : undefined}
              style={{
                left: `${(seg.startTime / safeDuration) * 100}%`,
                width: `${((seg.endTime - seg.startTime) / safeDuration) * 100}%`,
                zIndex: 2,
              }}
            >
              {emptyRanges.map((range, idx) => (
                <div
                  key={`${seg.id}-empty-${idx}`}
                  className="keystroke-empty-window absolute inset-y-0 rounded-xs pointer-events-none"
                  style={{
                    left: `${((range.startTime - seg.startTime) / segmentDuration) * 100}%`,
                    width: `${((range.endTime - range.startTime) / segmentDuration) * 100}%`,
                    backgroundImage: 'repeating-linear-gradient(135deg, rgba(100,116,139,0.20) 0 5px, rgba(100,116,139,0.07) 5px 10px)',
                  }}
                />
              ))}
              {hasIndicators && indicatorRanges.map((range, idx) => (
                <div
                  key={`${seg.id}-indicator-${idx}`}
                  className="keystroke-indicator-window absolute inset-y-0 rounded-xs pointer-events-none"
                  style={{
                    left: `${((range.startTime - seg.startTime) / segmentDuration) * 100}%`,
                    width: `${((range.endTime - range.startTime) / segmentDuration) * 100}%`,
                    backgroundColor: "color-mix(in srgb, var(--timeline-success-color) 24%, transparent)",
                  }}
                />
              ))}
              <div className="keystroke-segment-content absolute inset-0 flex items-center justify-center overflow-hidden px-1 pointer-events-none">
                <div
                  className={`keystroke-segment-icon keystroke-segment-icon-${segmentIcon} flex items-center gap-[2px] ${
                    hasIndicators
                      ? 'text-[var(--timeline-success-color)]'
                      : 'text-[var(--timeline-neutral-color)]/80'
                  }`}
                >
                  {segmentIcon === 'none' && (
                    <span className="keystroke-segment-icon-empty text-[10px] leading-none">·</span>
                  )}
                  {(segmentIcon === 'keyboard' || segmentIcon === 'both') && (
                    <Keyboard className="keystroke-segment-icon-keyboard w-2.5 h-2.5" strokeWidth={2.2} />
                  )}
                  {(segmentIcon === 'mouse' || segmentIcon === 'both') && (
                    <MousePointer2 className="keystroke-segment-icon-mouse w-2.5 h-2.5" strokeWidth={2.2} />
                  )}
                </div>
              </div>
              <div
                className="keystroke-handle-start absolute inset-y-0 -left-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 z-10"
                {...keys.start}
                onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'start'); }}
              >
                <div
                  className="keystroke-handle-bar timeline-handle-pill"
                />
              </div>
              <div
                className="keystroke-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 z-10"
                {...keys.end}
                onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'end'); }}
              >
                <div
                  className="keystroke-handle-bar timeline-handle-pill"
                />
              </div>
            </div>
          );
        })()
      ))}

      {rangeSelect && Math.abs(rangeSelect.endX - rangeSelect.startX) > 2 && (
        <div className="keystroke-range-select timeline-range-select absolute pointer-events-none z-5"
          style={{ left: Math.min(rangeSelect.startX, rangeSelect.endX), width: Math.abs(rangeSelect.endX - rangeSelect.startX) }} />
      )}

      {mode !== 'off' && hoverState && hoverState.type === 'split' && !isDraggingRange.current && (
        <button
          className="keystroke-split-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center"
          data-tone="accent"
          style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            onKeystrokeClick(hoverState.segId, hoverState.time);
            setHoverState(null);
          }}
        >
          <Scissors className="w-2 h-2" />
        </button>
      )}
      {mode !== 'off' && hoverState && hoverState.type === 'add' && onAddKeystrokeSegment && !isDraggingRange.current && (
        <button
          className="keystroke-add-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center text-[8px] font-bold"
          data-tone="success"
          style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const time = (hoverState.x / rect.width) * safeDuration;
            onAddKeystrokeSegment(time);
            setHoverState(null);
          }}
        >
          +
        </button>
      )}
    </div>
  );
};
