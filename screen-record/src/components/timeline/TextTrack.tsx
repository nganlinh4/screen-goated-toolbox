import React, { useState } from 'react';
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
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
  onAddText?: (atTime?: number) => void;
}

export const TextTrack: React.FC<TextTrackProps> = ({
  segment,
  duration,
  editingTextId,
  onTextClick,
  onHandleDragStart,
  onAddText,
}) => {
  const [hoverX, setHoverX] = useState<number | null>(null);

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * duration;
    const thresholdTime = getHandlePriorityThresholdTime(duration, rect.width);
    const isOverSegment = segment.textSegments?.some(
      text => time >= text.startTime && time <= text.endTime
    );
    const isNearBoundary = isTimeNearRangeBoundary(
      time,
      segment.textSegments ?? [],
      thresholdTime,
    );
    setHoverX(isOverSegment || isNearBoundary ? null : x);
  };

  return (
    <div
      className="text-track timeline-lane relative h-7"
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setHoverX(null)}
    >
      {segment.textSegments?.map((text) => (
        <div
          key={text.id}
          onPointerDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const clickX = e.clientX - rect.left;
            const clickTime = (clickX / rect.width) * duration;
            onHandleDragStart(text.id, 'body', clickTime - text.startTime);
          }}
          onClick={(e) => {
            e.stopPropagation();
            onTextClick(text.id);
          }}
          className="text-segment timeline-block absolute h-full cursor-move group"
          data-tone="accent"
          data-active={editingTextId === text.id ? "true" : "false"}
          style={{
            left: `${(text.startTime / duration) * 100}%`,
            width: `${((text.endTime - text.startTime) / duration) * 100}%`,
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
          {/* Resize handles — rounded pill style */}
          <div
            className="text-handle-start absolute inset-y-0 -left-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(text.id, 'start'); }}
          >
            <div
              className="text-handle-bar timeline-handle-pill"
            />
          </div>
          <div
            className="text-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(text.id, 'end'); }}
          >
            <div
              className="text-handle-bar timeline-handle-pill"
            />
          </div>
        </div>
      ))}

      {/* Hover add button */}
      {hoverX !== null && onAddText && (
        <button
          className="text-add-btn timeline-add-button absolute top-1/2 -translate-y-1/2 w-4 h-4 text-[10px] leading-none font-bold z-10 pointer-events-auto"
          data-tone="accent"
          style={{ left: hoverX - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const time = (hoverX / rect.width) * duration;
            onAddText(time);
            setHoverX(null);
          }}
        >
          +
        </button>
      )}
    </div>
  );
};
