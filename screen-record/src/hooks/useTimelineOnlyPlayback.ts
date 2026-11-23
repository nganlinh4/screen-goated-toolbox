import { useEffect, useRef, type MutableRefObject } from "react";
import { getSpeedAtTime } from "@/lib/speedCurve";
import { publishTimelinePlaybackTime } from "@/lib/timelinePlaybackClock";
import type { VideoSegment } from "@/types/video";

/** React time state cadence during timeline-only playback, comparable to the video element timeupdate cadence. */
export const TIMELINE_ONLY_STATE_COMMIT_MS = 100;
const END_EPSILON_SEC = 0.001;

interface TimelineOnlyPlaybackOptions {
  isTimelineOnly: boolean;
  isPlaying: boolean;
  segment: VideoSegment | null;
  duration: number;
  currentTimeRef: MutableRefObject<number>;
  setCurrentTime: (time: number) => void;
  setIsPlaying: (playing: boolean) => void;
  renderFrame: (time: number) => void;
}

/**
 * Advances timeline-only playback every animation frame through refs and the
 * renderer, committing React time state only every TIMELINE_ONLY_STATE_COMMIT_MS
 * and when playback stops, so the App tree does not re-render per frame.
 */
export function useTimelineOnlyPlayback({
  isTimelineOnly,
  isPlaying,
  segment,
  duration,
  currentTimeRef,
  setCurrentTime,
  setIsPlaying,
  renderFrame,
}: TimelineOnlyPlaybackOptions) {
  const playbackRef = useRef<{ raf: number | null; lastTick: number; lastCommit: number }>({
    raf: null,
    lastTick: 0,
    lastCommit: 0,
  });

  useEffect(() => {
    if (!isTimelineOnly || !isPlaying || !segment) return;
    const playback = playbackRef.current;
    const safeDuration = Math.max(segment.trimEnd, duration, 1);
    // Play pressed at the end replays from the start, as video projects do.
    if (currentTimeRef.current >= safeDuration - END_EPSILON_SEC) {
      currentTimeRef.current = 0;
      setCurrentTime(0);
    }
    playback.lastTick = performance.now();
    playback.lastCommit = playback.lastTick;
    publishTimelinePlaybackTime(currentTimeRef.current);

    const tick = (now: number) => {
      const dt = Math.max(0, (now - playback.lastTick) / 1000);
      playback.lastTick = now;
      const speed = Math.max(0.0625, getSpeedAtTime(currentTimeRef.current, segment.speedPoints ?? []));
      const nextTime = Math.min(safeDuration, currentTimeRef.current + dt * speed);
      currentTimeRef.current = nextTime;
      publishTimelinePlaybackTime(nextTime);
      renderFrame(nextTime);
      if (nextTime >= safeDuration) {
        playback.raf = null;
        setCurrentTime(nextTime);
        setIsPlaying(false);
        return;
      }
      if (now - playback.lastCommit >= TIMELINE_ONLY_STATE_COMMIT_MS) {
        playback.lastCommit = now;
        setCurrentTime(nextTime);
      }
      playback.raf = requestAnimationFrame(tick);
    };
    playback.raf = requestAnimationFrame(tick);

    return () => {
      if (playback.raf !== null) {
        cancelAnimationFrame(playback.raf);
        playback.raf = null;
        // Pausing, seeking or unmounting lands React state on the exact frame time.
        setCurrentTime(currentTimeRef.current);
      }
    };
  }, [currentTimeRef, duration, isPlaying, isTimelineOnly, renderFrame, segment, setCurrentTime, setIsPlaying]);
}
