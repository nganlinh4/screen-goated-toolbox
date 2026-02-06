import React from 'react';
import { VideoSegment } from '@/types/video';

interface TextTrackProps {
  segment: VideoSegment;
  duration: number;
  editingTextId: string | null;
  onTextClick: (id: string) => void;
  onHandleDragStart: (id: string, type: 'start' | 'end' | 'body', offset?: number) => void;
}

export const TextTrack: React.FC<TextTrackProps> = ({
  segment,
  duration,
  editingTextId,
  onTextClick,
  onHandleDragStart,
}) => (
  <div className="relative h-7 rounded bg-[var(--surface)]/80">
    {segment.textSegments?.map((text) => (
      <div
        key={text.id}
        onMouseDown={(e) => {
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
        className={`absolute h-full cursor-move group rounded-sm ${
          editingTextId === text.id
            ? 'bg-[var(--primary-color)]/40 ring-1 ring-[var(--primary-color)]'
            : 'bg-[var(--primary-color)]/20 hover:bg-[var(--primary-color)]/30'
        }`}
        style={{
          left: `${(text.startTime / duration) * 100}%`,
          width: `${((text.endTime - text.startTime) / duration) * 100}%`,
        }}
      >
        <div className="absolute inset-0 flex items-center justify-center overflow-hidden px-1">
          <span className="truncate text-[10px] font-medium text-[var(--on-surface)]">
            {text.text}
          </span>
        </div>
        {/* Resize handles - visible on hover */}
        <div
          className="absolute inset-y-0 left-0 w-1 cursor-ew-resize opacity-0 group-hover:opacity-100 group-hover:bg-[var(--primary-color)]"
          onMouseDown={(e) => { e.stopPropagation(); onHandleDragStart(text.id, 'start'); }}
        />
        <div
          className="absolute inset-y-0 right-0 w-1 cursor-ew-resize opacity-0 group-hover:opacity-100 group-hover:bg-[var(--primary-color)]"
          onMouseDown={(e) => { e.stopPropagation(); onHandleDragStart(text.id, 'end'); }}
        />
      </div>
    ))}
  </div>
);
