import React from "react";
import { Music } from "lucide-react";
import type { MusicAudioSegment } from "@/types/video";

interface MusicAudioTrackProps {
  segments: MusicAudioSegment[];
  duration: number;
  onAddSegment?: () => void;
  onSelectSegment?: (id: string) => void;
}

export const MusicAudioTrack: React.FC<MusicAudioTrackProps> = ({
  segments,
  duration,
  onAddSegment,
  onSelectSegment,
}) => {
  const safeDuration = Math.max(duration, 0.001);
  return (
    <div className="music-audio-track timeline-lane timeline-lane-strong relative h-10 overflow-hidden">
      {segments.map((seg) => {
        const trimmed = Math.max(seg.outPoint - seg.inPoint, 0);
        const widthPct = Math.min(100, (trimmed / safeDuration) * 100);
        const leftPct = Math.min(100, (seg.startTime / safeDuration) * 100);
        return (
          <button
            key={seg.id}
            type="button"
            onClick={() => onSelectSegment?.(seg.id)}
            className="music-audio-segment absolute top-0 bottom-0 flex items-center gap-1.5 rounded px-1.5 bg-[var(--primary-color)]/15 border border-[var(--primary-color)]/30 hover:bg-[var(--primary-color)]/25 transition-colors text-[10px] text-[var(--on-surface)] truncate"
            style={{ left: `${leftPct}%`, width: `${widthPct}%`, minWidth: 16 }}
            title={`${seg.name} • ${seg.duration.toFixed(2)}s`}
          >
            <Music className="w-2.5 h-2.5 shrink-0 text-[var(--primary-color)]" />
            <span className="truncate font-medium">{seg.name}</span>
          </button>
        );
      })}
      {onAddSegment && (
        <button
          type="button"
          onClick={onAddSegment}
          className="music-audio-add-btn absolute right-1 top-1/2 -translate-y-1/2 ui-chip-button h-5 w-5 rounded-full flex items-center justify-center text-[var(--primary-color)] opacity-0 hover:opacity-100 focus:opacity-100"
          title="Add audio file"
          aria-label="Add audio file"
        >
          +
        </button>
      )}
    </div>
  );
};
