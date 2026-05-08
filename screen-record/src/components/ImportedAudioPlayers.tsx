import { memo, useEffect, useMemo, useRef, useState } from "react";
import type { AudioGainPoint, ImportedAudioSegment, NarrationSegment, SpeedPoint } from "@/types/video";
import { getSpeedAtTime } from "@/lib/exportEstimator";
import { getMediaServerUrl } from "@/lib/mediaServer";
import {
  mergeLiveNarrationSegments,
  useLiveNarrationState,
} from "@/lib/liveNarrationStreamStore";

interface ImportedAudioPlayersProps {
  segments: ImportedAudioSegment[] | undefined;
  audioTrackVolumePoints?: AudioGainPoint[];
  narrationTrackVolumePoints?: AudioGainPoint[];
  speedPoints?: SpeedPoint[];
  currentTime: number;
  isPlaying: boolean;
  resetKey?: number;
  liveNarrationProjectId?: string | null;
}

type PreviewAudioSegment = ImportedAudioSegment & {
  previewTrackKind?: "imported" | "narration";
};

const SEEK_DRIFT_THRESHOLD_SEC = 0.15;
const MIN_ACTIVE_SEC = 0.001;
const PREROLL_SEC = 0.75;
const PLAYBACK_WINDOW_TAIL_SEC = 0.35;

const mediaUrlCache = new Map<string, string>();

function getSegmentTimelineDuration(segment: ImportedAudioSegment) {
  const rate = segment.playbackRate && segment.playbackRate > 0
    ? segment.playbackRate
    : 1;
  const sourceDuration = Math.max(MIN_ACTIVE_SEC, segment.outPoint - segment.inPoint);
  return Math.max(MIN_ACTIVE_SEC, sourceDuration / rate);
}

function isSegmentInPlaybackWindow(
  segment: ImportedAudioSegment,
  currentTime: number,
  isPlaying: boolean,
) {
  const timelineDuration = getSegmentTimelineDuration(segment);
  const segmentEnd = segment.startTime + timelineDuration;
  if (!isPlaying) {
    return currentTime >= segment.startTime - PREROLL_SEC && currentTime <= segmentEnd;
  }
  return (
    segmentEnd >= currentTime - PLAYBACK_WINDOW_TAIL_SEC &&
    segment.startTime <= currentTime + PREROLL_SEC
  );
}

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
  liveNarrationProjectId,
}: ImportedAudioPlayersProps) {
  const liveNarrationState = useLiveNarrationState(liveNarrationProjectId);
  const effectiveSegments = useMemo<ImportedAudioSegment[]>(() => {
    const importedSegments: ImportedAudioSegment[] = [];
    const narrationSegments: NarrationSegment[] = [];
    for (const segment of segments ?? []) {
      if ((segment as PreviewAudioSegment).previewTrackKind === "narration") {
        narrationSegments.push(segment as NarrationSegment);
      } else {
        importedSegments.push(segment);
      }
    }
    return [
      ...importedSegments,
      ...mergeLiveNarrationSegments(narrationSegments, liveNarrationState).map((segment) => ({
        ...segment,
        previewTrackKind: "narration" as const,
      })),
    ];
  }, [liveNarrationState, segments]);
  const activeSegments = useMemo(
    () => effectiveSegments.filter((segment) =>
      isSegmentInPlaybackWindow(segment, currentTime, isPlaying),
    ),
    [currentTime, effectiveSegments, isPlaying],
  );
  const activeSignature = activeSegments.map((segment) => segment.id).join("|");
  const lastActiveSignatureRef = useRef("");
  useEffect(() => {
    if (lastActiveSignatureRef.current === activeSignature) return;
    lastActiveSignatureRef.current = activeSignature;
    if (!isPlaying) return;
    const narrationCount = activeSegments.filter(
      (segment) => (segment as PreviewAudioSegment).previewTrackKind === "narration",
    ).length;
    if (activeSegments.length === 0 && narrationCount === 0) return;
    console.info(
      `[NarrationPerf][PreviewAudioWindow] t=${currentTime.toFixed(2)} active=${activeSegments.length} narration=${narrationCount} total=${effectiveSegments.length} ids=${activeSegments.slice(0, 4).map((segment) => segment.id).join(",")}`,
    );
  }, [activeSegments, activeSignature, currentTime, effectiveSegments.length, isPlaying]);
  if (activeSegments.length === 0) return null;
  return (
    <>
      {activeSegments.map((segment) => (
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

const MusicSegmentAudio = memo(function MusicSegmentAudio({
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
    const cached = mediaUrlCache.get(segment.rawAudioPath);
    if (cached) {
      setUrl(cached);
      return undefined;
    }
    let cancelled = false;
    void (async () => {
      try {
        const next = await getMediaServerUrl(segment.rawAudioPath);
        mediaUrlCache.set(segment.rawAudioPath, next);
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
    el.preservesPitch = true;
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
      preload="metadata"
    />
  );
});
