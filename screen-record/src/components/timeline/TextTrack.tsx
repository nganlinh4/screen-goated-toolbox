import React, { useState, useCallback, useEffect, useRef } from 'react';
import { Scissors } from 'lucide-react';
import { VideoSegment, TextSegment } from '@/types/video';
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from "./trackHoverUtils";

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
  onTextSelectionChange?: (ids: string[]) => void;
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
  onTextSelectionChange,
}) => {
  const [hoverState, setHoverState] = useState<
    | { type: 'split'; x: number; time: number; seg: TextSegment }
    | { type: 'add'; x: number }
    | null
  >(null);

  // Range-select drag state
  const [rangeSelect, setRangeSelect] = useState<{
    startX: number; endX: number; startTime: number; endTime: number;
  } | null>(null);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const trackRef = useRef<HTMLDivElement>(null);
  const isDraggingRange = useRef(false);

  const safeDuration = Math.max(duration, 0.001);
  const texts = segment.textSegments ?? [];

  // Notify parent when selection changes
  useEffect(() => {
    onTextSelectionChange?.(Array.from(selectedIds));
  }, [selectedIds, onTextSelectionChange]);

  const getSelectedInRange = useCallback((t1: number, t2: number) => {
    const lo = Math.min(t1, t2);
    const hi = Math.max(t1, t2);
    const ids = new Set<string>();
    for (const seg of texts) {
      if (seg.endTime > lo && seg.startTime < hi) ids.add(seg.id);
    }
    return ids;
  }, [texts]);

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (isDraggingRange.current) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const thresholdTime = getHandlePriorityThresholdTime(safeDuration, rect.width);

    const containing = texts.find(
      seg => time >= seg.startTime && time <= seg.endTime
    );
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

  // Start range-select drag on empty area
  const handleTrackPointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (e.target !== e.currentTarget) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const onSeg = texts.some(seg => time >= seg.startTime && time <= seg.endTime);
    if (onSeg) return;

    isDraggingRange.current = true;
    setHoverState(null);
    setRangeSelect({ startX: x, endX: x, startTime: time, endTime: time });
    setSelectedIds(new Set());
    e.currentTarget.setPointerCapture(e.pointerId);
  }, [safeDuration, texts]);

  const handleTrackPointerMove = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDraggingRange.current || !rangeSelect) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const time = (x / rect.width) * safeDuration;
    setRangeSelect(prev => prev ? { ...prev, endX: x, endTime: time } : null);
    setSelectedIds(getSelectedInRange(rangeSelect.startTime, time));
  }, [rangeSelect, safeDuration, getSelectedInRange]);

  const handleTrackPointerUp = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDraggingRange.current) return;
    isDraggingRange.current = false;
    e.currentTarget.releasePointerCapture(e.pointerId);
    if (rangeSelect && Math.abs(rangeSelect.endX - rangeSelect.startX) < 4) {
      setRangeSelect(null);
      setSelectedIds(new Set());
      return;
    }
    setRangeSelect(null);
  }, [rangeSelect]);

  // Delete key for selected segments
  useEffect(() => {
    if (selectedIds.size === 0) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.code === 'Delete' || e.code === 'Backspace') && selectedIds.size > 0) {
        const target = e.target as HTMLElement;
        if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) return;
        e.preventDefault();
        e.stopPropagation();
        onDeleteTextSegments?.(Array.from(selectedIds));
        setSelectedIds(new Set());
      }
      if (e.code === 'Escape') {
        setSelectedIds(new Set());
      }
    };
    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [selectedIds, onDeleteTextSegments]);

  // Clear selection when clicking another track's segment (not SidePanel controls).
  // Escape key handles intentional deselection.

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
            setSelectedIds(new Set());
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const clickX = e.clientX - rect.left;
            const clickTime = (clickX / rect.width) * safeDuration;
            onHandleDragStart(text.id, 'body', clickTime - text.startTime);
          }}
          onClick={(e) => {
            e.stopPropagation();
            onTextClick(text.id);
          }}
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
            <span
              className="truncate text-[10px] text-[var(--on-surface)]"
              style={{
                fontFamily: "'Google Sans Flex', 'Segoe UI', system-ui, sans-serif",
                fontWeight: text.style.fontVariations?.wght ?? 400,
                fontVariationSettings: buildFontVariationCSS(text.style.fontVariations),
              }}
            >
              {text.text}
            </span>
          </div>
          <div
            className="text-handle-start absolute inset-y-0 -left-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(text.id, 'start'); }}
          >
            <div className="text-handle-bar timeline-handle-pill" />
          </div>
          <div
            className="text-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(text.id, 'end'); }}
          >
            <div className="text-handle-bar timeline-handle-pill" />
          </div>
        </div>
      ))}

      {/* Range-select drag line */}
      {rangeSelect && rangeWidth > 2 && (
        <div
          className="text-range-select timeline-range-select absolute pointer-events-none z-5"
          style={{ left: rangeLeft, width: rangeWidth }}
        />
      )}

      {/* Selected count badge */}
      {selectedIds.size > 0 && !rangeSelect && (
        <div
          className="text-selection-badge absolute top-0 right-1 text-[8px] font-bold px-1 rounded-b-sm pointer-events-none z-20"
          style={{ background: 'var(--timeline-zoom-color)', color: 'var(--timeline-float-fg)' }}
        >
          {selectedIds.size} selected · Del
        </div>
      )}

      {hoverState && hoverState.type === 'split' && !isDraggingRange.current && (
        <button
          className="text-split-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center"
          data-tone="accent"
          style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            onTextSplit?.(hoverState.seg.id, hoverState.time);
            setHoverState(null);
          }}
        >
          <Scissors className="w-2 h-2" />
        </button>
      )}
      {hoverState && hoverState.type === 'add' && onAddText && !isDraggingRange.current && (
        <button
          className="text-add-btn timeline-arch-button absolute bottom-0 z-10 pointer-events-auto flex items-center justify-center text-[8px] font-bold"
          data-tone="accent"
          style={{ left: hoverState.x - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const time = (hoverState.x / rect.width) * safeDuration;
            onAddText(time);
            setHoverState(null);
          }}
        >
          +
        </button>
      )}
    </div>
  );
};
