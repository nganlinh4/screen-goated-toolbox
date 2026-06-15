import {
  buildSequenceTimeline,
  type SequenceTimelineClip,
} from '@/lib/sequenceTimeline';
import { getEffectiveCompositionMode } from '@/lib/projectComposition';
import { getTrimSegments } from '@/lib/trimSegments';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import type { ImportedAudioSegment, ProjectComposition, VideoSegment } from '@/types/video';

export type SubtitleSource = 'video' | 'mic' | 'audio' | `audio:${string}`;

export interface AudioSubtitleClipTransform {
  kind: 'audio';
  audioSegmentId: string;
  sourceName: string;
  sourcePath: string;
  timelineOffsetSec: number;
  sourceLocalOffsetSec: number;
}

export interface SubtitleClipPayload {
  clipId: string;
  clipName: string;
  sourcePath: string;
  sourceDuration: number;
  trimSegments: Array<{ id: string; startTime: number; endTime: number }>;
  micAudioOffsetSec?: number;
}

export interface SubtitleGenerationIndicator {
  mode: 'full' | 'range';
  range: TrackSelectionRange | null;
}

export interface SubtitleGenerationPlan {
  clips: SubtitleClipPayload[];
  replacementRangesByClip: Record<
    string,
    Array<{ startTime: number; endTime: number }>
  >;
  indicator: SubtitleGenerationIndicator;
  sourceTypeForNative: 'video' | 'mic' | 'audio';
  clipTransformsByClip: Record<string, AudioSubtitleClipTransform>;
}

export function nativeSubtitleSourceType(source: SubtitleSource): 'video' | 'mic' | 'audio' {
  if (source === 'video' || source === 'mic') return source;
  return 'audio';
}

function intersectSourceRangeWithTrimSegments(
  segment: VideoSegment,
  duration: number,
  range: Pick<TrackSelectionRange, 'startTime' | 'endTime'>,
): Array<{ startTime: number; endTime: number }> {
  const start = Math.min(range.startTime, range.endTime);
  const end = Math.max(range.startTime, range.endTime);
  return getTrimSegments(segment, duration)
    .map((trimSegment) => ({
      startTime: Math.max(start, trimSegment.startTime),
      endTime: Math.min(end, trimSegment.endTime),
    }))
    .filter((trimSegment) => trimSegment.endTime - trimSegment.startTime > 0.0001);
}

function compactRangeToSourceRanges(
  compactStart: number,
  compactEnd: number,
  segment: VideoSegment,
  duration: number,
): Array<{ startTime: number; endTime: number }> {
  let compactCursor = 0;
  return getTrimSegments(segment, duration)
    .map((trimSegment) => {
      const segmentCompactStart = compactCursor;
      const segmentCompactEnd =
        segmentCompactStart + (trimSegment.endTime - trimSegment.startTime);
      compactCursor = segmentCompactEnd;
      const overlapStart = Math.max(compactStart, segmentCompactStart);
      const overlapEnd = Math.min(compactEnd, segmentCompactEnd);
      if (overlapEnd - overlapStart <= 0.0001) {
        return null;
      }
      return {
        startTime: trimSegment.startTime + (overlapStart - segmentCompactStart),
        endTime: trimSegment.startTime + (overlapEnd - segmentCompactStart),
      };
    })
    .filter(
      (
        trimSegment,
      ): trimSegment is { startTime: number; endTime: number } => trimSegment !== null,
    );
}

function toPayloadTrimSegments(
  ranges: Array<{ startTime: number; endTime: number }>,
) {
  return ranges.map((trimSegment, index) => ({
    id: `subtitle-range-${index}`,
    startTime: trimSegment.startTime,
    endTime: trimSegment.endTime,
  }));
}

function buildSingleClipPayload(params: {
  clipId: string;
  clipName: string;
  sourcePath: string;
  sourceDuration: number;
  segment: VideoSegment;
  selectedRange: TrackSelectionRange | null | undefined;
  sourceTypeForNative?: 'video' | 'mic' | 'audio';
}): SubtitleGenerationPlan {
  const replacementRanges = params.selectedRange
    ? intersectSourceRangeWithTrimSegments(
        params.segment,
        params.sourceDuration,
        params.selectedRange,
      )
    : getTrimSegments(params.segment, params.sourceDuration).map((trimSegment) => ({
        startTime: trimSegment.startTime,
        endTime: trimSegment.endTime,
      }));

  return {
    clips:
      replacementRanges.length > 0
        ? [
            {
              clipId: params.clipId,
              clipName: params.clipName,
              sourcePath: params.sourcePath,
              sourceDuration: params.sourceDuration,
              trimSegments: toPayloadTrimSegments(replacementRanges),
              micAudioOffsetSec: params.segment.micAudioOffsetSec,
            },
          ]
        : [],
    replacementRangesByClip:
      replacementRanges.length > 0
        ? { [params.clipId]: replacementRanges }
        : {},
    indicator: {
      mode: params.selectedRange ? 'range' : 'full',
      range: params.selectedRange ?? null,
    },
    sourceTypeForNative: params.sourceTypeForNative ?? 'video',
    clipTransformsByClip: {},
  };
}

function emptyPlan(
  indicator: SubtitleGenerationIndicator,
  sourceType: SubtitleSource,
): SubtitleGenerationPlan {
  return {
    clips: [],
    replacementRangesByClip: {},
    indicator,
    sourceTypeForNative: nativeSubtitleSourceType(sourceType),
    clipTransformsByClip: {},
  };
}

function buildMusicClipPayload(
  segment: ImportedAudioSegment,
  selectedRange: TrackSelectionRange | null | undefined,
) {
  const inPoint = Math.max(0, Math.min(segment.inPoint, segment.duration));
  const outPoint = Math.max(inPoint, Math.min(segment.outPoint, segment.duration));
  const visibleDuration = outPoint - inPoint;
  if (!segment.rawAudioPath || visibleDuration <= 0.0001) return null;

  const timelineStart = segment.startTime;
  const timelineEnd = timelineStart + visibleDuration;
  const selectedStart = selectedRange
    ? Math.min(selectedRange.startTime, selectedRange.endTime)
    : timelineStart;
  const selectedEnd = selectedRange
    ? Math.max(selectedRange.startTime, selectedRange.endTime)
    : timelineEnd;
  if (selectedEnd <= timelineStart || selectedStart >= timelineEnd) return null;

  const sourceStart = inPoint + Math.max(0, selectedStart - timelineStart);
  const sourceEnd = inPoint + Math.min(visibleDuration, selectedEnd - timelineStart);
  const clampedStart = Math.max(inPoint, Math.min(sourceStart, outPoint));
  const clampedEnd = Math.max(clampedStart, Math.min(sourceEnd, outPoint));
  if (clampedEnd - clampedStart <= 0.0001) return null;

  const clipId = `audio:${segment.id}`;
  return {
    clip: {
      clipId,
      clipName: segment.name || 'Audio',
      sourcePath: segment.rawAudioPath,
      sourceDuration: segment.duration,
      trimSegments: toPayloadTrimSegments([
        { startTime: clampedStart, endTime: clampedEnd },
      ]),
    },
    replacementRanges: [
      {
        startTime: clampedStart + segment.startTime - inPoint,
        endTime: clampedEnd + segment.startTime - inPoint,
      },
    ],
    transform: {
      kind: 'audio' as const,
      audioSegmentId: segment.id,
      sourceName: segment.name || 'Audio',
      sourcePath: segment.rawAudioPath,
      timelineOffsetSec: segment.startTime - inPoint,
      sourceLocalOffsetSec: inPoint,
    },
  };
}

function buildUnifiedClipPayload(params: {
  timelineClip: SequenceTimelineClip;
  sourcePath: string;
  selectedRange: TrackSelectionRange | null | undefined;
}) {
  const fullRanges = getTrimSegments(
    params.timelineClip.clip.segment,
    params.timelineClip.sourceDuration,
  ).map((trimSegment) => ({
    startTime: trimSegment.startTime,
    endTime: trimSegment.endTime,
  }));

  const replacementRanges = params.selectedRange
    ? compactRangeToSourceRanges(
        Math.max(
          0,
          params.selectedRange.startTime - params.timelineClip.sequenceStart,
        ),
        Math.max(
          0,
          params.selectedRange.endTime - params.timelineClip.sequenceStart,
        ),
        params.timelineClip.clip.segment,
        params.timelineClip.sourceDuration,
      )
    : fullRanges;

  if (replacementRanges.length === 0) {
    return null;
  }

  return {
    clip: {
      clipId: params.timelineClip.clip.id,
      clipName: params.timelineClip.clip.name,
      sourcePath: params.sourcePath,
      sourceDuration: params.timelineClip.sourceDuration,
      trimSegments: toPayloadTrimSegments(replacementRanges),
      micAudioOffsetSec: params.timelineClip.clip.segment.micAudioOffsetSec,
    },
    replacementRanges,
  };
}

export function buildSubtitleGenerationPlan(params: {
  segment: VideoSegment | null;
  composition: ProjectComposition | null;
  activeClipId: string | null | undefined;
  currentRawVideoPath: string;
  currentRawMicAudioPath: string;
  duration: number;
  sourceType: SubtitleSource;
  selectedRange?: TrackSelectionRange | null;
}): SubtitleGenerationPlan {
  const indicator: SubtitleGenerationIndicator = {
    mode: params.selectedRange ? 'range' : 'full',
    range: params.selectedRange ?? null,
  };

  if (params.sourceType === 'audio' || params.sourceType.startsWith('audio:')) {
    const segments = params.composition?.audioSegments ?? [];
    const requestedId = params.sourceType.startsWith('audio:')
      ? params.sourceType.slice('audio:'.length)
      : null;
    const payloads = segments
      .filter((segment) => !requestedId || segment.id === requestedId)
      .slice()
      .sort((left, right) => left.startTime - right.startTime)
      .map((segment) => buildMusicClipPayload(segment, params.selectedRange))
      .filter((payload): payload is NonNullable<typeof payload> => payload !== null);

    return {
      clips: payloads.map((payload) => payload.clip),
      replacementRangesByClip: Object.fromEntries(
        payloads.map((payload) => [payload.clip.clipId, payload.replacementRanges]),
      ),
      indicator,
      sourceTypeForNative: 'audio',
      clipTransformsByClip: Object.fromEntries(
        payloads.map((payload) => [payload.clip.clipId, payload.transform]),
      ),
    };
  }

  const effectiveMode = getEffectiveCompositionMode(params.composition);

  if (!params.composition || effectiveMode === 'separate') {
    if (!params.segment) {
      return emptyPlan(indicator, params.sourceType);
    }
    const sourcePath =
      params.sourceType === 'mic'
        ? params.currentRawMicAudioPath
        : params.currentRawVideoPath;
    if (!sourcePath) {
      return emptyPlan(indicator, params.sourceType);
    }
    return buildSingleClipPayload({
      clipId: params.activeClipId ?? 'root',
      clipName: 'Current Clip',
      sourcePath,
      sourceDuration: params.duration,
      segment: params.segment,
      selectedRange: params.selectedRange,
      sourceTypeForNative: nativeSubtitleSourceType(params.sourceType),
    });
  }

  const timeline = buildSequenceTimeline(params.composition);
  if (!timeline) {
    return emptyPlan(indicator, params.sourceType);
  }

  const replacementRangesByClip: SubtitleGenerationPlan['replacementRangesByClip'] = {};
  const clips = timeline.clips.flatMap((timelineClip) => {
    // 'audio' source was already handled at the top of this function.
    const sourcePath =
      params.sourceType === 'mic'
        ? timelineClip.clip.rawMicAudioPath ?? ''
        : timelineClip.clip.rawVideoPath ?? '';
    if (!sourcePath) {
      return [];
    }

    if (!params.selectedRange) {
      const fullRanges = getTrimSegments(
        timelineClip.clip.segment,
        timelineClip.sourceDuration,
      ).map((trimSegment) => ({
        startTime: trimSegment.startTime,
        endTime: trimSegment.endTime,
      }));
      replacementRangesByClip[timelineClip.clip.id] = fullRanges;
      return [
        {
          clipId: timelineClip.clip.id,
          clipName: timelineClip.clip.name,
          sourcePath,
          sourceDuration: timelineClip.sourceDuration,
          trimSegments: toPayloadTrimSegments(fullRanges),
          micAudioOffsetSec: timelineClip.clip.segment.micAudioOffsetSec,
        },
      ];
    }

    const overlapStart = Math.max(
      Math.min(params.selectedRange.startTime, params.selectedRange.endTime),
      timelineClip.sequenceStart,
    );
    const overlapEnd = Math.min(
      Math.max(params.selectedRange.startTime, params.selectedRange.endTime),
      timelineClip.sequenceEnd,
    );
    if (overlapEnd - overlapStart <= 0.0001) {
      return [];
    }
    const payload = buildUnifiedClipPayload({
      timelineClip,
      sourcePath,
      selectedRange: {
        ...params.selectedRange,
        startTime: overlapStart,
        endTime: overlapEnd,
      },
    });
    if (!payload) {
      return [];
    }
    replacementRangesByClip[timelineClip.clip.id] = payload.replacementRanges;
    return [payload.clip];
  });

  return {
    clips,
    replacementRangesByClip,
    indicator,
    sourceTypeForNative: nativeSubtitleSourceType(params.sourceType),
    clipTransformsByClip: {},
  };
}

