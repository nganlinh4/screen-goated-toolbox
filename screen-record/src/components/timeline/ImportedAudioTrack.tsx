import React, { useCallback, useRef } from "react";
import { AudioLines, X } from "lucide-react";
import type { ImportedAudioSegment } from "@/types/video";

interface ImportedAudioTrackProps {
  segments: ImportedAudioSegment[];
  duration: number;
  onSelectSegment?: (id: string) => void;
  onUpdateSegment?: (id: string, patch: Partial<ImportedAudioSegment>) => void;
  onDeleteSegment?: (id: string) => void;
  selectedSegmentId?: string | null;
  beginBatch?: () => void;
  commitBatch?: () => void;
  onCommitSegments?: () => void;
}

const MIN_SEGMENT_DURATION_SEC = 0.05;
const DRAG_SLOP_PX = 3;

type DragMode = "body" | "trim-left" | "trim-right";

export const ImportedAudioTrack: React.FC<ImportedAudioTrackProps> = ({
  segments,
  duration,
  onSelectSegment,
  onUpdateSegment,
  onDeleteSegment,
  selectedSegmentId,
  beginBatch,
  commitBatch,
  onCommitSegments,
}) => {
  const trackRef = useRef<HTMLDivElement>(null);
  const safeDuration = Math.max(duration, 0.001);

  const startDrag = useCallback(
    (
      mode: DragMode,
      event: React.PointerEvent<HTMLElement>,
      seg: ImportedAudioSegment,
    ) => {
      if (event.button !== 0) return;
      if (!onUpdateSegment) return;
      const trackEl = trackRef.current;
      if (!trackEl) return;

      const rect = trackEl.getBoundingClientRect();
      const trackWidthPx = rect.width;
      if (trackWidthPx <= 0) return;
      const startClientX = event.clientX;
      const initialStart = seg.startTime;
      const initialIn = seg.inPoint;
      const initialOut = seg.outPoint;
      const initialLength = Math.max(initialOut - initialIn, MIN_SEGMENT_DURATION_SEC);
      const sourceDuration = Math.max(seg.duration, initialLength);
      const target = event.currentTarget;
      target.setPointerCapture(event.pointerId);
      let moved = false;
      beginBatch?.();

      const handleMove = (moveEvent: PointerEvent) => {
        const dxPx = moveEvent.clientX - startClientX;
        if (!moved && Math.abs(dxPx) < DRAG_SLOP_PX) return;
        moved = true;
        const dxSec = (dxPx / trackWidthPx) * safeDuration;

        if (mode === "body") {
          const maxStart = Math.max(0, safeDuration - initialLength);
          const next = Math.min(maxStart, Math.max(0, initialStart + dxSec));
          onUpdateSegment(seg.id, { startTime: next });
          return;
        }

        if (mode === "trim-left") {
          // Drag right → in_point grows, start_time grows by same delta so
          // the right edge of the visible segment stays put.
          const minIn = 0;
          const maxIn = Math.max(0, initialOut - MIN_SEGMENT_DURATION_SEC);
          const newIn = Math.min(maxIn, Math.max(minIn, initialIn + dxSec));
          const deltaIn = newIn - initialIn;
          const newStart = Math.min(
            Math.max(0, initialStart + deltaIn),
            Math.max(0, safeDuration - (initialOut - newIn)),
          );
          onUpdateSegment(seg.id, { inPoint: newIn, startTime: newStart });
          return;
        }

        if (mode === "trim-right") {
          // Drag right → out_point grows; left edge stays put.
          const minOut = initialIn + MIN_SEGMENT_DURATION_SEC;
          const maxOut = sourceDuration;
          const newOut = Math.min(maxOut, Math.max(minOut, initialOut + dxSec));
          onUpdateSegment(seg.id, { outPoint: newOut });
        }
      };

      const handleUp = (upEvent: PointerEvent) => {
        target.releasePointerCapture(upEvent.pointerId);
        target.removeEventListener("pointermove", handleMove);
        target.removeEventListener("pointerup", handleUp);
        target.removeEventListener("pointercancel", handleUp);
        if (!moved && mode === "body" && onSelectSegment) onSelectSegment(seg.id);
        commitBatch?.();
        if (moved) onCommitSegments?.();
      };

      target.addEventListener("pointermove", handleMove);
      target.addEventListener("pointerup", handleUp);
      target.addEventListener("pointercancel", handleUp);
      event.stopPropagation();
      event.preventDefault();
    },
    [beginBatch, commitBatch, onCommitSegments, onUpdateSegment, onSelectSegment, safeDuration],
  );

  return (
    <div className="imported-audio-track timeline-lane timeline-lane-strong group relative h-10 overflow-hidden">
      <div
        ref={trackRef}
        className="imported-audio-track-content absolute inset-y-0 left-0 right-0"
      >
        {segments.map((seg) => {
          const trimmed = Math.max(seg.outPoint - seg.inPoint, MIN_SEGMENT_DURATION_SEC);
          const widthPct = Math.min(100, (trimmed / safeDuration) * 100);
          const leftPct = Math.min(100, Math.max(0, (seg.startTime / safeDuration) * 100));
          const isSelected = selectedSegmentId === seg.id;
          return (
            <div
              key={seg.id}
              className="imported-audio-segment-wrap group/segment absolute top-0 bottom-0"
              style={{ left: `${leftPct}%`, width: `${widthPct}%`, minWidth: 16 }}
            >
              <button
                type="button"
                onPointerDown={(e) => startDrag("body", e, seg)}
                className={`imported-audio-segment relative w-full h-full flex items-center gap-1.5 rounded px-1.5 bg-[var(--primary-color)]/15 border hover:bg-[var(--primary-color)]/25 transition-colors text-[10px] text-[var(--on-surface)] truncate cursor-grab active:cursor-grabbing ${
                  isSelected
                    ? "border-[var(--primary-color)] shadow-[0_0_0_1px_var(--primary-color)]"
                    : "border-[var(--primary-color)]/30"
                }`}
                title={`${seg.name} • ${seg.duration.toFixed(2)}s`}
              >
                <AudioLines className="w-2.5 h-2.5 shrink-0 text-[var(--primary-color)]" />
                <span className="truncate font-medium pointer-events-none">{seg.name}</span>
              </button>
              {onDeleteSegment && (
                <button
                  type="button"
                  onClick={(event) => {
                    event.stopPropagation();
                    onDeleteSegment(seg.id);
                  }}
                  className={`imported-audio-delete-btn ui-icon-button absolute right-1 top-1/2 z-20 h-5 w-5 -translate-y-1/2 rounded-full bg-[var(--surface)]/90 text-[var(--on-surface-variant)] transition-opacity duration-150 hover:text-[var(--tertiary-color)] ${
                    isSelected
                      ? "opacity-100"
                      : "opacity-0 group-hover/segment:opacity-100 focus-visible:opacity-100"
                  }`}
                  title="Delete audio segment"
                  aria-label="Delete audio segment"
                >
                  <X className="h-3 w-3" strokeWidth={2.5} />
                </button>
              )}
              {onUpdateSegment && (
                <>
                  <div
                    onPointerDown={(e) => startDrag("trim-left", e, seg)}
                    className="imported-audio-segment-trim-left absolute left-0 top-0 bottom-0 w-1.5 cursor-ew-resize bg-[var(--primary-color)]/40 hover:bg-[var(--primary-color)]/70 rounded-l"
                    title="Trim start"
                  />
                  <div
                    onPointerDown={(e) => startDrag("trim-right", e, seg)}
                    className="imported-audio-segment-trim-right absolute right-0 top-0 bottom-0 w-1.5 cursor-ew-resize bg-[var(--primary-color)]/40 hover:bg-[var(--primary-color)]/70 rounded-r"
                    title="Trim end"
                  />
                </>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
};
