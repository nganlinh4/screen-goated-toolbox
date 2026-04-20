import {
  buildSequenceTimeline,
  getSequenceClipById,
  type SequenceTimelineClip,
} from '@/lib/sequenceTimeline';
import { getEffectiveCompositionMode } from '@/lib/projectComposition';
import { getTrimSegments } from '@/lib/trimSegments';
import type { TrackSelectionRange } from '@/lib/timelineSegmentSelection';
import type { ProjectComposition, VideoSegment } from '@/types/video';

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
  sourceType: 'video' | 'mic';
  selectedRange?: TrackSelectionRange | null;
}): SubtitleGenerationPlan {
  const indicator: SubtitleGenerationIndicator = {
    mode: params.selectedRange ? 'range' : 'full',
    range: params.selectedRange ?? null,
  };
  const effectiveMode = getEffectiveCompositionMode(params.composition);

  if (!params.composition || effectiveMode === 'separate') {
    if (!params.segment) {
      return { clips: [], replacementRangesByClip: {}, indicator };
    }
    const sourcePath =
      params.sourceType === 'mic'
        ? params.currentRawMicAudioPath
        : params.currentRawVideoPath;
    if (!sourcePath) {
      return { clips: [], replacementRangesByClip: {}, indicator };
    }
    return buildSingleClipPayload({
      clipId: params.activeClipId ?? 'root',
      clipName: 'Current Clip',
      sourcePath,
      sourceDuration: params.duration,
      segment: params.segment,
      selectedRange: params.selectedRange,
    });
  }

  const timeline = buildSequenceTimeline(params.composition);
  if (!timeline) {
    return { clips: [], replacementRangesByClip: {}, indicator };
  }

  const replacementRangesByClip: SubtitleGenerationPlan['replacementRangesByClip'] = {};
  const clips = timeline.clips.flatMap((timelineClip) => {
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
  };
}

export function getSequenceReplacementRanges(
  composition: ProjectComposition | null,
  clipId: string,
  ranges: Array<{ startTime: number; endTime: number }>,
) {
  const timeline = buildSequenceTimeline(composition);
  const timelineClip = getSequenceClipById(timeline, clipId);
  if (!timelineClip) return [];
  let compactCursor = 0;
  return getTrimSegments(
    timelineClip.clip.segment,
    timelineClip.sourceDuration,
  )
    .map((trimSegment) => {
      const segmentCompactStart = compactCursor;
      const segmentCompactEnd =
        segmentCompactStart + (trimSegment.endTime - trimSegment.startTime);
      compactCursor = segmentCompactEnd;
      return ranges
        .map((range) => {
          const overlapStart = Math.max(range.startTime, trimSegment.startTime);
          const overlapEnd = Math.min(range.endTime, trimSegment.endTime);
          if (overlapEnd - overlapStart <= 0.0001) {
            return null;
          }
          return {
            startTime:
              timelineClip.sequenceStart +
              segmentCompactStart +
              (overlapStart - trimSegment.startTime),
            endTime:
              timelineClip.sequenceStart +
              segmentCompactStart +
              (overlapEnd - trimSegment.startTime),
          };
        })
        .filter(
          (
            range,
          ): range is { startTime: number; endTime: number } => range !== null,
        );
    })
    .flat();
}
