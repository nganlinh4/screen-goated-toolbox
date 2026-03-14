import type {
  ProjectComposition,
  ProjectCompositionClip,
  VideoSegment,
} from "@/types/video";
import {
  getTotalTrimDuration,
  getTrimSegments,
  toCompactTime,
  toSourceTime,
} from "@/lib/trimSegments";

export interface SequenceTimelineClip {
  clipId: string;
  clip: ProjectCompositionClip;
  sourceDuration: number;
  activeDuration: number;
  sequenceStart: number;
  sequenceEnd: number;
}

export interface SequenceTimelineModel {
  clips: SequenceTimelineClip[];
  clipById: Record<string, SequenceTimelineClip>;
  totalDuration: number;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

export function getClipSourceDuration(clip: ProjectCompositionClip): number {
  return Math.max(
    0,
    clip.duration || 0,
    clip.segment.trimEnd || 0,
    ...(clip.segment.trimSegments ?? []).map((segment) => segment.endTime),
  );
}

export function buildSequenceTimeline(
  composition: ProjectComposition | null | undefined,
): SequenceTimelineModel | null {
  if (!composition) return null;

  let sequenceCursor = 0;
  const clips: SequenceTimelineClip[] = composition.clips.map((clip) => {
    const sourceDuration = Math.max(getClipSourceDuration(clip), 0.001);
    const activeDuration = Math.max(
      getTotalTrimDuration(clip.segment, sourceDuration),
      0.001,
    );
    const sequenceStart = sequenceCursor;
    const sequenceEnd = sequenceStart + activeDuration;
    sequenceCursor = sequenceEnd;
    return {
      clipId: clip.id,
      clip,
      sourceDuration,
      activeDuration,
      sequenceStart,
      sequenceEnd,
    };
  });

  return {
    clips,
    clipById: Object.fromEntries(clips.map((clip) => [clip.clipId, clip])),
    totalDuration: sequenceCursor,
  };
}

export function getSequenceClipById(
  timeline: SequenceTimelineModel | null | undefined,
  clipId: string | null | undefined,
): SequenceTimelineClip | null {
  if (!timeline || !clipId) return null;
  return timeline.clipById[clipId] ?? null;
}

export function findSequenceClipAtTime(
  timeline: SequenceTimelineModel | null | undefined,
  time: number,
): SequenceTimelineClip | null {
  if (!timeline || timeline.clips.length === 0) return null;
  const clampedTime = clamp(
    time,
    0,
    Math.max(0, timeline.totalDuration - 0.0001),
  );
  return (
    timeline.clips.find(
      (clip) =>
        clampedTime >= clip.sequenceStart && clampedTime < clip.sequenceEnd,
    ) ?? timeline.clips[timeline.clips.length - 1]
  );
}

export function sequenceTimeToClipSourceTime(
  time: number,
  timelineClip: SequenceTimelineClip,
): number {
  const localCompactTime = clamp(
    time - timelineClip.sequenceStart,
    0,
    timelineClip.activeDuration,
  );
  return toSourceTime(
    localCompactTime,
    timelineClip.clip.segment,
    timelineClip.sourceDuration,
  );
}

export function clipSourceTimeToSequenceTime(
  time: number,
  timelineClip: SequenceTimelineClip,
): number {
  return (
    timelineClip.sequenceStart +
    toCompactTime(time, timelineClip.clip.segment, timelineClip.sourceDuration)
  );
}

function withSequenceBounds(
  segment: VideoSegment,
  duration: number,
): VideoSegment {
  return {
    ...segment,
    trimStart: 0,
    trimEnd: duration,
    trimSegments: [
      {
        id: "sequence-full",
        startTime: 0,
        endTime: duration,
      },
    ],
  };
}

export function projectClipSegmentToSequence(
  segment: VideoSegment,
  timelineClip: SequenceTimelineClip,
  sequenceDuration: number,
): VideoSegment {
  const projectTime = (time: number) =>
    clipSourceTimeToSequenceTime(time, timelineClip);

  return withSequenceBounds(
    {
      ...segment,
      trimStart: timelineClip.sequenceStart,
      trimEnd: timelineClip.sequenceEnd,
      trimSegments: getTrimSegments(segment, timelineClip.sourceDuration).map(
        (trimSegment) => ({
          ...trimSegment,
          startTime: projectTime(trimSegment.startTime),
          endTime: projectTime(trimSegment.endTime),
        }),
      ),
      zoomKeyframes: segment.zoomKeyframes.map((keyframe) => ({
        ...keyframe,
        time: projectTime(keyframe.time),
      })),
      smoothMotionPath: segment.smoothMotionPath?.map((point) => ({
        ...point,
        time: projectTime(point.time),
      })),
      zoomInfluencePoints: segment.zoomInfluencePoints?.map((point) => ({
        ...point,
        time: projectTime(point.time),
      })),
      textSegments: segment.textSegments.map((textSegment) => ({
        ...textSegment,
        startTime: projectTime(textSegment.startTime),
        endTime: projectTime(textSegment.endTime),
      })),
      cursorVisibilitySegments: segment.cursorVisibilitySegments?.map(
        (range) => ({
          ...range,
          startTime: projectTime(range.startTime),
          endTime: projectTime(range.endTime),
        }),
      ),
      webcamVisibilitySegments: segment.webcamVisibilitySegments?.map(
        (range) => ({
          ...range,
          startTime: projectTime(range.startTime),
          endTime: projectTime(range.endTime),
        }),
      ),
      keystrokeEvents: segment.keystrokeEvents?.map((event) => ({
        ...event,
        startTime: projectTime(event.startTime),
        endTime: projectTime(event.endTime),
      })),
      keyboardVisibilitySegments: segment.keyboardVisibilitySegments?.map(
        (range) => ({
          ...range,
          startTime: projectTime(range.startTime),
          endTime: projectTime(range.endTime),
        }),
      ),
      keyboardMouseVisibilitySegments:
        segment.keyboardMouseVisibilitySegments?.map((range) => ({
          ...range,
          startTime: projectTime(range.startTime),
          endTime: projectTime(range.endTime),
        })),
      speedPoints: segment.speedPoints?.map((point) => ({
        ...point,
        time: projectTime(point.time),
      })),
      deviceAudioPoints: segment.deviceAudioPoints?.map((point) => ({
        ...point,
        time: projectTime(point.time),
      })),
      micAudioPoints: segment.micAudioPoints?.map((point) => ({
        ...point,
        time: projectTime(point.time),
      })),
    },
    sequenceDuration,
  );
}

export function projectSequenceSegmentToClip(
  sequenceSegment: VideoSegment,
  timelineClip: SequenceTimelineClip,
): VideoSegment {
  const toClipTime = (time: number) =>
    sequenceTimeToClipSourceTime(time, timelineClip);
  const overlapsClip = (startTime: number, endTime: number) =>
    endTime > timelineClip.sequenceStart &&
    startTime < timelineClip.sequenceEnd;

  return {
    ...sequenceSegment,
    trimStart: timelineClip.clip.segment.trimStart,
    trimEnd: timelineClip.clip.segment.trimEnd,
    trimSegments: timelineClip.clip.segment.trimSegments,
    crop: timelineClip.clip.segment.crop,
    useCustomCursor: timelineClip.clip.segment.useCustomCursor,
    zoomKeyframes: sequenceSegment.zoomKeyframes
      .filter((keyframe) => overlapsClip(keyframe.time, keyframe.time))
      .map((keyframe) => ({
        ...keyframe,
        time: toClipTime(keyframe.time),
      })),
    smoothMotionPath: sequenceSegment.smoothMotionPath
      ?.filter((point) => overlapsClip(point.time, point.time))
      .map((point) => ({
        ...point,
        time: toClipTime(point.time),
      })),
    zoomInfluencePoints: sequenceSegment.zoomInfluencePoints
      ?.filter((point) => overlapsClip(point.time, point.time))
      .map((point) => ({
        ...point,
        time: toClipTime(point.time),
      })),
    textSegments: sequenceSegment.textSegments
      .filter((textSegment) =>
        overlapsClip(textSegment.startTime, textSegment.endTime),
      )
      .map((textSegment) => ({
        ...textSegment,
        startTime: toClipTime(textSegment.startTime),
        endTime: toClipTime(textSegment.endTime),
      })),
    cursorVisibilitySegments: sequenceSegment.cursorVisibilitySegments
      ?.filter((range) => overlapsClip(range.startTime, range.endTime))
      .map((range) => ({
        ...range,
        startTime: toClipTime(range.startTime),
        endTime: toClipTime(range.endTime),
      })),
    webcamVisibilitySegments: sequenceSegment.webcamVisibilitySegments
      ?.filter((range) => overlapsClip(range.startTime, range.endTime))
      .map((range) => ({
        ...range,
        startTime: toClipTime(range.startTime),
        endTime: toClipTime(range.endTime),
      })),
    keystrokeEvents: sequenceSegment.keystrokeEvents
      ?.filter((event) => overlapsClip(event.startTime, event.endTime))
      .map((event) => ({
        ...event,
        startTime: toClipTime(event.startTime),
        endTime: toClipTime(event.endTime),
      })),
    keyboardVisibilitySegments: sequenceSegment.keyboardVisibilitySegments
      ?.filter((range) => overlapsClip(range.startTime, range.endTime))
      .map((range) => ({
        ...range,
        startTime: toClipTime(range.startTime),
        endTime: toClipTime(range.endTime),
      })),
    keyboardMouseVisibilitySegments:
      sequenceSegment.keyboardMouseVisibilitySegments
        ?.filter((range) => overlapsClip(range.startTime, range.endTime))
        .map((range) => ({
          ...range,
          startTime: toClipTime(range.startTime),
          endTime: toClipTime(range.endTime),
        })),
    speedPoints: sequenceSegment.speedPoints
      ?.filter((point) => overlapsClip(point.time, point.time))
      .map((point) => ({
        ...point,
        time: toClipTime(point.time),
      })),
    deviceAudioPoints: sequenceSegment.deviceAudioPoints
      ?.filter((point) => overlapsClip(point.time, point.time))
      .map((point) => ({
        ...point,
        time: toClipTime(point.time),
      })),
    micAudioPoints: sequenceSegment.micAudioPoints
      ?.filter((point) => overlapsClip(point.time, point.time))
      .map((point) => ({
        ...point,
        time: toClipTime(point.time),
      })),
  };
}

export function mergeCompositionSegmentsToSequence(
  timeline: SequenceTimelineModel,
): VideoSegment {
  const merged: VideoSegment = {
    trimStart: 0,
    trimEnd: timeline.totalDuration,
    trimSegments: [
      {
        id: "sequence-full",
        startTime: 0,
        endTime: timeline.totalDuration,
      },
    ],
    zoomKeyframes: [],
    smoothMotionPath: [],
    zoomInfluencePoints: [],
    textSegments: [],
    cursorVisibilitySegments: [],
    webcamVisibilitySegments: [],
    keystrokeMode: "keyboardMouse",
    keystrokeLanguage: "en",
    keystrokeDelaySec: 0,
    keystrokeEvents: [],
    keyboardVisibilitySegments: [],
    keyboardMouseVisibilitySegments: [],
    speedPoints: [],
    deviceAudioPoints: [],
    micAudioPoints: [],
    deviceAudioAvailable: false,
    micAudioAvailable: false,
    useCustomCursor: true,
  };

  for (const timelineClip of timeline.clips) {
    const projected = projectClipSegmentToSequence(
      timelineClip.clip.segment,
      timelineClip,
      timeline.totalDuration,
    );
    merged.zoomKeyframes.push(...projected.zoomKeyframes);
    merged.smoothMotionPath?.push(...(projected.smoothMotionPath ?? []));
    merged.zoomInfluencePoints?.push(...(projected.zoomInfluencePoints ?? []));
    merged.textSegments.push(...projected.textSegments);
    merged.cursorVisibilitySegments?.push(
      ...(projected.cursorVisibilitySegments ?? []),
    );
    merged.webcamVisibilitySegments?.push(
      ...(projected.webcamVisibilitySegments ?? []),
    );
    merged.keystrokeEvents?.push(...(projected.keystrokeEvents ?? []));
    merged.keyboardVisibilitySegments?.push(
      ...(projected.keyboardVisibilitySegments ?? []),
    );
    merged.keyboardMouseVisibilitySegments?.push(
      ...(projected.keyboardMouseVisibilitySegments ?? []),
    );
    merged.speedPoints?.push(...(projected.speedPoints ?? []));
    merged.deviceAudioPoints?.push(...(projected.deviceAudioPoints ?? []));
    merged.micAudioPoints?.push(...(projected.micAudioPoints ?? []));
    merged.deviceAudioAvailable =
      merged.deviceAudioAvailable || projected.deviceAudioAvailable !== false;
    merged.micAudioAvailable =
      merged.micAudioAvailable || projected.micAudioAvailable === true;
  }

  if ((merged.speedPoints?.length ?? 0) === 0) {
    merged.speedPoints = [
      { time: 0, speed: 1 },
      { time: timeline.totalDuration, speed: 1 },
    ];
  }
  if ((merged.deviceAudioPoints?.length ?? 0) === 0) {
    merged.deviceAudioPoints = [
      { time: 0, volume: 1 },
      { time: timeline.totalDuration, volume: 1 },
    ];
  }
  if ((merged.micAudioPoints?.length ?? 0) === 0) {
    merged.micAudioPoints = [
      { time: 0, volume: 0 },
      { time: timeline.totalDuration, volume: 0 },
    ];
  }

  return merged;
}

export function replaceSequenceClipSegmentInGlobal(
  globalSegment: VideoSegment,
  clipSegment: VideoSegment,
  timelineClip: SequenceTimelineClip,
  sequenceDuration: number,
): VideoSegment {
  const projectedClipSegment = projectClipSegmentToSequence(
    clipSegment,
    timelineClip,
    sequenceDuration,
  );
  const overlapsClip = (startTime: number, endTime: number) =>
    endTime > timelineClip.sequenceStart &&
    startTime < timelineClip.sequenceEnd;

  return {
    ...globalSegment,
    trimStart: 0,
    trimEnd: sequenceDuration,
    trimSegments: [
      {
        id: "sequence-full",
        startTime: 0,
        endTime: sequenceDuration,
      },
    ],
    zoomKeyframes: [
      ...globalSegment.zoomKeyframes.filter(
        (keyframe) => !overlapsClip(keyframe.time, keyframe.time),
      ),
      ...projectedClipSegment.zoomKeyframes,
    ].sort((a, b) => a.time - b.time),
    smoothMotionPath: [
      ...(globalSegment.smoothMotionPath ?? []).filter(
        (point) => !overlapsClip(point.time, point.time),
      ),
      ...(projectedClipSegment.smoothMotionPath ?? []),
    ].sort((a, b) => a.time - b.time),
    zoomInfluencePoints: [
      ...(globalSegment.zoomInfluencePoints ?? []).filter(
        (point) => !overlapsClip(point.time, point.time),
      ),
      ...(projectedClipSegment.zoomInfluencePoints ?? []),
    ].sort((a, b) => a.time - b.time),
    textSegments: [
      ...globalSegment.textSegments.filter(
        (textSegment) =>
          !overlapsClip(textSegment.startTime, textSegment.endTime),
      ),
      ...projectedClipSegment.textSegments,
    ].sort((a, b) => a.startTime - b.startTime),
    cursorVisibilitySegments: [
      ...(globalSegment.cursorVisibilitySegments ?? []).filter(
        (range) => !overlapsClip(range.startTime, range.endTime),
      ),
      ...(projectedClipSegment.cursorVisibilitySegments ?? []),
    ].sort((a, b) => a.startTime - b.startTime),
    webcamVisibilitySegments: [
      ...(globalSegment.webcamVisibilitySegments ?? []).filter(
        (range) => !overlapsClip(range.startTime, range.endTime),
      ),
      ...(projectedClipSegment.webcamVisibilitySegments ?? []),
    ].sort((a, b) => a.startTime - b.startTime),
    keystrokeEvents: [
      ...(globalSegment.keystrokeEvents ?? []).filter(
        (event) => !overlapsClip(event.startTime, event.endTime),
      ),
      ...(projectedClipSegment.keystrokeEvents ?? []),
    ].sort((a, b) => a.startTime - b.startTime),
    keyboardVisibilitySegments: [
      ...(globalSegment.keyboardVisibilitySegments ?? []).filter(
        (range) => !overlapsClip(range.startTime, range.endTime),
      ),
      ...(projectedClipSegment.keyboardVisibilitySegments ?? []),
    ].sort((a, b) => a.startTime - b.startTime),
    keyboardMouseVisibilitySegments: [
      ...(globalSegment.keyboardMouseVisibilitySegments ?? []).filter(
        (range) => !overlapsClip(range.startTime, range.endTime),
      ),
      ...(projectedClipSegment.keyboardMouseVisibilitySegments ?? []),
    ].sort((a, b) => a.startTime - b.startTime),
    speedPoints: [
      ...(globalSegment.speedPoints ?? []).filter(
        (point) => !overlapsClip(point.time, point.time),
      ),
      ...(projectedClipSegment.speedPoints ?? []),
    ].sort((a, b) => a.time - b.time),
    deviceAudioPoints: [
      ...(globalSegment.deviceAudioPoints ?? []).filter(
        (point) => !overlapsClip(point.time, point.time),
      ),
      ...(projectedClipSegment.deviceAudioPoints ?? []),
    ].sort((a, b) => a.time - b.time),
    micAudioPoints: [
      ...(globalSegment.micAudioPoints ?? []).filter(
        (point) => !overlapsClip(point.time, point.time),
      ),
      ...(projectedClipSegment.micAudioPoints ?? []),
    ].sort((a, b) => a.time - b.time),
  };
}
