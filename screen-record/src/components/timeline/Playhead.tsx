import React, { useRef, useEffect } from 'react';
import { VideoSegment } from '@/types/video';
import { clampToTrimSegments } from '@/lib/trimSegments';

interface PlayheadProps {
  currentTime: number;
  duration: number;
  isPlaying: boolean;
  videoRef: React.RefObject<HTMLVideoElement | null>;
  segment: VideoSegment;
}

export const Playhead: React.FC<PlayheadProps> = ({ currentTime, duration, isPlaying, videoRef, segment }) => {
  const ref = useRef<HTMLDivElement>(null);
  const rafRef = useRef<number>(0);

  // During playback: RAF loop reads video.currentTime directly for 60fps movement.
  // During pause/seek: React prop `currentTime` drives position (lower frequency is fine).
  useEffect(() => {
    if (!isPlaying || !videoRef.current || !ref.current || duration <= 0) return;

    const video = videoRef.current;
    const el = ref.current;

    const tick = () => {
      const clamped = clampToTrimSegments(video.currentTime, segment, duration);
      const pct = (clamped / duration) * 100;
      el.style.left = `${pct}%`;
      rafRef.current = requestAnimationFrame(tick);
    };
    rafRef.current = requestAnimationFrame(tick);

    return () => cancelAnimationFrame(rafRef.current);
  }, [isPlaying, duration, videoRef, segment]);

  // When not playing, sync from React state
  const safeTime = clampToTrimSegments(currentTime, segment, duration);
  const left = duration > 0 ? `${(safeTime / duration) * 100}%` : '0%';

  return (
    <div
      ref={ref}
      className="playhead absolute top-0 bottom-0 flex flex-col items-center pointer-events-none z-40"
      style={{
        left: isPlaying ? undefined : left,
        transform: 'translateX(-50%)',
      }}
    >
      <div
        className="playhead-arrow w-0 h-0 flex-shrink-0"
        style={{
          borderLeft: '5px solid transparent',
          borderRight: '5px solid transparent',
          borderTop: '6px solid #ef4444',
        }}
      />
      <div className="playhead-line w-0.5 flex-1 bg-red-500" />
    </div>
  );
};
