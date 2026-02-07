import React, { useState } from 'react';
import { VideoSegment } from '@/types/video';

interface PointerTrackProps {
  segment: VideoSegment;
  duration: number;
  editingPointerId: string | null;
  onPointerClick: (id: string) => void;
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
  onAddPointerSegment?: (atTime?: number) => void;
}

export const PointerTrack: React.FC<PointerTrackProps> = ({
  segment,
  duration,
  editingPointerId,
  onPointerClick,
  onHandleDragStart,
  onAddPointerSegment,
}) => {
  const [hoverX, setHoverX] = useState<number | null>(null);
  const explicitSegments = segment.cursorVisibilitySegments;

  // When smart pointer hiding is off, show one full-duration segment (always visible)
  const isAlwaysVisible = !explicitSegments;
  const segments = explicitSegments ?? [{
    id: '__always-visible__',
    startTime: segment.trimStart,
    endTime: segment.trimEnd,
  }];

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (isAlwaysVisible) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * duration;
    const isOverSegment = segments.some(
      seg => time >= seg.startTime && time <= seg.endTime
    );
    setHoverX(isOverSegment ? null : x);
  };

  return (
    <div
      className="pointer-track relative h-7 rounded bg-[var(--surface)]/80"
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setHoverX(null)}
    >
      {segments.map((seg) => (
        <div
          key={seg.id}
          onMouseDown={isAlwaysVisible ? undefined : (e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const clickX = e.clientX - rect.left;
            const clickTime = (clickX / rect.width) * duration;
            onHandleDragStart(seg.id, 'body', clickTime - seg.startTime);
          }}
          onClick={isAlwaysVisible ? undefined : (e) => {
            e.stopPropagation();
            onPointerClick(seg.id);
          }}
          className={`pointer-segment absolute h-full rounded ${
            isAlwaysVisible
              ? 'bg-amber-500/15 cursor-default'
              : editingPointerId === seg.id
                ? 'bg-amber-500/40 ring-1 ring-amber-500 cursor-move group'
                : 'bg-amber-500/20 hover:bg-amber-500/30 cursor-move group'
          }`}
          style={{
            left: `${(seg.startTime / duration) * 100}%`,
            width: `${((seg.endTime - seg.startTime) / duration) * 100}%`,
          }}
        >
          <div className="pointer-segment-content absolute inset-0 flex items-center justify-center overflow-hidden px-1">
            <span className="pointer-segment-icon text-[10px] text-amber-300/80 truncate">
              ●
            </span>
          </div>
          {/* Resize handles — only for explicit segments */}
          {!isAlwaysVisible && (
            <>
              <div
                className="pointer-handle-start absolute inset-y-0 -left-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
                onMouseDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'start'); }}
              >
                <div className="pointer-handle-bar w-[3px] h-3 rounded-full bg-white/90 shadow-[0_0_4px_rgba(0,0,0,0.4)]" />
              </div>
              <div
                className="pointer-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
                onMouseDown={(e) => { e.stopPropagation(); onHandleDragStart(seg.id, 'end'); }}
              >
                <div className="pointer-handle-bar w-[3px] h-3 rounded-full bg-white/90 shadow-[0_0_4px_rgba(0,0,0,0.4)]" />
              </div>
            </>
          )}
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
            const time = (hoverX / rect.width) * duration;
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
