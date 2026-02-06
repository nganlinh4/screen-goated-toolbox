import React from 'react';
import { VideoSegment } from '@/types/video';

interface TrimTrackProps {
  segment: VideoSegment;
  duration: number;
  thumbnails: string[];
  onTrimDragStart: (type: 'start' | 'end') => void;
}

export const TrimTrack: React.FC<TrimTrackProps> = ({
  segment,
  duration,
  thumbnails,
  onTrimDragStart,
}) => (
  <div className="relative h-10 rounded overflow-hidden">
    {/* Thumbnail strip */}
    <div className="absolute inset-0 bg-[var(--surface-container)] flex gap-[1px]">
      {thumbnails.map((thumbnail, index) => (
        <div
          key={index}
          className="h-full flex-shrink-0"
          style={{
            width: `calc(${100 / thumbnails.length}% - 1px)`,
            backgroundImage: `url(${thumbnail})`,
            backgroundSize: 'cover',
            backgroundPosition: 'center',
            opacity: 0.5,
          }}
        />
      ))}
    </div>

    {/* Trimmed-out overlay: start */}
    <div
      className="absolute inset-y-0 left-0 bg-black/60"
      style={{ width: `${(segment.trimStart / duration) * 100}%` }}
    />
    {/* Trimmed-out overlay: end */}
    <div
      className="absolute inset-y-0 right-0 bg-black/60"
      style={{ width: `${((duration - segment.trimEnd) / duration) * 100}%` }}
    />

    {/* Active region border */}
    <div
      className="absolute inset-y-0 border border-white/20"
      style={{
        left: `${(segment.trimStart / duration) * 100}%`,
        right: `${((duration - segment.trimEnd) / duration) * 100}%`,
      }}
    />

    {/* Trim handle: start */}
    <div
      className="absolute inset-y-0 w-3 cursor-col-resize z-10 group"
      style={{ left: `calc(${(segment.trimStart / duration) * 100}% - 6px)` }}
      onMouseDown={(e) => { e.stopPropagation(); onTrimDragStart('start'); }}
    >
      <div className="absolute inset-y-0 w-1.5 bg-white/80 group-hover:bg-[var(--primary-color)] transition-colors rounded-full left-1/2 -translate-x-1/2" />
    </div>

    {/* Trim handle: end */}
    <div
      className="absolute inset-y-0 w-3 cursor-col-resize z-10 group"
      style={{ left: `calc(${(segment.trimEnd / duration) * 100}% - 6px)` }}
      onMouseDown={(e) => { e.stopPropagation(); onTrimDragStart('end'); }}
    >
      <div className="absolute inset-y-0 w-1.5 bg-white/80 group-hover:bg-[var(--primary-color)] transition-colors rounded-full left-1/2 -translate-x-1/2" />
    </div>
  </div>
);
