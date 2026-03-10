import React, { useRef, useEffect } from "react";
import { VideoSegment } from "@/types/video";
import { clampToTrimSegments } from "@/lib/trimSegments";

interface PlayheadProps {
  currentTime: number;
  duration: number;
  isPlaying: boolean;
  videoRef: React.RefObject<HTMLVideoElement | null>;
  segment: VideoSegment;
  disableVideoSync?: boolean;
  headCenterY?: number;
  lineBottomY?: number;
}

export const Playhead: React.FC<PlayheadProps> = ({
  currentTime,
  duration,
  isPlaying,
  videoRef,
  segment,
  disableVideoSync = false,
  headCenterY = 0,
  lineBottomY,
}) => {
  const ref = useRef<HTMLDivElement>(null);
  const rafRef = useRef<number>(0);
  const useVideoSync = isPlaying && !disableVideoSync;

  // During playback: RAF loop reads video.currentTime directly for 60fps movement.
  // During pause/seek: React prop `currentTime` drives position (lower frequency is fine).
  useEffect(() => {
    if (!useVideoSync || !videoRef.current || !ref.current || duration <= 0)
      return;

    const video = videoRef.current;
    const el = ref.current;

    const tick = () => {
      const clamped = clampToTrimSegments(video.currentTime, segment, duration);
      const pct = Math.max(0, Math.min(100, (clamped / duration) * 100));
      el.style.left = `${pct}%`;
      rafRef.current = requestAnimationFrame(tick);
    };
    rafRef.current = requestAnimationFrame(tick);

    return () => cancelAnimationFrame(rafRef.current);
  }, [duration, segment, useVideoSync, videoRef]);

  // When not playing, sync from React state
  const safeTime = clampToTrimSegments(currentTime, segment, duration);
  const left =
    duration > 0
      ? `${Math.max(0, Math.min(100, (safeTime / duration) * 100))}%`
      : "0%";

  return (
    <div
      ref={ref}
      className="playhead absolute top-0 bottom-0 w-4 pointer-events-none z-40"
      style={{
        left: useVideoSync ? undefined : left,
        transform: "translateX(-50%)",
      }}
    >
      <div
        className="playhead-line absolute left-1/2 w-0.5 -translate-x-1/2 rounded-full"
        style={{
          top: 0,
          height: `${Math.max(lineBottomY ?? headCenterY + 8, 1)}px`,
          backgroundColor: "var(--timeline-playhead-color)",
          boxShadow: "0 0 10px color-mix(in srgb, var(--timeline-playhead-color) 35%, transparent)",
        }}
      />
      <div
        className="playhead-head absolute left-1/2 flex h-4 w-4 -translate-x-1/2 items-center justify-center"
        style={{ top: `${Math.max(headCenterY - 8, 0)}px` }}
      >
        <div
          className="playhead-head-shell absolute inset-0 rounded-full border"
          style={{
            backgroundColor: "var(--ui-surface-3)",
            borderColor: "color-mix(in srgb, var(--timeline-playhead-color) 55%, var(--ui-border))",
            boxShadow: "var(--shadow-elevation-2)",
          }}
        />
        <div
          className="playhead-head-core h-1.5 w-1.5 rounded-full"
          style={{
            backgroundColor: "var(--timeline-playhead-color)",
            boxShadow: "0 0 8px color-mix(in srgb, var(--timeline-playhead-color) 50%, transparent)",
          }}
        />
      </div>
    </div>
  );
};
