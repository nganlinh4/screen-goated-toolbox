import type {
  ImportedAudioSegment,
  SubtitleSegment,
  SubtitleSourceGroup,
  SubtitleSourceGroupAssignment,
  SubtitleSourceGroupKind,
} from '@/types/video';

export type SubtitleSourceGroupId =
  | 'video'
  | 'mic'
  | 'audio'
  | 'unassigned'
  | `audio:${string}`;

const SOURCE_GROUP_COLORS = [
  '#2563eb',
  '#0f9f8d',
  '#d97706',
  '#8b5cf6',
  '#e11d48',
  '#0891b2',
  '#65a30d',
  '#f97316',
  '#a855f7',
  '#14b8a6',
];

export function getSubtitleSourceGroupId(
  subtitle: Pick<SubtitleSegment, 'sourceGroup' | 'provenance'>,
): SubtitleSourceGroupId {
  const group = subtitle.sourceGroup;
  if (group?.kind === 'audio') {
    return group.audioSegmentId ? `audio:${group.audioSegmentId}` : 'audio';
  }
  if (group?.kind === 'video' || group?.kind === 'mic' || group?.kind === 'unassigned') {
    return group.kind;
  }
  const provenance = getLegacyCompatibleProvenance(subtitle.provenance);
  if (provenance) {
    return `audio:${provenance.audioSegmentId}`;
  }
  return 'unassigned';
}

export function getSubtitleSourceGroup(
  subtitle: Pick<SubtitleSegment, 'sourceGroup' | 'provenance'>,
): SubtitleSourceGroup {
  if (subtitle.sourceGroup) return subtitle.sourceGroup;
  const provenance = getLegacyCompatibleProvenance(subtitle.provenance);
  if (provenance) {
    return {
      kind: 'audio',
      assignment: 'generated',
      audioSegmentId: provenance.audioSegmentId,
      sourceName: provenance.sourceName,
      sourcePath: provenance.sourcePath,
    };
  }
  return { kind: 'unassigned' };
}

function getLegacyCompatibleProvenance(provenance: SubtitleSegment['provenance'] | undefined) {
  if (!provenance) return null;
  const raw = provenance as SubtitleSegment['provenance'] & {
    sourceKind?: string;
    musicSegmentId?: string;
  };
  const legacyKind = raw.sourceKind as string | undefined;
  if (legacyKind === 'audio' && raw.audioSegmentId) {
    return raw;
  }
  if (legacyKind === 'music' && raw.musicSegmentId) {
    return {
      ...raw,
      sourceKind: 'audio' as const,
      audioSegmentId: raw.musicSegmentId,
    };
  }
  return null;
}

export function makeSubtitleSourceGroup(params: {
  kind: SubtitleSourceGroupKind;
  assignment?: SubtitleSourceGroupAssignment;
  audioSegment?: ImportedAudioSegment | null;
}): SubtitleSourceGroup {
  if (params.kind === 'audio') {
    return {
      kind: 'audio',
      assignment: params.assignment,
      audioSegmentId: params.audioSegment?.id,
      sourceName: params.audioSegment?.name,
      sourcePath: params.audioSegment?.rawAudioPath,
    };
  }
  return {
    kind: params.kind,
    assignment: params.assignment,
  };
}

export function getSubtitleSourceGroupColor(groupId: SubtitleSourceGroupId): string | null {
  if (groupId === 'unassigned') return null;
  let hash = 0;
  for (let index = 0; index < groupId.length; index += 1) {
    hash = ((hash << 5) - hash + groupId.charCodeAt(index)) | 0;
  }
  return SOURCE_GROUP_COLORS[Math.abs(hash) % SOURCE_GROUP_COLORS.length];
}

export function inferAudioSourceGroupAtRange(
  startTime: number,
  endTime: number,
  audioSegments: readonly ImportedAudioSegment[] | null | undefined,
): SubtitleSourceGroup {
  const midpoint = (startTime + endTime) / 2;
  const matches = (audioSegments ?? []).filter((segment) => {
    const visibleDuration = Math.max(segment.outPoint - segment.inPoint, 0);
    return midpoint >= segment.startTime && midpoint <= segment.startTime + visibleDuration;
  });
  if (matches.length !== 1) {
    return { kind: 'unassigned' };
  }
  return makeSubtitleSourceGroup({
    kind: 'audio',
    assignment: 'inferred',
    audioSegment: matches[0],
  });
}

export function subtitleOverlapsSourceGroup(
  subtitle: SubtitleSegment,
  groupId: SubtitleSourceGroupId,
): boolean {
  if (groupId === 'audio') {
    return getSubtitleSourceGroup(subtitle).kind === 'audio';
  }
  return getSubtitleSourceGroupId(subtitle) === groupId;
}

export function getAudioLocalSubtitleTiming(
  subtitle: SubtitleSegment,
  audioSegment: ImportedAudioSegment,
): { startTime: number; endTime: number } | null {
  const visibleDuration = Math.max(audioSegment.outPoint - audioSegment.inPoint, 0);
  const timelineStart = audioSegment.startTime;
  const timelineEnd = timelineStart + visibleDuration;
  const overlapStart = Math.max(subtitle.startTime, timelineStart);
  const overlapEnd = Math.min(subtitle.endTime, timelineEnd);
  if (overlapEnd - overlapStart <= 0.0001) return null;
  return {
    startTime: audioSegment.inPoint + (overlapStart - timelineStart),
    endTime: audioSegment.inPoint + (overlapEnd - timelineStart),
  };
}
