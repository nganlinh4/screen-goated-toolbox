import type { Translations } from '@/i18n';
import type { AudioSubtitleClipTransform } from '@/lib/subtitleGenerationPlan';
import { smartSplitText, splitTimingByChunks } from '@/lib/segmentSmartSplit';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import {
  getSubtitleTracks,
  getVisibleSubtitleSegments,
} from '@/lib/subtitleTracks';
import type { SubtitleSegment, VideoSegment } from '@/types/video';
import type {
  SubtitleClipResult,
  SubtitleClipResultSegment,
  SubtitleJobStatus,
  SubtitleJobViewStatus,
} from './subtitleGenerationTypes';

function buildSubtitleId(
  clipId: string,
  entry: { startTime: number; endTime: number; text: string },
  _index: number,
) {
  return `subtitle-${clipId}-${Math.round(entry.startTime * 1000)}`;
}

function formatTemplate(template: string, params?: Record<string, string> | null) {
  let formatted = template;
  for (const [key, value] of Object.entries(params ?? {})) {
    formatted = formatted.split(`{${key}}`).join(value);
  }
  return formatted;
}

export function localizeSubtitleStatus(
  t: Translations,
  status: Pick<SubtitleJobViewStatus, 'message' | 'messageKey' | 'messageParams'> | null,
) {
  if (!status) {
    return null;
  }
  const key = status.messageKey;
  if (key && key in t) {
    return formatTemplate(t[key as keyof Translations] as string, status.messageParams);
  }
  return status.message;
}

export function stripSubtitleJobResults(status: SubtitleJobStatus): SubtitleJobViewStatus {
  const { results: _results, ...viewStatus } = status;
  return viewStatus;
}

export function buildSubtitleStatusViewKey(status: SubtitleJobViewStatus): string {
  return [
    status.state,
    status.messageKey ?? '',
    status.message,
    status.progress.toFixed(4),
    status.activeClipId ?? '',
    status.totalClips,
    status.completedClips,
    status.skipped.length,
    status.error ?? '',
    JSON.stringify(status.messageParams ?? {}),
  ].join('|');
}

export function buildAppliedResultsKey(results: SubtitleClipResult[]): string {
  return results.map((result) => [
    result.clipId,
    result.isPartial ? 'p' : 'f',
    String(result.segments.length),
    result.segments.map((segment, index) => {
      const isLastPartial = result.isPartial && index === result.segments.length - 1;
      return [
        Math.round(segment.startTime * 100),
        isLastPartial ? 'live' : Math.round(segment.endTime * 100),
        segment.text,
      ].join(':');
    }).join('|'),
  ].join('/')).join('||');
}

function splitGeneratedSubtitleSegments(
  segments: SubtitleClipResult['segments'],
  maxUnits: number,
): SubtitleClipResult['segments'] {
  return segments.flatMap((segment, segmentIndex) => {
    const chunks = smartSplitText(segment.text, maxUnits);
    if (chunks.length <= 1) return [segment];
    const timings = splitTimingByChunks(segment.startTime, segment.endTime, chunks);
    const splitGroupId = `split:${Math.round(segment.startTime * 1000)}:${Math.round(segment.endTime * 1000)}:${segmentIndex}`;
    return chunks.map((chunk, index) => ({
      startTime: timings[index]?.startTime ?? segment.startTime,
      endTime: timings[index]?.endTime ?? segment.endTime,
      text: chunk.text,
      splitGroupId,
      splitGroupIndex: index,
      splitGroupCount: chunks.length,
      splitGroupText: segment.text,
      splitGroupStartTime: segment.startTime,
      splitGroupEndTime: segment.endTime,
    }));
  });
}

export function splitGeneratedSubtitleResults(
  results: SubtitleClipResult[],
  enabled: boolean,
  maxUnits: number,
): SubtitleClipResult[] {
  if (!enabled) return results;
  return results.map((result) => ({
    ...result,
    segments: splitGeneratedSubtitleSegments(result.segments, maxUnits),
  }));
}

export function hasFinalSubtitleResult(results: SubtitleClipResult[]) {
  return results.some((result) => !result.isPartial);
}

export function summarizeSubtitleRanges(
  segments: ReadonlyArray<{ startTime: number; endTime: number; text: string }>,
) {
  const first = segments[0];
  const last = segments[segments.length - 1];
  const invalid = segments.filter((subtitle) => (
    !Number.isFinite(subtitle.startTime)
    || !Number.isFinite(subtitle.endTime)
    || subtitle.endTime <= subtitle.startTime
  )).length;
  const emptyText = segments.filter((subtitle) => !subtitle.text.trim()).length;
  const maxEnd = segments.reduce((max, subtitle) => Math.max(max, subtitle.endTime), 0);
  return [
    `count=${segments.length}`,
    `range=${first ? `${first.startTime.toFixed(2)}-${last?.endTime.toFixed(2)}` : 'none'}`,
    `maxEnd=${maxEnd.toFixed(2)}`,
    `invalid=${invalid}`,
    `empty=${emptyText}`,
    `first="${(first?.text ?? '').slice(0, 32)}"`,
    `last="${(last?.text ?? '').slice(0, 32)}"`,
  ].join(' ');
}

export function logSubtitleApplyDiagnostics(
  phase: string,
  result: SubtitleClipResult,
  before: VideoSegment | null,
  after: VideoSegment | null,
  replacementRanges: ReadonlyArray<Pick<TrackSelectionRange, 'startTime' | 'endTime'>>,
) {
  const beforeVisible = before ? getVisibleSubtitleSegments(before) : [];
  const afterVisible = after ? getVisibleSubtitleSegments(after) : [];
  const afterTracks = after ? getSubtitleTracks(after) : [];
  const activeView = after?.activeSubtitleView;
  const trackSummary = afterTracks
    .map((track) => `${track.id}:${track.segments.length}`)
    .join(',');
  const replacementSummary = replacementRanges
    .map((range) => `${range.startTime.toFixed(2)}-${range.endTime.toFixed(2)}`)
    .join(',');
  console.log(
    `[SubtitleGen][Diag][${phase}] clip=${result.clipId} partial=${result.isPartial ? 1 : 0} `
    + `incoming(${summarizeSubtitleRanges(result.segments)}) `
    + `visibleBefore=${beforeVisible.length} visibleAfter=${afterVisible.length} `
    + `active=${activeView?.kind ?? 'none'}:${activeView?.trackId ?? 'none'} `
    + `tracks=${trackSummary || 'none'} replacement=${replacementSummary || 'all'}`,
  );
}

export function coalesceSubtitleClipResults(
  current: SubtitleClipResult[],
  incoming: SubtitleClipResult[],
) {
  const byClipId = new Map(current.map((result) => [result.clipId, result]));
  for (const result of incoming) {
    const existing = byClipId.get(result.clipId);
    if (!existing || !result.isPartial || existing.isPartial) {
      byClipId.set(result.clipId, result);
    }
  }
  return Array.from(byClipId.values());
}

export function partialApplySignature(result: SubtitleClipResult) {
  const tail = result.segments[result.segments.length - 1] ?? null;
  return [
    result.clipId,
    result.segments.length,
    tail ? Math.round(tail.startTime * 10) : 'none',
  ].join(':');
}

export function buildInsertedSubtitle(
  result: SubtitleClipResult,
  entry: SubtitleClipResultSegment,
  index: number,
  subtitleStyle: SubtitleSegment['style'],
  transform?: AudioSubtitleClipTransform,
  sourceTypeForNative: 'video' | 'mic' | 'audio' = 'video',
): SubtitleSegment {
  if (!transform) {
    return {
      id: buildSubtitleId(result.clipId, entry, index),
      startTime: entry.startTime,
      endTime: entry.endTime,
      text: entry.text,
      style: subtitleStyle,
      splitGroupId: entry.splitGroupId,
      splitGroupIndex: entry.splitGroupIndex,
      splitGroupCount: entry.splitGroupCount,
      splitGroupText: entry.splitGroupText,
      splitGroupStartTime: entry.splitGroupStartTime,
      splitGroupEndTime: entry.splitGroupEndTime,
      sourceGroup: {
        kind: sourceTypeForNative === 'mic'
          ? 'mic'
          : sourceTypeForNative === 'audio'
            ? 'audio'
            : 'video',
        assignment: 'generated',
      },
    };
  }

  const sourceLocalStartTime = Math.max(0, entry.startTime - transform.sourceLocalOffsetSec);
  const sourceLocalEndTime = Math.max(sourceLocalStartTime, entry.endTime - transform.sourceLocalOffsetSec);
  return {
    id: buildSubtitleId(result.clipId, {
      ...entry,
      startTime: entry.startTime + transform.timelineOffsetSec,
    }, index),
    startTime: entry.startTime + transform.timelineOffsetSec,
    endTime: entry.endTime + transform.timelineOffsetSec,
    text: entry.text,
    style: subtitleStyle,
    splitGroupId: entry.splitGroupId,
    splitGroupIndex: entry.splitGroupIndex,
    splitGroupCount: entry.splitGroupCount,
    splitGroupText: entry.splitGroupText,
    splitGroupStartTime: entry.splitGroupStartTime !== undefined
      ? entry.splitGroupStartTime + transform.timelineOffsetSec
      : undefined,
    splitGroupEndTime: entry.splitGroupEndTime !== undefined
      ? entry.splitGroupEndTime + transform.timelineOffsetSec
      : undefined,
    provenance: {
      sourceKind: 'audio',
      audioSegmentId: transform.audioSegmentId,
      sourceName: transform.sourceName,
      sourcePath: transform.sourcePath,
      sourceLocalStartTime,
      sourceLocalEndTime,
    },
    sourceGroup: {
      kind: 'audio',
      assignment: 'generated',
      audioSegmentId: transform.audioSegmentId,
      sourceName: transform.sourceName,
      sourcePath: transform.sourcePath,
    },
  };
}
