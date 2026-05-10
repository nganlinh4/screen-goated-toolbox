import { invoke } from "@/lib/ipc";
import { getTotalTrimDuration, getTrimBounds, normalizeSegmentTrimData } from "@/lib/trimSegments";
import type {
  AudioDownloadFormat,
  AudioDownloadResult,
  AudioDownloadTrackKind,
  ImportedAudioSegment,
  NarrationSegment,
  ProjectComposition,
  ProjectCompositionClip,
  VideoSegment,
} from "@/types/video";

interface NativeAudioDownloadClipJob {
  clipId: string;
  clipName: string;
  sourceVideoPath: string;
  deviceAudioPath: string;
  micAudioPath: string;
  trimStart: number;
  duration: number;
  segment: VideoSegment;
}

interface NativeVideoMetadataProbe {
  duration: number;
}

interface AudioDownloadRequest {
  trackKind: AudioDownloadTrackKind;
  format: AudioDownloadFormat;
  outputDir: string;
  trackLabel: string;
  clips: NativeAudioDownloadClipJob[];
  audioSegments?: ImportedAudioSegment[];
  audioTrackVolumePoints?: import("@/types/video").AudioGainPoint[];
  narrationSegments?: NarrationSegment[];
  narrationTrackVolumePoints?: import("@/types/video").AudioGainPoint[];
}

export interface StartAudioDownloadOptions {
  trackKind: AudioDownloadTrackKind;
  format: AudioDownloadFormat;
  outputDir: string;
  trackLabel: string;
  segment: VideoSegment | null;
  sourceVideoPath: string;
  micAudioPath: string;
  videoDuration: number;
  composition: ProjectComposition | null;
  resolveClipSourcePath: (clip: ProjectCompositionClip) => Promise<string>;
  resolveClipMicAudioPath: (clip: ProjectCompositionClip) => Promise<string>;
}

function sanitizeNativeExportValue<T>(value: T): T {
  return JSON.parse(
    JSON.stringify(value, (_key, nestedValue) =>
      nestedValue === null ? undefined : nestedValue),
  ) as T;
}

async function probeDuration(path: string): Promise<number> {
  if (!path.trim()) return 0;
  try {
    const metadata = await invoke<Partial<NativeVideoMetadataProbe>>("probe_video_metadata", { path });
    return typeof metadata?.duration === "number" && Number.isFinite(metadata.duration)
      ? metadata.duration
      : 0;
  } catch {
    return 0;
  }
}

async function buildSingleClipJob(options: StartAudioDownloadOptions): Promise<NativeAudioDownloadClipJob | null> {
  if (!options.segment) return null;
  const sourceDuration = options.videoDuration || options.segment.trimEnd || await probeDuration(options.sourceVideoPath);
  const normalizedSegment = normalizeSegmentTrimData(options.segment, sourceDuration || options.segment.trimEnd);
  const trimBounds = getTrimBounds(normalizedSegment, sourceDuration || normalizedSegment.trimEnd);
  const activeDuration = getTotalTrimDuration(normalizedSegment, sourceDuration || normalizedSegment.trimEnd);
  return {
    clipId: "root",
    clipName: "Root",
    sourceVideoPath: options.sourceVideoPath,
    deviceAudioPath: normalizedSegment.deviceAudioAvailable === false ? "" : options.sourceVideoPath,
    micAudioPath: options.micAudioPath,
    trimStart: trimBounds.trimStart,
    duration: activeDuration,
    segment: normalizedSegment,
  };
}

async function buildCompositionClipJobs(options: StartAudioDownloadOptions): Promise<NativeAudioDownloadClipJob[]> {
  const composition = options.composition;
  if (!composition || composition.clips.length <= 1) {
    const job = await buildSingleClipJob(options);
    return job ? [job] : [];
  }
  const jobs: NativeAudioDownloadClipJob[] = [];
  for (const clip of composition.clips) {
    const sourceVideoPath = await options.resolveClipSourcePath(clip);
    const micAudioPath = await options.resolveClipMicAudioPath(clip);
    const sourceDuration = await probeDuration(sourceVideoPath);
    const normalizedSegment = normalizeSegmentTrimData(
      clip.segment,
      sourceDuration || clip.segment.trimEnd,
    );
    const trimBounds = getTrimBounds(
      normalizedSegment,
      sourceDuration || normalizedSegment.trimEnd,
    );
    const activeDuration = getTotalTrimDuration(
      normalizedSegment,
      sourceDuration || normalizedSegment.trimEnd,
    );
    jobs.push({
      clipId: clip.id,
      clipName: clip.name,
      sourceVideoPath,
      deviceAudioPath: normalizedSegment.deviceAudioAvailable === false ? "" : sourceVideoPath,
      micAudioPath,
      trimStart: trimBounds.trimStart,
      duration: activeDuration,
      segment: normalizedSegment,
    });
  }
  return jobs;
}

export async function startAudioTrackDownload(options: StartAudioDownloadOptions): Promise<AudioDownloadResult> {
  const clips = await buildCompositionClipJobs(options);
  const request: AudioDownloadRequest = sanitizeNativeExportValue({
    trackKind: options.trackKind,
    format: options.format,
    outputDir: options.outputDir,
    trackLabel: options.trackLabel,
    clips,
    audioSegments: options.composition?.audioSegments ?? [],
    audioTrackVolumePoints: options.composition?.audioTrackVolumePoints ?? [],
    narrationSegments: options.composition?.narrationSegments ?? [],
    narrationTrackVolumePoints: options.composition?.narrationTrackVolumePoints ?? [],
  });
  return invoke<AudioDownloadResult>("start_audio_download", request as unknown as Record<string, unknown>);
}
