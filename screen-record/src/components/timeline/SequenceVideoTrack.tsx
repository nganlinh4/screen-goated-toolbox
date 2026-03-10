import type { ProjectComposition } from "@/types/video";
import type { SequenceTimelineModel } from "@/lib/sequenceTimeline";
import { Plus, X } from "lucide-react";

interface SequenceVideoTrackProps {
  composition: ProjectComposition;
  timeline: SequenceTimelineModel;
  thumbnailsByClipId: Record<string, string[]>;
  onSelectClip: (clipId: string) => void;
  onInsertClip: (clipId: string | null, placement: "before" | "after") => void;
  onRemoveClip: (clipId: string) => void;
}

export function SequenceVideoTrack({
  composition,
  timeline,
  thumbnailsByClipId,
  onSelectClip,
  onInsertClip,
  onRemoveClip,
}: SequenceVideoTrackProps) {
  const safeDuration = Math.max(timeline.totalDuration, 0.001);

  return (
    <div className="sequence-video-track-container relative h-14">
      <div className="sequence-video-track timeline-lane timeline-lane-strong relative h-10 overflow-hidden">
        {timeline.clips.map((timelineClip) => {
          const widthPct = (timelineClip.activeDuration / safeDuration) * 100;
          const leftPct = (timelineClip.sequenceStart / safeDuration) * 100;
          const thumbnails = thumbnailsByClipId[timelineClip.clipId] ?? [];
          const isSelected = composition.selectedClipId === timelineClip.clipId;

          return (
            <div
              key={timelineClip.clipId}
              className="sequence-video-clip absolute inset-y-0 px-[1px]"
              style={{
                left: `${leftPct}%`,
                width: `${widthPct}%`,
              }}
            >
              <button
                type="button"
                onClick={() => onSelectClip(timelineClip.clipId)}
                className={`sequence-video-clip-btn timeline-block group relative h-full w-full overflow-hidden text-left ${
                  isSelected
                    ? "bg-[var(--ui-accent-soft)]"
                    : "bg-[color-mix(in_srgb,var(--ui-surface-3)_78%,transparent)]"
                }`}
                data-tone="accent"
                data-active={isSelected ? "true" : "false"}
              >
                <div className="sequence-video-thumb-strip absolute inset-0 flex gap-px bg-[var(--ui-surface-2)]">
                  {thumbnails.length > 0 ? (
                    thumbnails.map((thumbnail, index) => (
                      <div
                        key={`${timelineClip.clipId}-${index}`}
                        className="sequence-video-thumb h-full flex-1"
                        style={{
                          backgroundImage: `url(${thumbnail})`,
                          backgroundSize: "cover",
                          backgroundPosition: "center",
                        }}
                      />
                    ))
                  ) : (
                    <div className="sequence-video-thumb-placeholder h-full w-full bg-[linear-gradient(135deg,rgba(255,255,255,0.05),rgba(255,255,255,0.12))]" />
                  )}
                </div>
                <div className="sequence-video-clip-overlay absolute inset-0 bg-black/15" />
                <div className="sequence-video-clip-header absolute inset-x-1 top-1 flex items-center justify-between gap-2">
                  <span className="sequence-video-clip-name max-w-full truncate rounded bg-black/45 px-1.5 py-0.5 text-[9px] font-semibold text-white/95 shadow-sm">
                    {timelineClip.clip.name}
                  </span>
                </div>
                {timelineClip.clip.role !== "root" && (
                  <button
                    type="button"
                    onClick={(event) => {
                      event.stopPropagation();
                      onRemoveClip(timelineClip.clipId);
                    }}
                    className="sequence-video-remove-btn ui-chip-button absolute left-1/2 top-1/2 z-10 flex h-6 w-6 -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-full bg-black/55 text-white/90 hover:bg-red-500/90"
                    title="Remove clip"
                  >
                    <X className="h-3.5 w-3.5" />
                  </button>
                )}
              </button>
            </div>
          );
        })}
      </div>

      <div className="sequence-video-boundary-controls pointer-events-none absolute inset-x-0 top-0 h-full">
        <button
          type="button"
          className="sequence-video-insert-btn timeline-add-button pointer-events-auto absolute flex h-5 w-5 items-center justify-center"
          data-tone="accent"
          style={{ left: "-10px", top: 44 }}
          onMouseDown={(event) => {
            event.stopPropagation();
            onInsertClip(null, "before");
          }}
          title="Insert project at start"
        >
          <Plus className="h-3 w-3" />
        </button>

        {timeline.clips.map((timelineClip, index) => (
          <button
            key={`after-${timelineClip.clipId}`}
            type="button"
            className="sequence-video-insert-btn timeline-add-button pointer-events-auto absolute flex h-5 w-5 items-center justify-center"
            data-tone="accent"
            style={{
              left: `calc(${(timelineClip.sequenceEnd / safeDuration) * 100}% - 10px)`,
              top: 44,
            }}
            onMouseDown={(event) => {
              event.stopPropagation();
              onInsertClip(timelineClip.clipId, "after");
            }}
            title={
              index === timeline.clips.length - 1
                ? "Insert project at end"
                : "Insert project between clips"
            }
          >
            <Plus className="h-3 w-3" />
          </button>
        ))}
      </div>
    </div>
  );
}
