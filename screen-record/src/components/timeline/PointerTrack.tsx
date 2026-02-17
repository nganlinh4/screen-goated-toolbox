import React, { useState } from 'react';
import { VideoSegment } from '@/types/video';
import { clampVisibilitySegmentsToDuration } from '@/lib/cursorHiding';

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
    const isOverSegment = segments.some(
      seg => time >= seg.startTime && time <= seg.endTime
    );
    setHoverX(isOverSegment ? null : x);
  };

  return (
    <div
      className="pointer-track relative h-7 rounded"
      style={{ backgroundColor: 'var(--timeline-track-bg)' }}
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setHoverX(null)}
    >
      {segments.map((seg) => (
        <div
          key={seg.id}
          onMouseDown={(e) => {
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
          className="pointer-segment absolute h-full rounded bg-amber-500/20 hover:bg-amber-500/30 cursor-move group"
          style={{
            left: `${(seg.startTime / safeDuration) * 100}%`,
            width: `${((seg.endTime - seg.startTime) / safeDuration) * 100}%`,
          }}
        >
          <div className="pointer-segment-content absolute inset-0 flex items-center justify-center overflow-hidden px-1">
            <span className="pointer-segment-icon text-[10px] text-amber-300/80 truncate">
              ●
            </span>
          </div>
          {/* Resize handles */}
          <div
            className="pointer-handle-start absolute inset-y-0 -left-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onMouseDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'start'); }}
          >
            <div
              className="pointer-handle-bar w-[3px] h-3 rounded-full shadow-[0_0_4px_rgba(0,0,0,0.4)]"
              style={{ backgroundColor: 'var(--timeline-handle)' }}
            />
          </div>
          <div
            className="pointer-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onMouseDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'end'); }}
          >
            <div
              className="pointer-handle-bar w-[3px] h-3 rounded-full shadow-[0_0_4px_rgba(0,0,0,0.4)]"
              style={{ backgroundColor: 'var(--timeline-handle)' }}
            />
          </div>
        </div>
      ))}

      {/* Hover add button */}
      {hoverX !== null && onAddPointerSegment && (
        <button
          className="pointer-add-btn absolute top-1/2 -translate-y-1/2 w-4 h-4 rounded-full bg-amber-500/50 hover:bg-amber-500 flex items-center justify-center text-white text-[10px] leading-none font-bold transition-colors z-10 pointer-events-auto"
          style={{ left: hoverX - 8 }}
          onMouseDown={(e) => {
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
