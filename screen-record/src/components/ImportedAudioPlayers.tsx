import { useEffect, useRef, useState } from "react";
import type { AudioGainPoint, ImportedAudioSegment, SpeedPoint } from "@/types/video";
import { getSpeedAtTime } from "@/lib/exportEstimator";
import { getMediaServerUrl } from "@/lib/mediaServer";

interface ImportedAudioPlayersProps {
  segments: ImportedAudioSegment[] | undefined;
  audioTrackVolumePoints?: AudioGainPoint[];
  narrationTrackVolumePoints?: AudioGainPoint[];
  speedPoints?: SpeedPoint[];
  currentTime: number;
  isPlaying: boolean;
  resetKey?: number;
}

type PreviewAudioSegment = ImportedAudioSegment & {
  previewTrackKind?: "imported" | "narration";
};

const SEEK_DRIFT_THRESHOLD_SEC = 0.15;
const MIN_ACTIVE_SEC = 0.001;
const PREROLL_SEC = 0.75;
function getTrackVolumeAtTime(time: number, points: AudioGainPoint[] | undefined | null) {
  if (!points || points.length === 0) return 1;
  const sorted = [...points].sort((a, b) => a.time - b.time);
  const idx = sorted.findIndex((point) => point.time >= time);
  if (idx === -1) return Math.max(0, Math.min(1, sorted[sorted.length - 1]?.volume ?? 1));
  if (idx === 0) return Math.max(0, Math.min(1, sorted[0]?.volume ?? 1));
  const left = sorted[idx - 1];
  const right = sorted[idx];
  const ratio = Math.max(0, Math.min(1, (time - left.time) / Math.max(0.0001, right.time - left.time)));
  const cosT = (1 - Math.cos(ratio * Math.PI)) / 2;
  return Math.max(0, Math.min(1, left.volume + (right.volume - left.volume) * cosT));
}

/**
 * Renders a hidden `<audio>` element per audio segment and keeps each one
 * play/pause/seek-synced with the timeline. Multiple overlapping segments are
 * allowed — each plays independently.
 */
export function ImportedAudioPlayers({
  segments,
  audioTrackVolumePoints,
  narrationTrackVolumePoints,
  speedPoints,
  currentTime,
  isPlaying,
  resetKey = 0,
}: ImportedAudioPlayersProps) {
  if (!segments || segments.length === 0) return null;
  return (
    <>
      {segments.map((segment) => (
        <MusicSegmentAudio
          key={segment.id}
          segment={segment as PreviewAudioSegment}
          trackVolumePoints={
            (segment as PreviewAudioSegment).previewTrackKind === "narration"
              ? narrationTrackVolumePoints
              : audioTrackVolumePoints
          }
          speedPoints={speedPoints}
          currentTime={currentTime}
          isPlaying={isPlaying}
          resetKey={resetKey}
        />
      ))}
    </>
  );
}

interface MusicSegmentAudioProps {
  segment: PreviewAudioSegment;
  trackVolumePoints?: AudioGainPoint[];
  speedPoints?: SpeedPoint[];
  currentTime: number;
  isPlaying: boolean;
  resetKey: number;
}

function MusicSegmentAudio({
  segment,
  trackVolumePoints,
  speedPoints,
  currentTime,
  isPlaying,
  resetKey,
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
    const rate = segment.playbackRate && segment.playbackRate > 0
      ? segment.playbackRate
      : 1;
    const timelineSpeed = Math.max(0.1, Math.min(16, getSpeedAtTime(currentTime, speedPoints ?? [])));
    const effectiveRate = Math.max(0.05, Math.min(64, rate * timelineSpeed));
    if (Math.abs(el.playbackRate - effectiveRate) > 0.001) {
      el.playbackRate = effectiveRate;
    }
    const sourceDuration = Math.max(MIN_ACTIVE_SEC, segment.outPoint - segment.inPoint);
    const timelineDuration = Math.max(MIN_ACTIVE_SEC, sourceDuration / rate);
    const localTime = currentTime - segment.startTime;
    const inAudibleRange = localTime >= 0 && localTime < timelineDuration;
    const inWarmRange = localTime >= -PREROLL_SEC && localTime < timelineDuration;
    const targetTime =
      segment.inPoint + Math.max(0, Math.min(sourceDuration, localTime * rate));

    el.volume = inAudibleRange
      ? getTrackVolumeAtTime(currentTime, trackVolumePoints)
      : 0;

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

    if (inWarmRange && isPlaying) {
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
    segment.playbackRate,
    speedPoints,
    trackVolumePoints,
  ]);

  return (
    <audio
      key={`${segment.id}:${resetKey}`}
      ref={audioRef}
      src={url ?? undefined}
      className="hidden imported-audio-element"
      preload="auto"
    />
  );
}
