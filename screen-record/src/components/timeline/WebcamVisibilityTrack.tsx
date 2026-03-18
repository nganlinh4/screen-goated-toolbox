import React, { useState } from "react";
import type { VideoSegment } from "@/types/video";
import { clampVisibilitySegmentsToDuration } from "@/lib/cursorHiding";
import {
  getHandlePriorityThresholdTime,
  isTimeNearRangeBoundary,
} from "./trackHoverUtils";

interface WebcamVisibilityTrackProps {
  segment: VideoSegment;
  duration: number;
  isAvailable: boolean;
  onWebcamClick: (id: string, splitTime: number) => void;
  onHandleDragStart: (
    id: string,
    type: "start" | "end" | "body",
    offset?: number,
  ) => void;
  onAddWebcamSegment?: (atTime?: number) => void;
}

export const WebcamVisibilityTrack: React.FC<WebcamVisibilityTrackProps> = ({
  segment,
  duration,
  isAvailable,
  onWebcamClick,
  onHandleDragStart,
  onAddWebcamSegment,
}) => {
  const [hoverX, setHoverX] = useState<number | null>(null);
  const safeDuration = Math.max(duration, 0.001);
  const segments = clampVisibilitySegmentsToDuration(
    segment.webcamVisibilitySegments,
    safeDuration,
  );

  const handleMouseMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!isAvailable) {
      setHoverX(null);
      return;
    }
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const time = (x / rect.width) * safeDuration;
    const thresholdTime = getHandlePriorityThresholdTime(safeDuration, rect.width);
    const isOverSegment = segments.some(
      (segmentRange) =>
        time >= segmentRange.startTime && time <= segmentRange.endTime,
    );
    const isNearBoundary = isTimeNearRangeBoundary(
      time,
      segments,
      thresholdTime,
    );
    setHoverX(isOverSegment || isNearBoundary ? null : x);
  };

  return (
    <div
      className={`webcam-visibility-track timeline-lane relative h-7 ${
        isAvailable ? "" : "timeline-lane-unavailable pointer-events-none"
      }`}
      onMouseMove={handleMouseMove}
      onMouseLeave={() => setHoverX(null)}
    >
      {segments.map((segmentRange) => (
        <div
          key={segmentRange.id}
          onPointerDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const clickX = e.clientX - rect.left;
            const clickTime = (clickX / rect.width) * safeDuration;
            onHandleDragStart(
              segmentRange.id,
              "body",
              clickTime - segmentRange.startTime,
            );
          }}
          onClick={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.getBoundingClientRect();
            const frac = (e.clientX - rect.left) / rect.width;
            const time =
              segmentRange.startTime +
              frac * (segmentRange.endTime - segmentRange.startTime);
            onWebcamClick(segmentRange.id, time);
          }}
          className="webcam-visibility-segment timeline-block absolute h-full cursor-move group"
          data-tone="webcam"
          style={{
            left: `${(segmentRange.startTime / safeDuration) * 100}%`,
            width: `${((segmentRange.endTime - segmentRange.startTime) / safeDuration) * 100}%`,
          }}
        >
          <div className="webcam-visibility-segment-content absolute inset-0 flex items-center justify-center overflow-hidden px-1">
            <span className="webcam-visibility-segment-icon text-[10px] text-[var(--timeline-webcam-color)] truncate">
              ●
            </span>
          </div>
          <div
            className="webcam-visibility-handle-start absolute inset-y-0 -left-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => {
              e.stopPropagation();
              onHandleDragStart(segmentRange.id, "start");
            }}
          >
            <div className="webcam-visibility-handle-bar timeline-handle-pill" />
          </div>
          <div
            className="webcam-visibility-handle-end absolute inset-y-0 -right-[2px] w-[5px] cursor-ew-resize flex items-center justify-center opacity-0 group-hover:opacity-100 z-10"
            onPointerDown={(e) => {
              e.stopPropagation();
              onHandleDragStart(segmentRange.id, "end");
            }}
          >
            <div className="webcam-visibility-handle-bar timeline-handle-pill" />
          </div>
        </div>
      ))}

      {hoverX !== null && onAddWebcamSegment && (
        <button
          className="webcam-visibility-add-btn timeline-add-button absolute top-1/2 -translate-y-1/2 w-4 h-4 text-white text-[10px] leading-none font-bold z-10 pointer-events-auto"
          data-tone="webcam"
          style={{ left: hoverX - 8 }}
          onPointerDown={(e) => {
            e.stopPropagation();
            const rect = e.currentTarget.parentElement!.getBoundingClientRect();
            const time = (hoverX / rect.width) * safeDuration;
            onAddWebcamSegment(time);
            setHoverX(null);
          }}
        >
          +
        </button>
      )}
    </div>
  );
};
