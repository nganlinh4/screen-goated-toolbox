import React, { useCallback, useRef } from "react";
import { Music, Plus } from "lucide-react";
import type { MusicAudioSegment } from "@/types/video";

interface MusicAudioTrackProps {
  segments: MusicAudioSegment[];
  duration: number;
  onAddSegment?: () => void;
  onSelectSegment?: (id: string) => void;
  onUpdateSegment?: (id: string, patch: Partial<MusicAudioSegment>) => void;
}

const MIN_SEGMENT_DURATION_SEC = 0.05;

export const MusicAudioTrack: React.FC<MusicAudioTrackProps> = ({
  segments,
  duration,
  onAddSegment,
  onSelectSegment,
  onUpdateSegment,
}) => {
  const trackRef = useRef<HTMLDivElement>(null);
  const safeDuration = Math.max(duration, 0.001);

  const segmentDuration = useCallback(
    (seg: MusicAudioSegment) => Math.max(seg.outPoint - seg.inPoint, MIN_SEGMENT_DURATION_SEC),
    [],
  );

  const handleSegmentPointerDown = useCallback(
    (event: React.PointerEvent<HTMLButtonElement>, seg: MusicAudioSegment) => {
      if (event.button !== 0) return;
      if (!onUpdateSegment) return;
      const trackEl = trackRef.current;
      if (!trackEl) return;

      // Capture original layout in a coordinate system stable against re-renders
      const rect = trackEl.getBoundingClientRect();
      const trackWidthPx = rect.width;
      if (trackWidthPx <= 0) return;
      const startClientX = event.clientX;
      const initialStartTime = seg.startTime;
      const segLengthSec = segmentDuration(seg);
      const maxStart = Math.max(0, safeDuration - segLengthSec);

      const target = event.currentTarget;
      target.setPointerCapture(event.pointerId);
      let moved = false;

      const handleMove = (moveEvent: PointerEvent) => {
        const dxPx = moveEvent.clientX - startClientX;
        const dxSec = (dxPx / trackWidthPx) * safeDuration;
        if (!moved && Math.abs(dxPx) < 3) return;
        moved = true;
        const next = Math.min(maxStart, Math.max(0, initialStartTime + dxSec));
        onUpdateSegment(seg.id, { startTime: next });
      };

      const handleUp = (upEvent: PointerEvent) => {
        target.releasePointerCapture(upEvent.pointerId);
        target.removeEventListener("pointermove", handleMove);
        target.removeEventListener("pointerup", handleUp);
        target.removeEventListener("pointercancel", handleUp);
        if (!moved && onSelectSegment) onSelectSegment(seg.id);
      };

      target.addEventListener("pointermove", handleMove);
      target.addEventListener("pointerup", handleUp);
      target.addEventListener("pointercancel", handleUp);
      event.preventDefault();
    },
    [onUpdateSegment, onSelectSegment, safeDuration, segmentDuration],
  );

  return (
    <div
      ref={trackRef}
      className="music-audio-track timeline-lane timeline-lane-strong group relative h-10 overflow-hidden"
    >
      {segments.map((seg) => {
        const trimmed = segmentDuration(seg);
        const widthPct = Math.min(100, (trimmed / safeDuration) * 100);
        const leftPct = Math.min(100, Math.max(0, (seg.startTime / safeDuration) * 100));
        return (
          <button
            key={seg.id}
            type="button"
            onPointerDown={(e) => handleSegmentPointerDown(e, seg)}
            className="music-audio-segment absolute top-0 bottom-0 flex items-center gap-1.5 rounded px-1.5 bg-[var(--primary-color)]/15 border border-[var(--primary-color)]/30 hover:bg-[var(--primary-color)]/25 transition-colors text-[10px] text-[var(--on-surface)] truncate cursor-grab active:cursor-grabbing"
            style={{ left: `${leftPct}%`, width: `${widthPct}%`, minWidth: 16 }}
            title={`${seg.name} • ${seg.duration.toFixed(2)}s`}
          >
            <Music className="w-2.5 h-2.5 shrink-0 text-[var(--primary-color)]" />
            <span className="truncate font-medium pointer-events-none">{seg.name}</span>
          </button>
        );
      })}
      {onAddSegment && (
        <button
          type="button"
          onClick={onAddSegment}
          className="music-audio-add-btn ui-chip-button absolute right-1 top-1/2 -translate-y-1/2 z-10 h-5 w-5 rounded-full flex items-center justify-center text-[var(--primary-color)] bg-[var(--surface)]/80 backdrop-blur opacity-0 transition-opacity duration-150 group-hover:opacity-100 focus-visible:opacity-100"
          title="Add audio file"
          aria-label="Add audio file"
        >
          <Plus className="w-3 h-3" strokeWidth={3} />
        </button>
      )}
    </div>
  );
};
