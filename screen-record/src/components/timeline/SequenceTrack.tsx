import type { ProjectComposition, ProjectCompositionMode } from "@/types/video";
import { getClipOffsets, getSequenceDuration } from "@/lib/projectComposition";
import { Plus, Trash2 } from "lucide-react";

interface SequenceTrackProps {
  composition: ProjectComposition;
  onSelectClip: (clipId: string) => void;
  onInsertAt: (clipId: string | null, placement: "before" | "after") => void;
  onRemoveClip: (clipId: string) => void;
  onModeChange: (mode: ProjectCompositionMode) => void;
}

function formatDuration(duration: number): string {
  const minutes = Math.floor(duration / 60);
  const seconds = Math.floor(duration % 60);
  return `${minutes}:${seconds.toString().padStart(2, "0")}`;
}

export function SequenceTrack({
  composition,
  onSelectClip,
  onInsertAt,
  onRemoveClip,
  onModeChange,
}: SequenceTrackProps) {
  const totalDuration = Math.max(getSequenceDuration(composition), 0.001);
  const offsets = getClipOffsets(composition);
  return (
    <div className="sequence-track ui-surface flex items-center gap-3 rounded-xl px-3 py-2">
      <div className="sequence-track-meta flex items-center gap-2">
        <div className="sequence-mode-toggle ui-segmented">
          {(["separate", "unified"] as const).map((mode) => {
            const active = composition.mode === mode;
            return (
              <button
                key={mode}
                type="button"
                className={`sequence-mode-btn sequence-mode-btn-${mode} ui-segmented-button px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.12em] ${
                  active
                    ? "ui-segmented-button-active"
                    : ""
                }`}
                onClick={() => onModeChange(mode)}
              >
                {mode}
              </button>
            );
          })}
        </div>
      </div>
      <div className="sequence-track-rail ui-list-shell relative flex-1 min-w-0 h-14 rounded-lg overflow-hidden">
        <button
          type="button"
          className="sequence-insert-btn sequence-insert-btn-start ui-chip-button absolute left-1 top-1/2 z-10 -translate-y-1/2 rounded-full p-1 text-[var(--on-surface)]"
          onClick={() => onInsertAt(composition.selectedClipId, "before")}
          title="Add project before"
        >
          <Plus className="w-3 h-3" />
        </button>
        {composition.clips.map((clip) => {
          const widthPct = `${(Math.max(clip.duration, 0.001) / totalDuration) * 100}%`;
          const leftPct = `${(offsets[clip.id] / totalDuration) * 100}%`;
          const isSelected = composition.selectedClipId === clip.id;
          return (
            <div
              key={clip.id}
              className="sequence-clip absolute top-1 bottom-1 px-1"
              style={{ left: leftPct, width: widthPct }}
            >
              <button
                type="button"
                onClick={() => onSelectClip(clip.id)}
                className={`sequence-clip-card ui-choice-tile group relative h-full w-full rounded-md px-3 text-left ${
                  isSelected
                    ? "ui-choice-tile-active bg-[var(--ui-accent-soft)] text-[var(--on-surface)]"
                    : "text-[var(--on-surface-variant)]"
                }`}
              >
                <div className="sequence-clip-title truncate text-[11px] font-semibold">
                  {clip.name}
                </div>
                <div className="sequence-clip-duration text-[10px] opacity-70">
                  {formatDuration(clip.duration)}
                </div>
                <div className="sequence-clip-role text-[9px] uppercase tracking-[0.12em] opacity-55">
                  {clip.role === "root" ? "current" : "inserted"}
                </div>
              </button>
              <button
                type="button"
                onClick={() => onInsertAt(clip.id, "after")}
                className="sequence-insert-btn sequence-insert-btn-after ui-chip-button absolute -right-2 top-1/2 z-10 -translate-y-1/2 rounded-full p-1 text-[var(--on-surface)]"
                title="Add project after"
              >
                <Plus className="w-3 h-3" />
              </button>
              {clip.role !== "root" && composition.clips.length > 1 && (
                <button
                  type="button"
                  onClick={() => onRemoveClip(clip.id)}
                  className="sequence-remove-btn absolute right-1 top-1 rounded-md p-1 text-[var(--outline)] opacity-0 transition-opacity hover:text-red-400 group-hover:opacity-100"
                  title="Remove inserted clip"
                >
                  <Trash2 className="w-3 h-3" />
                </button>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
