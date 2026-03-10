import React, { useState } from 'react';
import { VideoSegment } from '@/types/video';
import { clampVisibilitySegmentsToDuration } from '@/lib/cursorHiding';
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from "./trackHoverUtils";

interface PointerTrackProps {
  segment: VideoSegment;
  duration: number;
  onPointerClick: (id: string, splitTime: number) => void;
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
  onAddPointerSegment?: (atTime?: number) => void;
  onPointerHover?: (id: string | null) => void;
}

export const PointerTrack: React.FC<PointerTrackProps> = ({
  segment,
  duration,
  onPointerClick,
  onHandleDragStart,
  onAddPointerSegment,
  onPointerHover,
}) => {
  const [hoverX, setHoverX] = useState<number | null>(null);
  const safeDuration = Math.max(duration, 0.001);
  const segments = clampVisibilitySegmentsToDuration(segment.cursorVisibilitySegments, safeDuration);

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const thresholdTime = getHandlePriorityThresholdTime(safeDuration, rect.width);
    const isOverSegment = segments.some(
      seg => time >= seg.startTime && time <= seg.endTime
    );
    const isNearBoundary = isTimeNearRangeBoundary(
      time,
      segments,
      thresholdTime,
    );
    setHoverX(isOverSegment || isNearBoundary ? null : x);
  };

  return (
    <div
      className="pointer-track timeline-lane relative h-7"
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setHoverX(null)}
    >
      {segments.map((seg) => (
        <div
          key={seg.id}
          onPointerDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const clickX = e.clientX - rect.left;
            const clickTime = (clickX / rect.width) * safeDuration;
            onHandleDragStart(seg.id, 'body', clickTime - seg.startTime);
          }}
          onClick={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.getBoundingClientRect();
            const frac = (e.clientX - rect.left) / rect.width;
            const time = seg.startTime + frac * (seg.endTime - seg.startTime);
            onPointerClick(seg.id, time);
          }}
          onMouseEnter={() => onPointerHover?.(seg.id)}
          onMouseLeave={() => onPointerHover?.(null)}
          className="pointer-segment timeline-block absolute h-full cursor-move group"
          data-tone="warning"
          style={{
            left: `${(seg.startTime / safeDuration) * 100}%`,
            width: `${((seg.endTime - seg.startTime) / safeDuration) * 100}%`,
          }}
        >
          <div className="pointer-segment-content absolute inset-0 flex items-center justify-center overflow-hidden px-1">
            <span className="pointer-segment-icon text-[10px] text-[var(--timeline-warning-color)] truncate">
              ●
            </span>
          </div>
          {/* Resize handles */}
          <div
            className="pointer-handle-start absolute inset-y-0 -left-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'start'); }}
          >
            <div
              className="pointer-handle-bar timeline-handle-pill"
            />
          </div>
          <div
            className="pointer-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'end'); }}
          >
            <div
              className="pointer-handle-bar timeline-handle-pill"
            />
          </div>
        </div>
      ))}

      {/* Hover add button */}
      {hoverX !== null && onAddPointerSegment && (
        <button
          className="pointer-add-btn timeline-add-button absolute top-1/2 -translate-y-1/2 w-4 h-4 text-white text-[10px] leading-none font-bold z-10 pointer-events-auto"
          data-tone="warning"
          style={{ left: hoverX - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const time = (hoverX / rect.width) * safeDuration;
            onAddPointerSegment(time);
            setHoverX(null);
          }}
        >
          +
        </button>
      )}
    </div>
  );
};
