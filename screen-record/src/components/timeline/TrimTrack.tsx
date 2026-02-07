import React from 'react';
import { VideoSegment } from '@/types/video';

function formatTime(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60);
  return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
}

interface TrimTrackProps {
  segment: VideoSegment;
  duration: number;
  thumbnails: string[];
  onTrimDragStart: (type: 'start' | 'end') => void;
  isDraggingTrim?: boolean;
}

export const TrimTrack: React.FC<TrimTrackProps> = ({
  segment,
  duration,
  thumbnails,
  onTrimDragStart,
  isDraggingTrim,
}) => (
  <div className="trim-track relative h-10 rounded overflow-hidden">
    {/* Thumbnail strip */}
    <div className="trim-thumbnails absolute inset-0 bg-[var(--surface-container)] flex gap-[1px]">
      {thumbnails.map((thumbnail, index) => (
        <div
          key={index}
          className="trim-thumb h-full flex-shrink-0"
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
      className="trim-region-start absolute inset-y-0 left-0 bg-black/60"
      style={{ width: `${(segment.trimStart / duration) * 100}%` }}
    />
    {/* Trimmed-out overlay: end */}
    <div
      className="trim-region-end absolute inset-y-0 right-0 bg-black/60"
      style={{ width: `${((duration - segment.trimEnd) / duration) * 100}%` }}
    />

    {/* Active region border */}
    <div
      className="trim-active-region absolute inset-y-0 border border-white/20"
      style={{
        left: `${(segment.trimStart / duration) * 100}%`,
        right: `${((duration - segment.trimEnd) / duration) * 100}%`,
      }}
    />

    {/* Trim duration - shown during drag */}
    {isDraggingTrim && (
      <div
        className="trim-duration-label absolute inset-y-0 flex items-center justify-center z-20 pointer-events-none"
        style={{
          left: `${(segment.trimStart / duration) * 100}%`,
          width: `${((segment.trimEnd - segment.trimStart) / duration) * 100}%`,
        }}
      >
        <span className="text-[10px] font-bold text-white bg-black/60 backdrop-blur-sm px-1.5 py-0.5 rounded">
          {formatTime(segment.trimEnd - segment.trimStart)} / {formatTime(duration)}
        </span>
      </div>
    )}

    {/* Trim handle: start */}
    <div
      className="trim-handle-start absolute inset-y-0 w-3 cursor-col-resize z-10 group"
      style={{ left: `calc(${(segment.trimStart / duration) * 100}% - 6px)` }}
      onMouseDown={(e) => { e.stopPropagation(); onTrimDragStart('start'); }}
    >
      <div className="trim-handle-bar absolute inset-y-0 w-1.5 bg-white/80 group-hover:bg-[var(--primary-color)] transition-colors rounded-full left-1/2 -translate-x-1/2" />
    </div>

    {/* Trim handle: end */}
    <div
      className="trim-handle-end absolute inset-y-0 w-3 cursor-col-resize z-10 group"
      style={{ left: `calc(${(segment.trimEnd / duration) * 100}% - 6px)` }}
      onMouseDown={(e) => { e.stopPropagation(); onTrimDragStart('end'); }}
    >
      <div className="trim-handle-bar absolute inset-y-0 w-1.5 bg-white/80 group-hover:bg-[var(--primary-color)] transition-colors rounded-full left-1/2 -translate-x-1/2" />
    </div>
  </div>
);
