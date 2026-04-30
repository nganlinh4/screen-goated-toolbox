import { useEffect, useRef, useState } from "react";
import type { ImportedAudioSegment } from "@/types/video";
import { getMediaServerUrl } from "@/lib/mediaServer";

interface ImportedAudioPlayersProps {
  segments: ImportedAudioSegment[] | undefined;
  currentTime: number;
  isPlaying: boolean;
}

const SEEK_DRIFT_THRESHOLD_SEC = 0.15;

/**
 * Renders a hidden `<audio>` element per audio segment and keeps each one
 * play/pause/seek-synced with the timeline. Multiple overlapping segments are
 * allowed — each plays independently.
 */
export function ImportedAudioPlayers({
  segments,
  currentTime,
  isPlaying,
}: ImportedAudioPlayersProps) {
  if (!segments || segments.length === 0) return null;
  return (
    <>
      {segments.map((segment) => (
        <MusicSegmentAudio
          key={segment.id}
          segment={segment}
          currentTime={currentTime}
          isPlaying={isPlaying}
        />
      ))}
    </>
  );
}

interface MusicSegmentAudioProps {
  segment: ImportedAudioSegment;
  currentTime: number;
  isPlaying: boolean;
}

function MusicSegmentAudio({
  segment,
  currentTime,
  isPlaying,
}: MusicSegmentAudioProps) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const next = await getMediaServerUrl(segment.rawAudioPath);
        if (!cancelled) setUrl(next);
      } catch (err) {
        console.warn("[ImportedAudio] failed to resolve URL", err);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [segment.rawAudioPath]);

  useEffect(() => {
    const el = audioRef.current;
    if (!el || !url) return;
    const segDuration = Math.max(0.001, segment.outPoint - segment.inPoint);
    const localTime = currentTime - segment.startTime;
    const inRange = localTime >= 0 && localTime < segDuration;
    const targetTime =
      segment.inPoint + Math.max(0, Math.min(segDuration, localTime));

    if (Number.isFinite(targetTime)) {
      const drift = Math.abs((el.currentTime || 0) - targetTime);
      if (drift > SEEK_DRIFT_THRESHOLD_SEC) {
        try {
          el.currentTime = targetTime;
        } catch {
          /* element not ready yet */
        }
      }
    }

    if (inRange && isPlaying) {
      if (el.paused) {
        el.play().catch(() => {
          /* autoplay blocked or src not ready */
        });
      }
    } else if (!el.paused) {
      el.pause();
    }
  }, [
    url,
    currentTime,
    isPlaying,
    segment.startTime,
    segment.inPoint,
    segment.outPoint,
  ]);

  return (
    <audio
      ref={audioRef}
      src={url ?? undefined}
      className="hidden imported-audio-element"
      preload="auto"
    />
  );
}
