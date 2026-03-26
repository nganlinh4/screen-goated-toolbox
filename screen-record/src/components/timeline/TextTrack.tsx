import React, { useState } from 'react';
import { Scissors } from 'lucide-react';
import { VideoSegment, TextSegment } from '@/types/video';
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from "./trackHoverUtils";
import { useTrackRangeSelect } from './useTrackRangeSelect';

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
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
  onAddText?: (atTime?: number) => void;
  onDeleteTextSegments?: (ids: string[]) => void;
  onSelectionChange?: (ids: string[]) => void;
}

export const TextTrack: React.FC<TextTrackProps> = ({
  segment,
  duration,
  editingTextId,
  onTextClick,
  onTextSplit,
  onHandleDragStart,
  onAddText,
  onDeleteTextSegments,
  onSelectionChange,
}) => {
  const [hoverState, setHoverState] = useState<
    | { type: 'split'; x: number; time: number; seg: TextSegment }
    | { type: 'add'; x: number }
    | null
  >(null);

  const safeDuration = Math.max(duration, 0.001);
  const texts = segment.textSegments ?? [];

  const {
    selectedIds, rangeSelect, trackRef, isDraggingRange,
    onSegmentPointerDown,
    handleTrackPointerDown, handleTrackPointerMove, handleTrackPointerUp,
  } = useTrackRangeSelect(texts, duration, onSelectionChange, onDeleteTextSegments);

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (isDraggingRange.current) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const thresholdTime = getHandlePriorityThresholdTime(safeDuration, rect.width);

    const containing = texts.find(seg => time >= seg.startTime && time <= seg.endTime);
    if (containing) {
      const canSplit = onTextSplit && time > containing.startTime + 0.15 && time < containing.endTime - 0.15;
      setHoverState(canSplit ? { type: 'split', x, time, seg: containing } : null);
      return;
    }
    if (isTimeNearRangeBoundary(time, texts, thresholdTime)) {
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
      {texts.map((text) => (
        <div
          key={text.id}
          onPointerDown={(e) => {
            e.stopPropagation();
            onSegmentPointerDown();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const clickX = e.clientX - rect.left;
            const clickTime = (clickX / rect.width) * safeDuration;
            onHandleDragStart(text.id, 'body', clickTime - text.startTime);
          }}
          onClick={(e) => { e.stopPropagation(); onTextClick(text.id); }}
          className="text-segment timeline-block absolute h-full cursor-move group"
          data-tone="accent"
          data-active={editingTextId === text.id ? "true" : "false"}
          data-selected={selectedIds.has(text.id) ? "true" : undefined}
          style={{
            left: `${(text.startTime / safeDuration) * 100}%`,
            width: `${((text.endTime - text.startTime) / safeDuration) * 100}%`,
          }}
        >
          <div className="text-segment-content absolute inset-0 flex items-center justify-center overflow-hidden px-1">
            <span className="truncate text-[10px] text-[var(--on-surface)]"
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
      ))}

      {rangeSelect && rangeWidth > 2 && (
        <div className="text-range-select timeline-range-select absolute pointer-events-none z-5"
          style={{ left: rangeLeft, width: rangeWidth }} />
      )}

      {hoverState && hoverState.type === 'split' && !isDraggingRange.current && (
        <button className="text-split-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center"
          data-tone="accent" style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => { e.stopPropagation(); onTextSplit?.(hoverState.seg.id, hoverState.time); setHoverState(null); }}>
          <Scissors className="w-2 h-2" />
        </button>
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
