import React, { useMemo, useRef, useState } from 'react';
import type { CSSProperties } from 'react';
import { Scissors } from 'lucide-react';
import { VideoSegment, TextSegment } from '@/types/video';
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from "./trackHoverUtils";
import { buildTextSplitPreview } from '@/lib/textSplitPreview';
import { useTrackRangeSelect } from './useTrackRangeSelect';
import type { TimelineVisibleRange } from './SegmentBlocksCanvas';
import { buildTimelineRenderWindow } from './timelineSegmentIndex';
import { countFrontendRender } from '@/lib/frontendPerfDiagnostics';

const DENSE_TEXT_COUNT = 260;
const MIN_INTERACTIVE_TEXT_PX = 7;

function rangesOverlap(
  a: { startTime: number; endTime: number },
  b: { startTime: number; endTime: number },
) {
  return a.startTime < b.endTime && b.startTime < a.endTime;
}

function getOverlapRange(
  segment: { startTime: number; endTime: number },
  elevated: { startTime: number; endTime: number } | null,
) {
  if (!elevated || !rangesOverlap(segment, elevated)) return null;
  const start = Math.max(segment.startTime, elevated.startTime);
  const end = Math.min(segment.endTime, elevated.endTime);
  const duration = Math.max(segment.endTime - segment.startTime, 0.0001);
  return {
    startPct: ((start - segment.startTime) / duration) * 100,
    endPct: ((end - segment.startTime) / duration) * 100,
  };
}

function buildContentMaskStyle(
  ranges: Array<{ startPct: number; endPct: number }>,
): CSSProperties | undefined {
  if (ranges.length === 0) return undefined;
  const merged = [...ranges]
    .sort((a, b) => a.startPct - b.startPct)
    .reduce<Array<{ startPct: number; endPct: number }>>((acc, range) => {
      const startPct = Math.max(0, Math.min(100, range.startPct));
      const endPct = Math.max(startPct, Math.min(100, range.endPct));
      const last = acc[acc.length - 1];
      if (last && startPct <= last.endPct) {
        last.endPct = Math.max(last.endPct, endPct);
      } else if (endPct > startPct) {
        acc.push({ startPct, endPct });
      }
      return acc;
    }, []);
  if (merged.length === 0) return undefined;
  const stops: string[] = ["black 0%"];
  for (const range of merged) {
    stops.push(`black ${range.startPct}%`);
    stops.push(`transparent ${range.startPct}%`);
    stops.push(`transparent ${range.endPct}%`);
    stops.push(`black ${range.endPct}%`);
  }
  stops.push("black 100%");
  const maskImage = `linear-gradient(to right, ${stops.join(", ")})`;
  return { maskImage, WebkitMaskImage: maskImage };
}

function isSegmentStackedAbove(
  index: number,
  rank: number,
  otherIndex: number,
  otherRank: number,
) {
  return otherRank > rank || (otherRank === rank && otherIndex > index);
}

function buildFontVariationCSS(vars?: TextSegment['style']['fontVariations']): string | undefined {
  const parts: string[] = [];
  if (vars?.wdth !== undefined && vars.wdth !== 100) parts.push(`'wdth' ${vars.wdth}`);
  if (vars?.slnt !== undefined && vars.slnt !== 0) parts.push(`'slnt' ${vars.slnt}`);
  if (vars?.ROND !== undefined && vars.ROND !== 0) parts.push(`'ROND' ${vars.ROND}`);
  return parts.length > 0 ? parts.join(', ') : undefined;
}

interface TextTrackProps {
  segment: VideoSegment;
  duration: number;
  editingTextId: string | null;
  onTextClick: (id: string) => void;
  onTextSplit?: (id: string, splitTime: number) => void;
  onTextDuplicate?: (id: string) => void;
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
  onAddText?: (atTime?: number) => void;
  onDeleteTextSegments?: (ids: string[]) => void;
  onSelectionChange?: (ids: string[]) => void;
  clearSignal?: number;
  onEmptyClick?: (time: number) => void;
  canvasWidthPx?: number;
  visibleTimeRange?: TimelineVisibleRange | null;
}

export const TextTrack: React.FC<TextTrackProps> = ({
  segment,
  duration,
  editingTextId,
  onTextClick,
  onTextSplit,
  onTextDuplicate,
  onHandleDragStart,
  onAddText,
  onDeleteTextSegments,
  onSelectionChange,
  clearSignal,
  onEmptyClick,
  canvasWidthPx = 0,
  visibleTimeRange,
}) => {
  countFrontendRender('TextTrack');
  const [hoverState, setHoverState] = useState<
    | { type: 'split'; x: number; time: number; seg: TextSegment; preview: { leftText: string; rightText: string } | null }
    | { type: 'add'; x: number }
    | null
  >(null);
  const [frontTextId, setFrontTextId] = useState<string | null>(null);

  const safeDuration = Math.max(duration, 0.001);
  const texts = segment.textSegments ?? [];
  const denseMode = texts.length >= DENSE_TEXT_COUNT;
  const lastClickRef = useRef<{ id: string | null; time: number }>({ id: null, time: 0 });
  const DOUBLE_CLICK_MS = 350;

  const {
    selectedIds, rangeSelect, trackRef, isDraggingRange,
    onSegmentPointerDown,
    addSegmentSelection,
    handleTrackPointerDown, handleTrackPointerMove, handleTrackPointerUp,
  } = useTrackRangeSelect(
    texts,
    duration,
    onSelectionChange,
    undefined,
    onDeleteTextSegments,
    clearSignal,
    { onEmptyClick },
  );

  const renderWindow = useMemo(
    () => buildTimelineRenderWindow({
      segments: texts,
      duration,
      canvasWidthPx,
      visibleRange: visibleTimeRange,
      denseMode,
      selectedIds,
      activeIds: editingTextId ? new Set([editingTextId]) : undefined,
      minInteractivePx: MIN_INTERACTIVE_TEXT_PX,
    }),
    [canvasWidthPx, denseMode, duration, editingTextId, selectedIds, texts, visibleTimeRange],
  );

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (isDraggingRange.current) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const thresholdTime = getHandlePriorityThresholdTime(safeDuration, rect.width);

    const containing = renderWindow.index.hitTest(time);
    if (containing) {
      const preview = buildTextSplitPreview({
        text: containing.text,
        startTime: containing.startTime,
        endTime: containing.endTime,
        splitTime: time,
      });
      const canSplit = onTextSplit && preview && time > containing.startTime + 0.15 && time < containing.endTime - 0.15;
      setHoverState(
        canSplit
          ? {
              type: 'split',
              x,
              time,
              seg: containing,
              preview,
            }
          : null,
      );
      return;
    }
    if (isTimeNearRangeBoundary(time, renderWindow.canvasSegments, thresholdTime)) {
      setHoverState(null);
      return;
    }
    setHoverState({ type: 'add', x });
  };

  const rangeLeft = rangeSelect ? Math.min(rangeSelect.startX, rangeSelect.endX) : 0;
  const rangeWidth = rangeSelect ? Math.abs(rangeSelect.endX - rangeSelect.startX) : 0;

  return (
    <div
      ref={trackRef}
      className="text-track timeline-lane relative h-7"
      onMouseMove={handleMouseMove}
      onMouseLeave={() => { if (!isDraggingRange.current) setHoverState(null); }}
      onPointerDown={handleTrackPointerDown}
      onPointerMove={handleTrackPointerMove}
      onPointerUp={handleTrackPointerUp}
    >
      {renderWindow.domSegments.map((text, textIndex) => (
        (() => {
          const isFront = frontTextId === text.id || editingTextId === text.id;
          const isSelected = selectedIds.has(text.id);
          const stackRank = isFront ? 3 : isSelected ? 2 : 1;
          const hasCoveredSegmentUnderneath = renderWindow.domSegments.some((other, otherIndex) => {
            if (other.id === text.id || !rangesOverlap(text, other)) return false;
            const otherIsFront = frontTextId === other.id || editingTextId === other.id;
            const otherRank = otherIsFront ? 3 : selectedIds.has(other.id) ? 2 : 1;
            return !isSegmentStackedAbove(textIndex, stackRank, otherIndex, otherRank);
          });
          const overlapRanges = renderWindow.domSegments
            .map((other, otherIndex) => {
              if (other.id === text.id) return null;
              const otherIsFront = frontTextId === other.id || editingTextId === other.id;
              const otherRank = otherIsFront ? 3 : selectedIds.has(other.id) ? 2 : 1;
              if (!isSegmentStackedAbove(textIndex, stackRank, otherIndex, otherRank)) return null;
              return getOverlapRange(text, other);
            })
            .filter((range): range is NonNullable<ReturnType<typeof getOverlapRange>> => Boolean(range));
          const contentMaskStyle = buildContentMaskStyle(overlapRanges);
          return (
        <div
          key={text.id}
          onPointerDown={(e) => {
            if (e.shiftKey || e.ctrlKey) return;
            // Manual double-click detection on pointerdown — pointerdown
            // fires synchronously before any drag-state cascade, so it's
            // robust against the body-drag taking over the click chain.
            const now = performance.now();
            const last = lastClickRef.current;
            const isDouble =
              !!onTextDuplicate
              && last.id === text.id
              && now - last.time < DOUBLE_CLICK_MS;
            if (isDouble) {
              e.stopPropagation();
              e.preventDefault();
              lastClickRef.current = { id: null, time: 0 };
              onTextDuplicate?.(text.id);
              return;
            }
            lastClickRef.current = { id: text.id, time: now };
            setFrontTextId(text.id);
            const preserveGroupDrag = selectedIds.has(text.id) && selectedIds.size > 1;
            if (!preserveGroupDrag) {
              addSegmentSelection(text.id);
            }
            e.stopPropagation();
            onSegmentPointerDown();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const clickX = e.clientX - rect.left;
            const clickTime = (clickX / rect.width) * safeDuration;
            onHandleDragStart(text.id, 'body', clickTime - text.startTime);
          }}
          onClick={(e) => {
            e.stopPropagation();
            setFrontTextId(text.id);
            if (e.shiftKey) {
              addSegmentSelection(text.id, { shiftKey: true });
            }
            onTextClick(text.id);
          }}
          className="text-segment timeline-block absolute h-full cursor-move overflow-hidden group"
          data-tone="accent"
          data-active={editingTextId === text.id ? "true" : "false"}
          data-selected={selectedIds.has(text.id) ? "true" : undefined}
          style={{
            left: `${(text.startTime / safeDuration) * 100}%`,
            width: `${((text.endTime - text.startTime) / safeDuration) * 100}%`,
            zIndex: isFront ? 5 : isSelected ? 4 : 3,
            background: hasCoveredSegmentUnderneath
              ? "color-mix(in srgb, var(--timeline-zoom-color) 34%, transparent)"
              : undefined,
          }}
        >
          <div
            className="text-segment-content absolute inset-0 z-[1] flex min-w-0 items-center justify-center overflow-hidden px-1"
            style={contentMaskStyle}
          >
            <span className="min-w-0 max-w-full truncate text-[10px] text-[var(--on-surface)]"
              style={{
                fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif",
                fontWeight: text.style.fontVariations?.wght ?? 400,
                fontVariationSettings: buildFontVariationCSS(text.style.fontVariations),
              }}>
              {text.text}
            </span>
          </div>
          <div className="text-handle-start absolute inset-y-0 -left-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(text.id, 'start'); }}>
            <div className="text-handle-bar timeline-handle-pill" />
          </div>
          <div className="text-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(text.id, 'end'); }}>
            <div className="text-handle-bar timeline-handle-pill" />
          </div>
        </div>
          );
        })()
      ))}

      {rangeSelect && rangeWidth > 2 && (
        <div className="text-range-select timeline-range-select absolute pointer-events-none z-5"
          style={{ left: rangeLeft, width: rangeWidth }} />
      )}

      {hoverState && hoverState.type === 'split' && !isDraggingRange.current && (
        <div className="text-split-control absolute bottom-0 z-10 pointer-events-auto" style={{ left: hoverState.x - 8 }}>
          <div className="text-split-hover group/text-split relative">
            <div className="text-split-preview-chip timeline-chip absolute left-1/2 z-30 -translate-x-1/2 bottom-[calc(100%+6px)] px-2.5 py-1 text-[11px] font-semibold whitespace-nowrap pointer-events-none opacity-0 translate-y-1 transition-all duration-150 group-hover/text-split:opacity-100 group-hover/text-split:translate-y-0" data-tone="accent">
              <span>{hoverState.preview?.leftText ?? hoverState.seg.text}</span>
              <span className="mx-1 opacity-80">|</span>
              <span>{hoverState.preview?.rightText ?? hoverState.seg.text}</span>
            </div>
            <button className="text-split-btn timeline-arch-button flex items-center justify-center"
              data-tone="accent"
              onPointerDown={(e) => { e.stopPropagation(); onTextSplit?.(hoverState.seg.id, hoverState.time); setHoverState(null); }}>
              <Scissors className="w-2 h-2" />
            </button>
          </div>
        </div>
      )}
      {hoverState && hoverState.type === 'add' && onAddText && !isDraggingRange.current && (
        <button className="text-add-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center text-[8px] font-bold"
          data-tone="accent" style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const time = (hoverState.x / rect.width) * safeDuration;
            onAddText(time); setHoverState(null);
          }}>
          +
        </button>
      )}
    </div>
  );
};
