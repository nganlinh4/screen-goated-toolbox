import type { ImportedAudioSegment, SubtitleSegment } from '@/types/video';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import { invoke } from '@/lib/ipc';
import {
  getAudioLocalSubtitleTiming,
  getSubtitleSourceGroup,
  getSubtitleSourceGroupId,
} from '@/lib/subtitleSourceGroups';

export function buildSubtitleSrt(
  subtitles: SubtitleSegment[],
  range?: TrackSelectionRange | null,
): string {
  const filtered = subtitles
    .filter((subtitle) => {
      if (!range) return true;
      return subtitle.endTime > range.startTime && subtitle.startTime < range.endTime;
    })
    .map((subtitle) => {
      if (!range) return subtitle;
      return {
        ...subtitle,
        startTime: Math.max(subtitle.startTime, range.startTime) - range.startTime,
        endTime: Math.min(subtitle.endTime, range.endTime) - range.startTime,
      };
    })
    .filter((subtitle) => subtitle.endTime > subtitle.startTime)
    .sort((a, b) => a.startTime - b.startTime);

  return filtered
    .map((subtitle, index) => {
      const text = subtitle.text.replace(/\r\n/g, '\n').trim();
      return `${index + 1}\n${formatSrtTime(subtitle.startTime)} --> ${formatSrtTime(subtitle.endTime)}\n${text}`;
    })
    .join('\n\n');
}

export async function saveSubtitleSrt(
  subtitles: SubtitleSegment[],
  range?: TrackSelectionRange | null,
  fileStem = 'subtitles',
  notificationTitle = 'SRT saved to',
) {
  const srt = buildSubtitleSrt(subtitles, range);
  const defaultFileName = `${sanitizeFileStem(fileStem)}.srt`;
  const result = await invoke<{ savedPath?: string } | null>('save_subtitle_srt', {
    srtContent: srt,
    defaultFileName,
    notificationTitle,
  });
  return result?.savedPath ?? null;
}

export async function saveAudioSubtitleSrts(
  subtitles: SubtitleSegment[],
  audioSegments: readonly ImportedAudioSegment[] = [],
  notificationTitle = 'SRT saved to',
) {
  const bySource = new Map<string, SubtitleSegment[]>();
  const musicById = new Map(audioSegments.map((segment) => [segment.id, segment]));
  for (const subtitle of subtitles) {
    const group = getSubtitleSourceGroup(subtitle);
    if (group.kind !== 'audio' || !group.audioSegmentId) continue;
    const audioSegment = musicById.get(group.audioSegmentId);
    const provenance = subtitle.provenance as (SubtitleSegment['provenance'] & {
      sourceKind?: string;
      musicSegmentId?: string;
    }) | undefined;
    const timing = audioSegment
      ? getAudioLocalSubtitleTiming(subtitle, audioSegment)
      : provenance?.sourceKind === 'audio' || provenance?.sourceKind === 'music'
        ? {
            startTime: provenance.sourceLocalStartTime,
            endTime: provenance.sourceLocalEndTime,
          }
        : null;
    if (!timing) continue;
    const existing = bySource.get(group.audioSegmentId) ?? [];
    existing.push({
      ...subtitle,
      startTime: timing.startTime,
      endTime: timing.endTime,
    });
    bySource.set(group.audioSegmentId, existing);
  }

  const savedPaths: string[] = [];
  for (const [sourceId, group] of bySource.entries()) {
    const first = group[0];
    if (!first) continue;
    const sourceGroup = getSubtitleSourceGroup(first);
    const audioSegment = musicById.get(sourceId);
    const savedPath = await saveSubtitleSrt(
      group,
      null,
      audioSegment?.name || sourceGroup.sourceName || getSubtitleSourceGroupId(first),
      notificationTitle,
    );
    if (savedPath) savedPaths.push(savedPath);
  }
  return savedPaths;
}

function formatSrtTime(totalSeconds: number): string {
  const clamped = Math.max(0, totalSeconds);
  const hours = Math.floor(clamped / 3600);
  const minutes = Math.floor((clamped % 3600) / 60);
  const seconds = Math.floor(clamped % 60);
  const milliseconds = Math.round((clamped - Math.floor(clamped)) * 1000);
  return [
    hours.toString().padStart(2, '0'),
    minutes.toString().padStart(2, '0'),
    seconds.toString().padStart(2, '0'),
  ].join(':') + `,${milliseconds.toString().padStart(3, '0')}`;
}

function sanitizeFileStem(fileStem: string): string {
  const normalized = fileStem.trim().replace(/[\\/:*?"<>|]+/g, '-');
  return normalized || 'subtitles';
}
