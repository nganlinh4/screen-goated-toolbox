import React, { useMemo, useRef, useState } from 'react';
import { VideoSegment, TrimSegment } from '@/types/video';
import { getTotalTrimDuration, getTrimSegments } from '@/lib/trimSegments';
import { Scissors, Plus } from 'lucide-react';

function formatTime(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60);
  return `${minutes}:${remainingSeconds.toString().padStart(2, '0')}`;
}

interface TrimTrackProps {
  segment: VideoSegment;
  duration: number;
  thumbnails: string[];
  onTrimDragStart: (id: string, type: 'start' | 'end') => void;
  onTrimSplit: (id: string, splitTime: number) => void;
  onTrimAddSegment: (atTime: number) => void;
  isDraggingTrim?: boolean;
}

export const TrimTrack: React.FC<TrimTrackProps> = ({
  segment,
  duration,
  thumbnails,
  onTrimDragStart,
  onTrimSplit,
  onTrimAddSegment,
  isDraggingTrim,
}) => {
  const [hoverState, setHoverState] = useState<
    { type: 'split'; x: number; time: number; segment: TrimSegment } |
    { type: 'add'; x: number; time: number } |
    null
  >(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const trimSegments = useMemo(() => getTrimSegments(segment, duration), [segment, duration]);
  const totalTrimDuration = useMemo(() => getTotalTrimDuration(segment, duration), [segment, duration]);

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!trackRef.current) return;
    const rect = trackRef.current.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const time = (x / rect.width) * duration;
    const containing = trimSegments.find(seg => time >= seg.startTime && time <= seg.endTime);
    if (containing) {
      const canSplit = time > containing.startTime + 0.2 && time < containing.endTime - 0.2;
      setHoverState(canSplit ? { type: 'split', x, time, segment: containing } : null);
      return;
    }
    setHoverState({ type: 'add', x, time });
  };

  const excludedRanges = (() => {
    const gaps: Array<{ start: number; end: number }> = [];
    let cursor = 0;
    for (const seg of trimSegments) {
      if (seg.startTime > cursor) gaps.push({ start: cursor, end: seg.startTime });
      cursor = seg.endTime;
    }
    if (cursor < duration) gaps.push({ start: cursor, end: duration });
    return gaps;
  })();

  return (
    <div
      className="trim-track-container relative h-14"
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setHoverState(null)}
    >
      <div
        ref={trackRef}
        className="trim-track relative h-10 rounded overflow-hidden"
      >
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

        {excludedRanges.map((gap, idx) => (
          <div
            key={`${gap.start}-${gap.end}-${idx}`}
            className="trim-gap-region absolute inset-y-0 bg-black/60"
            style={{
              left: `${(gap.start / duration) * 100}%`,
              width: `${((gap.end - gap.start) / duration) * 100}%`,
            }}
          />
        ))}

        {trimSegments.map(seg => (
          <div
            key={seg.id}
            className="trim-active-region absolute inset-y-0 border border-white/20 pointer-events-none"
            style={{
              left: `${(seg.startTime / duration) * 100}%`,
              width: `${((seg.endTime - seg.startTime) / duration) * 100}%`,
            }}
          />
        ))}

        {isDraggingTrim && (
          <div className="trim-duration-label absolute inset-0 flex items-center justify-center z-20 pointer-events-none">
            <span className="text-[10px] font-bold text-white bg-black/60 backdrop-blur-sm px-1.5 py-0.5 rounded">
              {formatTime(totalTrimDuration)} / {formatTime(duration)}
            </span>
          </div>
        )}

        {trimSegments.map(seg => (
          <React.Fragment key={`handles-${seg.id}`}>
            <div
              className="trim-handle-start absolute inset-y-0 w-3 cursor-col-resize z-10 group"
              style={{ left: `calc(${(seg.startTime / duration) * 100}% - 6px)` }}
              onMouseDown={(e) => { e.stopPropagation(); onTrimDragStart(seg.id, 'start'); }}
            >
              <div className="trim-handle-bar absolute inset-y-0 w-1.5 bg-white/80 group-hover:bg-[var(--primary-color)] transition-colors rounded-full left-1/2 -translate-x-1/2" />
            </div>
            <div
              className="trim-handle-end absolute inset-y-0 w-3 cursor-col-resize z-10 group"
              style={{ left: `calc(${(seg.endTime / duration) * 100}% - 6px)` }}
              onMouseDown={(e) => { e.stopPropagation(); onTrimDragStart(seg.id, 'end'); }}
            >
              <div className="trim-handle-bar absolute inset-y-0 w-1.5 bg-white/80 group-hover:bg-[var(--primary-color)] transition-colors rounded-full left-1/2 -translate-x-1/2" />
            </div>
          </React.Fragment>
        ))}
      </div>

      {hoverState && (
        <button
          className={`trim-floating-btn absolute w-5 h-5 rounded-full text-white leading-none flex items-center justify-center z-20 ${
            hoverState.type === 'split'
              ? 'bg-[var(--primary-color)]/70 hover:bg-[var(--primary-color)]'
              : 'bg-emerald-500/70 hover:bg-emerald-500'
          }`}
          style={{
            left: hoverState.x - 8,
            top: hoverState.type === 'split' ? 44 : 20,
            transform: hoverState.type === 'split' ? undefined : 'translateY(-50%)',
          }}
          onMouseDown={(e) => {
            e.stopPropagation();
            if (hoverState.type === 'split') {
              onTrimSplit(hoverState.segment.id, hoverState.time);
            } else {
              onTrimAddSegment(hoverState.time);
            }
            setHoverState(null);
          }}
          title={hoverState.type === 'split' ? 'Split segment' : 'Add segment'}
        >
          {hoverState.type === 'split' ? <Scissors className="w-3 h-3" /> : <Plus className="w-3 h-3" />}
        </button>
      )}
    </div>
  );
};
