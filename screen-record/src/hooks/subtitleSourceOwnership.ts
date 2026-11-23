import { getEffectiveCompositionMode } from '@/lib/projectComposition';
import type { SubtitleGenerationPlan } from '@/lib/subtitleGenerationPlan';
import type { ImportedAudioSegment, Project, ProjectComposition, VideoSegment } from '@/types/video';

type MediaOwner = {
  kind: 'media'; path: string; clipId: string; source: 'video' | 'mic';
  trim: string; clipDuration: number | null;
};
type AudioOwner = {
  kind: 'audio'; id: string; path: string; timing: string;
};
export type SubtitleSourceOwner = MediaOwner | AudioOwner;
export interface SubtitleSourceSnapshot {
  localSegment?: VideoSegment | null;
  composition: ProjectComposition | null;
  segment: VideoSegment | null;
  loadedClipId: string | null;
  videoPath: string;
  micPath: string;
}
export function readSubtitleSourceSnapshot(
  project: React.MutableRefObject<Project | null>, segment: React.MutableRefObject<VideoSegment | null>,
  clip: React.MutableRefObject<string | null>, video: React.MutableRefObject<string>, mic: React.MutableRefObject<string>,
  local?: React.MutableRefObject<VideoSegment | null>,
): SubtitleSourceSnapshot {
  return { composition: project.current?.composition ?? null, segment: segment.current, localSegment: local?.current ?? segment.current,
    loadedClipId: clip.current, videoPath: video.current, micPath: mic.current };
}

function mediaTrim(segment: VideoSegment, source: 'video' | 'mic'): string {
  return JSON.stringify([
    segment.trimStart, segment.trimEnd,
    segment.trimSegments?.map(({ startTime, endTime }) => [startTime, endTime]) ?? null,
    source === 'mic' ? segment.micAudioOffsetSec ?? 0 : null,
  ]);
}

function audioTiming(audio: ImportedAudioSegment): string {
  return JSON.stringify([
    audio.startTime, audio.inPoint, audio.outPoint, audio.duration,
    audio.playbackRate ?? 1, audio.addedAt,
  ]);
}

export function captureSubtitleSourceOwners(
  plan: SubtitleGenerationPlan,
  segment: VideoSegment | null,
  composition: ProjectComposition | null,
): Record<string, SubtitleSourceOwner> {
  const owners: Record<string, SubtitleSourceOwner> = {};
  for (const payload of plan.clips) {
    const transform = plan.clipTransformsByClip[payload.clipId];
    if (transform) {
      const audio = composition?.audioSegments?.find((item) => item.id === transform.audioSegmentId);
      if (audio) owners[payload.clipId] = { kind: 'audio', id: audio.id, path: payload.sourcePath, timing: audioTiming(audio) };
      continue;
    }
    const clip = composition?.clips.find((item) => item.id === payload.clipId);
    const sourceSegment = composition && getEffectiveCompositionMode(composition) === 'unified'
      ? clip?.segment : segment;
    if (!sourceSegment) continue;
    const source = plan.sourceTypeForNative === 'mic' ? 'mic' : 'video';
    owners[payload.clipId] = {
      kind: 'media', path: payload.sourcePath, clipId: payload.clipId, source,
      trim: mediaTrim(sourceSegment, source), clipDuration: clip?.duration ?? null,
    };
  }
  return owners;
}

export function subtitleSourceMatches(
  owner: SubtitleSourceOwner | undefined,
  composition: ProjectComposition | null | undefined,
  loadedClipId: string | null,
  liveSegment: VideoSegment | null,
  liveVideoPath: string,
  liveMicPath: string,
): boolean {
  if (!owner) return false;
  if (owner.kind === 'audio') {
    const audio = composition?.audioSegments?.find((item) => item.id === owner.id);
    return !!audio && audio.rawAudioPath === owner.path && audioTiming(audio) === owner.timing;
  }
  const clip = composition?.clips.find((item) => item.id === owner.clipId);
  const isLoaded = loadedClipId === owner.clipId || (!loadedClipId && (!composition || composition.clips.length <= 1));
  // A selected clip can be replaced in the project before its old media has
  // finished unloading. Both the project record and the live editor must
  // still identify the source that produced the result.
  if (clip) {
    const clipPath = owner.source === 'mic' ? clip.rawMicAudioPath : clip.rawVideoPath;
    if (clipPath !== owner.path || mediaTrim(clip.segment, owner.source) !== owner.trim) return false;
  } else if (composition) {
    return false;
  }
  const currentSegment = isLoaded ? liveSegment : clip?.segment;
  const sourcePath = isLoaded
    ? owner.source === 'mic' ? liveMicPath : liveVideoPath
    : owner.source === 'mic' ? clip?.rawMicAudioPath : clip?.rawVideoPath;
  return !!currentSegment && sourcePath === owner.path && mediaTrim(currentSegment, owner.source) === owner.trim
    && (owner.clipDuration === null || clip?.duration === owner.clipDuration);
}
