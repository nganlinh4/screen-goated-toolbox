import type { ImportedAudioSegment, SubtitleSegment, VideoSegment } from '@/types/video';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import { invoke } from '@/lib/ipc';
import { defaultSubtitleStyle } from '@/lib/subtitleDefaults';
import {
  clearDerivedSubtitleTracks,
  replaceOriginalSubtitleSegments,
} from '@/lib/subtitleTrackMutations';
import {
  getAudioLocalSubtitleTiming,
  getSubtitleSourceGroup,
  getSubtitleSourceGroupId,
} from '@/lib/subtitleSourceGroups';

export type SubtitleFileFormat = 'srt' | 'vtt';

export interface SubtitleFileInput {
  content: string;
  fileName?: string;
  format?: SubtitleFileFormat;
  mimeType?: string;
}

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

export function parseSubtitleSrt(
  srtContent: string,
  duration: number,
): SubtitleSegment[] {
  const safeDuration = Math.max(0, duration);
  const normalized = normalizeSubtitleText(srtContent);
  const blocks = normalized
    .split(/\n{2,}/)
    .map((block) => block.trim())
    .filter(Boolean);
  const subtitles: SubtitleSegment[] = [];

  for (const block of blocks) {
    const lines = block.split('\n').map((line) => line.trimEnd());
    const timingIndex = lines.findIndex((line) => line.includes('-->'));
    if (timingIndex < 0) continue;

    const timing = parseSrtTimingLine(lines[timingIndex]);
    if (!timing) continue;

    const text = lines
      .slice(timingIndex + 1)
      .join('\n')
      .trim();
    if (!text) continue;

    const startTime = clampTime(timing.startTime, safeDuration);
    const endTime = clampTime(timing.endTime, safeDuration);
    if (endTime <= startTime) continue;

    subtitles.push({
      id: crypto.randomUUID(),
      startTime,
      endTime,
      text,
      style: defaultSubtitleStyle(),
      sourceGroup: {
        kind: 'video',
        assignment: 'manual',
      },
    });
  }

  return subtitles.sort((a, b) => a.startTime - b.startTime);
}

export function parseSubtitleVtt(
  vttContent: string,
  duration: number,
): SubtitleSegment[] {
  const safeDuration = Math.max(0, duration);
  const lines = normalizeSubtitleText(vttContent).split('\n');
  const subtitles: SubtitleSegment[] = [];
  let index = 0;

  if (lines[index]?.trim().replace(/^\uFEFF/, '').startsWith('WEBVTT')) {
    index += 1;
  }

  while (index < lines.length) {
    while (index < lines.length && !lines[index].trim()) index += 1;
    if (index >= lines.length) break;

    const blockKind = lines[index].trim();
    if (blockKind.startsWith('NOTE')) {
      index = skipVttBlock(lines, index + 1);
      continue;
    }
    if (blockKind === 'STYLE' || blockKind === 'REGION') {
      index = skipVttBlock(lines, index + 1);
      continue;
    }

    let timingLine = lines[index].trim();
    if (!timingLine.includes('-->') && index + 1 < lines.length) {
      index += 1;
      timingLine = lines[index].trim();
    }
    if (!timingLine.includes('-->')) {
      index = skipVttBlock(lines, index + 1);
      continue;
    }

    const timing = parseVttTimingLine(timingLine);
    index += 1;
    const textLines: string[] = [];
    while (index < lines.length && lines[index].trim()) {
      textLines.push(lines[index]);
      index += 1;
    }
    if (!timing) continue;

    const text = stripSubtitleMarkup(textLines.join('\n')).trim();
    if (!text) continue;

    const startTime = clampTime(timing.startTime, safeDuration);
    const endTime = clampTime(timing.endTime, safeDuration);
    if (endTime <= startTime) continue;

    subtitles.push({
      id: crypto.randomUUID(),
      startTime,
      endTime,
      text,
      style: defaultSubtitleStyle(),
      sourceGroup: {
        kind: 'video',
        assignment: 'manual',
      },
    });
  }

  return subtitles.sort((a, b) => a.startTime - b.startTime);
}

export function detectSubtitleFileFormat(input: SubtitleFileInput): SubtitleFileFormat {
  if (input.format) return input.format;
  const fileName = input.fileName?.toLowerCase() ?? '';
  if (fileName.endsWith('.vtt')) return 'vtt';
  if (fileName.endsWith('.srt')) return 'srt';
  if (input.mimeType === 'text/vtt') return 'vtt';
  if (input.mimeType === 'application/x-subrip') return 'srt';
  const trimmed = input.content.replace(/^\uFEFF/, '').trimStart();
  if (trimmed.startsWith('WEBVTT')) return 'vtt';
  return 'srt';
}

export function parseSubtitleFile(
  input: SubtitleFileInput | string,
  duration: number,
): SubtitleSegment[] {
  const normalizedInput = typeof input === 'string'
    ? { content: input }
    : input;
  const format = detectSubtitleFileFormat(normalizedInput);
  return format === 'vtt'
    ? parseSubtitleVtt(normalizedInput.content, duration)
    : parseSubtitleSrt(normalizedInput.content, duration);
}

export function importSubtitleFileIntoSegment(
  segment: VideoSegment,
  input: SubtitleFileInput | string,
  duration: number,
): { segment: VideoSegment; subtitles: SubtitleSegment[] } {
  const isTimelineOnly = segment.mediaMode === 'timelineOnly';
  const subtitles = parseSubtitleFile(input, isTimelineOnly ? 0 : duration);
  if (subtitles.length === 0) {
    return { segment, subtitles };
  }
  const timelineEnd = isTimelineOnly
    ? Math.max(
        duration,
        segment.trimEnd,
        ...subtitles.map((subtitle) => subtitle.endTime),
        1,
      )
    : segment.trimEnd;
  const nextSegment = replaceOriginalSubtitleSegments(
    clearDerivedSubtitleTracks(segment),
    subtitles,
  );
  if (!isTimelineOnly) {
    return { segment: nextSegment, subtitles };
  }
  return {
    segment: {
      ...nextSegment,
      trimEnd: timelineEnd,
      trimSegments: (nextSegment.trimSegments?.length ? nextSegment.trimSegments : [
        { id: crypto.randomUUID(), startTime: 0, endTime: timelineEnd },
      ]).map((trimSegment, index) => index === 0
        ? { ...trimSegment, startTime: 0, endTime: Math.max(trimSegment.endTime, timelineEnd) }
        : trimSegment),
      speedPoints: nextSegment.speedPoints?.length
        ? nextSegment.speedPoints
        : [
            { time: 0, speed: 1 },
            { time: timelineEnd, speed: 1 },
          ],
    },
    subtitles,
  };
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

function parseSrtTimingLine(line: string): { startTime: number; endTime: number } | null {
  const [rawStart, rawEnd] = line.split('-->');
  if (!rawStart || !rawEnd) return null;
  const startTime = parseSrtTime(rawStart.trim());
  const endTime = parseSrtTime(rawEnd.trim().split(/\s+/)[0] ?? '');
  if (startTime === null || endTime === null) return null;
  return { startTime, endTime };
}

function parseSrtTime(value: string): number | null {
  const match = value.match(/^(\d{1,2}):(\d{2}):(\d{2})[,.](\d{1,3})$/);
  if (!match) return null;
  const [, hours, minutes, seconds, millis] = match;
  const milliseconds = Number(millis.padEnd(3, '0'));
  const parsed =
    Number(hours) * 3600
    + Number(minutes) * 60
    + Number(seconds)
    + milliseconds / 1000;
  return Number.isFinite(parsed) ? parsed : null;
}

function parseVttTimingLine(line: string): { startTime: number; endTime: number } | null {
  const [rawStart, rawEnd] = line.split('-->');
  if (!rawStart || !rawEnd) return null;
  const startTime = parseVttTime(rawStart.trim());
  const endTime = parseVttTime(rawEnd.trim().split(/\s+/)[0] ?? '');
  if (startTime === null || endTime === null) return null;
  return { startTime, endTime };
}

function parseVttTime(value: string): number | null {
  const normalized = value.replace(',', '.');
  const match = normalized.match(/^(?:(\d+):)?(\d{2}):(\d{2})\.(\d{1,3})$/);
  if (!match) return null;
  const [, hours = '0', minutes, seconds, millis] = match;
  const milliseconds = Number(millis.padEnd(3, '0'));
  const parsed =
    Number(hours) * 3600
    + Number(minutes) * 60
    + Number(seconds)
    + milliseconds / 1000;
  return Number.isFinite(parsed) ? parsed : null;
}

function skipVttBlock(lines: string[], startIndex: number): number {
  let index = startIndex;
  while (index < lines.length && lines[index].trim()) index += 1;
  return index;
}

function normalizeSubtitleText(content: string): string {
  return content
    .replace(/^\uFEFF/, '')
    .replace(/\r\n/g, '\n')
    .replace(/\r/g, '\n');
}

function stripSubtitleMarkup(text: string): string {
  return text
    .replace(/<\d{1,2}:\d{2}:\d{2}\.\d{1,3}>/g, '')
    .replace(/<\/?[^>]+>/g, '')
    .replace(/&nbsp;/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");
}

function clampTime(value: number, duration: number): number {
  if (duration <= 0) return Math.max(0, value);
  return Math.max(0, Math.min(duration, value));
}

function sanitizeFileStem(fileStem: string): string {
  const normalized = fileStem.trim().replace(/[\\/:*?"<>|]+/g, '-');
  return normalized || 'subtitles';
}
