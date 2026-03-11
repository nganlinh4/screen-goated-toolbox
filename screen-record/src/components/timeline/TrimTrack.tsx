import React, { useEffect, useMemo, useRef, useState } from "react";
import { VideoSegment, TrimSegment } from "@/types/video";
import { getTotalTrimDuration, getTrimSegments } from "@/lib/trimSegments";
import { Scissors, Plus } from "lucide-react";
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from "./trackHoverUtils";

function formatTime(seconds: number): string {
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.floor(seconds % 60);
  return `${minutes}:${remainingSeconds.toString().padStart(2, "0")}`;
}

interface TrimTrackProps {
  segment: VideoSegment;
  duration: number;
  thumbnails: string[];
  onTrimDragStart: (id: string, type: "start" | "end") => void;
  onTrimSplit: (id: string, splitTime: number) => void;
  onTrimAddSegment: (atTime: number) => void;
  isDraggingTrim?: boolean;
  isSeeking?: boolean;
}

export const TrimTrack: React.FC<TrimTrackProps> = ({
  segment,
  duration,
  thumbnails,
  onTrimDragStart,
  onTrimSplit,
  onTrimAddSegment,
  isDraggingTrim,
  isSeeking,
}) => {
  const [hoverState, setHoverState] = useState<
    | { type: "split"; x: number; time: number; segment: TrimSegment }
    | { type: "add"; x: number; time: number }
    | null
  >(null);
  const trackRef = useRef<HTMLDivElement>(null);
  const trimSegments = useMemo(
    () => getTrimSegments(segment, duration),
    [segment, duration],
  );
  const totalTrimDuration = useMemo(
    () => getTotalTrimDuration(segment, duration),
    [segment, duration],
  );
  const thumbnailCells = useMemo(
    () =>
      thumbnails.map((thumbnail, index) => (
        <div
          key={index}
          className="trim-thumb h-full flex-shrink-0"
          style={{
            width: `calc(${100 / thumbnails.length}% - 1px)`,
            backgroundImage: `url(${thumbnail})`,
            backgroundSize: "cover",
            backgroundPosition: "center",
          }}
        />
      )),
    [thumbnails],
  );

  useEffect(() => {
    if (isSeeking) {
      setHoverState(null);
    }
  }, [isSeeking]);

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (isSeeking) {
      setHoverState(null);
      return;
    }
    if (!trackRef.current) return;
    const rect = trackRef.current.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const time = (x / rect.width) * duration;
    const thresholdTime = getHandlePriorityThresholdTime(duration, rect.width);
    const containing = trimSegments.find(
      (seg) => time >= seg.startTime && time <= seg.endTime,
    );
    if (containing) {
      const canSplit =
        time > containing.startTime + 0.2 && time < containing.endTime - 0.2;
      setHoverState(
        canSplit ? { type: "split", x, time, segment: containing } : null,
      );
      return;
    }
    if (isTimeNearRangeBoundary(time, trimSegments, thresholdTime)) {
      setHoverState(null);
      return;
    }
    setHoverState({ type: "add", x, time });
  };

  const excludedRanges = (() => {
    const gaps: Array<{ start: number; end: number }> = [];
    let cursor = 0;
    for (const seg of trimSegments) {
      if (seg.startTime > cursor)
        gaps.push({ start: cursor, end: seg.startTime });
      cursor = seg.endTime;
    }
    if (cursor < duration) gaps.push({ start: cursor, end: duration });
    return gaps;
  })();

  return (
    <div
      className={`trim-track-container relative h-14 ${
        isSeeking ? "cursor-grabbing" : "cursor-grab"
      }`}
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setHoverState(null)}
    >
      <div
        ref={trackRef}
        className="trim-track timeline-lane timeline-lane-strong relative h-10"
      >
        <div
          className="trim-track-clip absolute inset-0 overflow-hidden"
          style={{ borderRadius: "inherit" }}
        >
          <div className="trim-thumbnails absolute inset-0 bg-[var(--ui-surface-2)] flex gap-[1px] opacity-[0.06]">
            {thumbnailCells}
          </div>

          {trimSegments.map((seg) => {
            const segmentDuration = Math.max(seg.endTime - seg.startTime, 0.001);
            return (
              <div
                key={`trim-active-thumbs-${seg.id}`}
                className="trim-active-thumbnails absolute inset-y-0 overflow-hidden pointer-events-none"
                style={{
                  left: `${(seg.startTime / duration) * 100}%`,
                  width: `${(segmentDuration / duration) * 100}%`,
                }}
              >
                <div
                  className="trim-active-thumbnails-strip absolute inset-y-0 flex gap-[1px]"
                  style={{
                    left: `${-(seg.startTime / segmentDuration) * 100}%`,
                    width: `${(duration / segmentDuration) * 100}%`,
                  }}
                >
                  {thumbnailCells}
                </div>
              </div>
            );
          })}

          {excludedRanges.map((gap, idx) => (
            <div
              key={`${gap.start}-${gap.end}-${idx}`}
              className="trim-gap-region absolute inset-y-0"
              style={{
                left: `${(gap.start / duration) * 100}%`,
                width: `${((gap.end - gap.start) / duration) * 100}%`,
                backgroundColor: "var(--timeline-gap-overlay)",
              }}
            />
          ))}

          {trimSegments.map((seg) => (
            <div
              key={seg.id}
              className="trim-active-region absolute inset-y-0 border pointer-events-none"
              style={{
                left: `${(seg.startTime / duration) * 100}%`,
                width: `${((seg.endTime - seg.startTime) / duration) * 100}%`,
                borderColor: "var(--timeline-active-border)",
              }}
            />
          ))}
        </div>

        {isDraggingTrim && (
          <div className="trim-duration-label absolute inset-0 flex items-center justify-center z-20 pointer-events-none">
            <span
              className="trim-duration-pill timeline-badge text-[10px] font-bold px-1.5 py-0.5"
              style={{
                color: "var(--timeline-label-fg)",
                backgroundColor: "var(--timeline-label-bg)",
              }}
            >
              {formatTime(totalTrimDuration)} / {formatTime(duration)}
            </span>
          </div>
        )}

        {trimSegments.map((seg) => (
          <React.Fragment key={`handles-${seg.id}`}>
            <div
              className="trim-handle-start absolute inset-y-0 w-3 cursor-col-resize z-10 group"
              style={{
                left: `calc(${(seg.startTime / duration) * 100}% - 6px)`,
              }}
              onPointerDown={(e) => {
                e.stopPropagation();
                onTrimDragStart(seg.id, "start");
              }}
            >
              <div className="trim-handle-bar trim-handle-pill absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 group-hover:bg-[var(--timeline-zoom-color)] group-hover:border-[var(--timeline-zoom-color)] group-hover:shadow-[0_0_6px_rgba(59,130,246,0.3)] group-hover:scale-y-110" />
            </div>
            <div
              className="trim-handle-end absolute inset-y-0 w-3 cursor-col-resize z-10 group"
              style={{ left: `calc(${(seg.endTime / duration) * 100}% - 6px)` }}
              onPointerDown={(e) => {
                e.stopPropagation();
                onTrimDragStart(seg.id, "end");
              }}
            >
              <div className="trim-handle-bar trim-handle-pill absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 group-hover:bg-[var(--timeline-zoom-color)] group-hover:border-[var(--timeline-zoom-color)] group-hover:shadow-[0_0_6px_rgba(59,130,246,0.3)] group-hover:scale-y-110" />
            </div>
          </React.Fragment>
        ))}
      </div>

      {hoverState && !isSeeking && (
        <button
          className="trim-floating-btn timeline-add-button absolute w-5 h-5 leading-none z-20"
          style={{
            left: hoverState.x - 8,
            top: hoverState.type === "split" ? 44 : 20,
            transform:
              hoverState.type === "split" ? undefined : "translateY(-50%)",
          }}
          data-tone={hoverState.type === "split" ? "accent" : "success"}
          onPointerDown={(e) => {
            e.stopPropagation();
            if (hoverState.type === "split") {
              onTrimSplit(hoverState.segment.id, hoverState.time);
            } else {
              onTrimAddSegment(hoverState.time);
            }
            setHoverState(null);
          }}
          title={hoverState.type === "split" ? "Split segment" : "Add segment"}
        >
          {hoverState.type === "split" ? (
            <Scissors className="w-3.5 h-3.5" />
          ) : (
            <Plus className="w-3 h-3" />
          )}
        </button>
      )}
    </div>
  );
};
