import { getInitialBackgroundConfig } from "@/lib/appUtils";
import { defaultSubtitleStyle } from "@/lib/subtitleDefaults";
import { DEFAULT_WEBCAM_CONFIG } from "@/lib/webcam";
import type {
  ImportedAudioSegment,
  NarrationSegment,
  Project,
  ProjectComposition,
  SubtitleSegment,
  VideoSegment,
} from "@/types/video";

export type SyntheticProjectProfile = "small" | "huge" | "mega";

export interface SyntheticProjectOptions {
  profile?: SyntheticProjectProfile;
  subtitleCount?: number;
  narrationCount?: number;
  audioCount?: number;
  durationSec?: number;
}

function makeSegment(durationSec: number, subtitles: SubtitleSegment[]): VideoSegment {
  return {
    mediaMode: "timelineOnly",
    trimStart: 0,
    trimEnd: durationSec,
    zoomKeyframes: [],
    textSegments: [],
    subtitleTracks: [
      {
        id: "original",
        kind: "original",
        segments: subtitles,
      },
    ],
    activeSubtitleView: { kind: "track", trackId: "original" },
    subtitleSegments: subtitles,
    speedPoints: [
      { time: 0, speed: 1 },
      { time: durationSec * 0.25, speed: 1.35 },
      { time: durationSec * 0.5, speed: 0.8 },
      { time: durationSec * 0.75, speed: 1.6 },
    ],
    deviceAudioPoints: [
      { time: 0, volume: 1 },
      { time: durationSec * 0.5, volume: 0.55 },
      { time: durationSec, volume: 1 },
    ],
    micAudioPoints: [],
    deviceAudioAvailable: true,
    micAudioAvailable: false,
  };
}

function makeSubtitles(count: number, durationSec: number): SubtitleSegment[] {
  const slot = durationSec / Math.max(1, count);
  return Array.from({ length: count }, (_, index) => {
    const startTime = index * slot;
    const endTime = Math.min(durationSec, startTime + Math.max(0.35, slot * 0.85));
    return {
      id: `synthetic-subtitle-${index}`,
      startTime,
      endTime,
      text: `Synthetic subtitle ${index + 1}`,
      style: defaultSubtitleStyle(),
      sourceGroup: {
        kind: "video",
        assignment: "generated",
      },
    };
  });
}

function makeAudioSegments(count: number, durationSec: number): ImportedAudioSegment[] {
  const spacing = durationSec / Math.max(1, count);
  return Array.from({ length: count }, (_, index) => ({
    id: `synthetic-audio-${index}`,
    rawAudioPath: `C:\\SGT-Test\\audio-${index}.wav`,
    name: `Audio ${index + 1}`,
    duration: Math.max(1, spacing * 1.4),
    startTime: index * spacing,
    inPoint: 0,
    outPoint: Math.max(1, spacing * 1.4),
    playbackRate: index % 4 === 0 ? 1.25 : 1,
    addedAt: index,
  }));
}

function makeNarrationSegments(count: number, durationSec: number): NarrationSegment[] {
  const spacing = durationSec / Math.max(1, count);
  return Array.from({ length: count }, (_, index) => ({
    id: `synthetic-narration-${index}`,
    rawAudioPath: `C:\\SGT-Test\\narration-${index}.wav`,
    name: `Narration ${index + 1}`,
    duration: Math.max(0.3, spacing * 0.9),
    startTime: index * spacing,
    inPoint: 0,
    outPoint: Math.max(0.3, spacing * 0.9),
    playbackRate: index % 3 === 0 ? 1.1 : 1,
    addedAt: index,
    sourceSubtitleId: `synthetic-subtitle-${index}`,
  }));
}

export function createSyntheticProjectFixture(options: SyntheticProjectOptions = {}): Project {
  const profile = options.profile ?? "small";
  const durationSec = options.durationSec ?? (profile === "mega" ? 1800 : profile === "huge" ? 600 : 30);
  const subtitleCount = options.subtitleCount ?? (profile === "mega" ? 50_000 : profile === "huge" ? 10_000 : 12);
  const narrationCount = options.narrationCount ?? (profile === "mega" ? 5_000 : profile === "huge" ? 1_000 : 4);
  const audioCount = options.audioCount ?? (profile === "mega" ? 500 : profile === "huge" ? 80 : 2);
  const subtitles = makeSubtitles(subtitleCount, durationSec);
  const segment = makeSegment(durationSec, subtitles);
  const backgroundConfig = {
    ...getInitialBackgroundConfig(),
    canvasMode: "custom" as const,
    canvasWidth: 1920,
    canvasHeight: 1080,
  };
  const audioSegments = makeAudioSegments(audioCount, durationSec);
  const narrationSegments = makeNarrationSegments(narrationCount, durationSec);
  const composition: ProjectComposition = {
    mode: "separate",
    selectedClipId: "root",
    focusedClipId: "root",
    clips: [
      {
        id: "root",
        role: "root",
        name: "Synthetic Root",
        duration: durationSec,
        segment,
        backgroundConfig,
        webcamConfig: DEFAULT_WEBCAM_CONFIG,
        mousePositions: [],
        rawVideoPath: "C:\\SGT-Test\\synthetic-root.mp4",
      },
    ],
    audioSegments,
    audioTrackVolumePoints: [
      { time: 0, volume: 1 },
      { time: durationSec * 0.35, volume: 0.45 },
      { time: durationSec, volume: 0.9 },
    ],
    narrationSegments,
    narrationTrackVolumePoints: [
      { time: 0, volume: 1 },
      { time: durationSec * 0.5, volume: 0.7 },
      { time: durationSec, volume: 1 },
    ],
    timelineOnly: true,
  };

  return {
    id: `synthetic-${profile}`,
    name: `Synthetic ${profile} editor fixture`,
    createdAt: 1,
    lastModified: 1,
    duration: durationSec,
    segment,
    backgroundConfig,
    webcamConfig: DEFAULT_WEBCAM_CONFIG,
    mousePositions: [],
    rawVideoPath: "C:\\SGT-Test\\synthetic-root.mp4",
    composition,
  };
}
