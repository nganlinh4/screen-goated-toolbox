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
import {
  appendProjectedSubtitleTrackState,
  filterSubtitleTrackState,
  projectSubtitleTrackState,
  replaceProjectedSubtitleTrackState,
} from "@/lib/sequenceSubtitleTracks";
import { createSubtitleTrackStateFromSegments, normalizeSubtitleTrackState } from "@/lib/subtitleTracks";
import { clamp } from "@/lib/mathUtils";

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

/**
 * Declarative table of the time-bearing array fields that must be carried
 * (offset / filtered / merged) when projecting a segment between clip-space
 * and sequence-space. Adding a new time-bearing field here makes ALL four
 * projection functions handle it uniformly, so a field can no longer be
 * carried in one direction and silently dropped in another.
 *
 * Subtitle track state (subtitleTracks/activeSubtitleView/subtitleCustomChain/
 * subtitleSegments) is intentionally NOT listed here — it requires per-track
 * merge semantics and is handled separately via sequenceSubtitleTracks.ts.
 *
 * `zoomKeyframes` is the deprecated point-keyframe model; `zoomBlocks` is
 * intentionally NOT projected by these functions (matching pre-refactor
 * behavior), so it is excluded from this table.
 */
type PointTimeField =
  | "zoomKeyframes"
  | "smoothMotionPath"
  | "zoomInfluencePoints"
  | "speedPoints"
  | "deviceAudioPoints"
  | "micAudioPoints";

type StartEndTimeField =
  | "textSegments"
  | "cursorVisibilitySegments"
  | "webcamVisibilitySegments"
  | "keystrokeEvents"
  | "keyboardVisibilitySegments"
  | "keyboardMouseVisibilitySegments";

type TimeFieldName = PointTimeField | StartEndTimeField;

type TimeFieldDescriptor =
  | { field: PointTimeField; kind: "point" }
  | { field: StartEndTimeField; kind: "startEnd" };

const TIME_FIELDS: readonly TimeFieldDescriptor[] = [
  { field: "zoomKeyframes", kind: "point" },
  { field: "smoothMotionPath", kind: "point" },
  { field: "zoomInfluencePoints", kind: "point" },
  { field: "textSegments", kind: "startEnd" },
  { field: "cursorVisibilitySegments", kind: "startEnd" },
  { field: "webcamVisibilitySegments", kind: "startEnd" },
  { field: "keystrokeEvents", kind: "startEnd" },
  { field: "keyboardVisibilitySegments", kind: "startEnd" },
  { field: "keyboardMouseVisibilitySegments", kind: "startEnd" },
  { field: "speedPoints", kind: "point" },
  { field: "deviceAudioPoints", kind: "point" },
  { field: "micAudioPoints", kind: "point" },
];

/** Minimal shape of a point-keyed element (e.g. zoom keyframes, audio points). */
type PointItem = { time: number };
/** Minimal shape of a start/end-keyed element (e.g. text/visibility ranges). */
type StartEndItem = { startTime: number; endTime: number };

/** Read a time field array off a segment, typed by descriptor kind. */
function readTimeField(
  segment: VideoSegment,
  descriptor: { field: PointTimeField; kind: "point" },
): PointItem[] | undefined;
function readTimeField(
  segment: VideoSegment,
  descriptor: { field: StartEndTimeField; kind: "startEnd" },
): StartEndItem[] | undefined;
function readTimeField(
  segment: VideoSegment,
  descriptor: TimeFieldDescriptor,
): (PointItem | StartEndItem)[] | undefined;
function readTimeField(
  segment: VideoSegment,
  descriptor: TimeFieldDescriptor,
): (PointItem | StartEndItem)[] | undefined {
  return segment[descriptor.field] as
    | (PointItem | StartEndItem)[]
    | undefined;
}

/** Offset a single element's time key(s) using the supplied mapping. */
function offsetItem<T extends PointItem | StartEndItem>(
  descriptor: TimeFieldDescriptor,
  item: T,
  mapTime: (time: number) => number,
): T {
  if (descriptor.kind === "point") {
    return { ...item, time: mapTime((item as PointItem).time) };
  }
  const range = item as StartEndItem;
  return {
    ...item,
    startTime: mapTime(range.startTime),
    endTime: mapTime(range.endTime),
  };
}

/** Does an element overlap the clip window, per descriptor kind. */
function itemOverlaps(
  descriptor: TimeFieldDescriptor,
  item: PointItem | StartEndItem,
  overlapsClip: (startTime: number, endTime: number) => boolean,
): boolean {
  if (descriptor.kind === "point") {
    const t = (item as PointItem).time;
    return overlapsClip(t, t);
  }
  const range = item as StartEndItem;
  return overlapsClip(range.startTime, range.endTime);
}

/** Sort comparator by the descriptor's primary time key (ascending). */
function compareByTime(
  descriptor: TimeFieldDescriptor,
  a: PointItem | StartEndItem,
  b: PointItem | StartEndItem,
): number {
  if (descriptor.kind === "point") {
    return (a as PointItem).time - (b as PointItem).time;
  }
  return (a as StartEndItem).startTime - (b as StartEndItem).startTime;
}

/**
 * Map + offset a time field. Preserves the source's undefined-vs-array
 * shape exactly: an undefined source yields undefined (matching `?.map`),
 * a present array yields a mapped array.
 */
function mapTimeField(
  segment: VideoSegment,
  descriptor: TimeFieldDescriptor,
  mapTime: (time: number) => number,
): (PointItem | StartEndItem)[] | undefined {
  const items = readTimeField(segment, descriptor);
  return items?.map((item) => offsetItem(descriptor, item, mapTime));
}

/**
 * Filter to clip overlap, then offset into clip time. Preserves the source's
 * undefined-vs-array shape exactly.
 */
function filterMapTimeField(
  segment: VideoSegment,
  descriptor: TimeFieldDescriptor,
  overlapsClip: (startTime: number, endTime: number) => boolean,
  mapTime: (time: number) => number,
): (PointItem | StartEndItem)[] | undefined {
  const items = readTimeField(segment, descriptor);
  return items
    ?.filter((item) => itemOverlaps(descriptor, item, overlapsClip))
    .map((item) => offsetItem(descriptor, item, mapTime));
}

/**
 * Replace the overlapping window of a base field with projected items:
 * keep base items that do NOT overlap, append the projected items, sort by
 * the descriptor's time key. Mirrors the original `[...filter, ...projected].sort()`.
 */
function replaceTimeField(
  baseSegment: VideoSegment,
  projectedSegment: VideoSegment,
  descriptor: TimeFieldDescriptor,
  overlapsClip: (startTime: number, endTime: number) => boolean,
): (PointItem | StartEndItem)[] {
  const base = readTimeField(baseSegment, descriptor) ?? [];
  const projected = readTimeField(projectedSegment, descriptor) ?? [];
  return [
    ...base.filter((item) => !itemOverlaps(descriptor, item, overlapsClip)),
    ...projected,
  ].sort((a, b) => compareByTime(descriptor, a, b));
}

/**
 * Assign each time field from `values` onto `target`. Values are written
 * unconditionally (including `undefined`) so the result matches the original
 * object literals, which explicitly set e.g. `smoothMotionPath: undefined`
 * when the source field is absent.
 */
function assignTimeFields(
  target: VideoSegment,
  values: Record<TimeFieldName, (PointItem | StartEndItem)[] | undefined>,
): void {
  const sink = target as unknown as Record<string, unknown>;
  for (const descriptor of TIME_FIELDS) {
    sink[descriptor.field] = values[descriptor.field];
  }
}

/** Build a per-field value map by applying `produce` to each descriptor. */
function buildTimeFieldValues(
  produce: (
    descriptor: TimeFieldDescriptor,
  ) => (PointItem | StartEndItem)[] | undefined,
): Record<TimeFieldName, (PointItem | StartEndItem)[] | undefined> {
  const values = {} as Record<
    TimeFieldName,
    (PointItem | StartEndItem)[] | undefined
  >;
  for (const descriptor of TIME_FIELDS) {
    values[descriptor.field] = produce(descriptor);
  }
  return values;
}

export function projectClipSegmentToSequence(
  segment: VideoSegment,
  timelineClip: SequenceTimelineClip,
  sequenceDuration: number,
): VideoSegment {
  const projectTime = (time: number) =>
    clipSourceTimeToSequenceTime(time, timelineClip);
  const subtitleTrackState = projectSubtitleTrackState(segment, projectTime);

  const projected: VideoSegment = {
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
  };
  // All time-bearing array fields are offset uniformly into sequence space.
  assignTimeFields(
    projected,
    buildTimeFieldValues((descriptor) =>
      mapTimeField(segment, descriptor, projectTime),
    ),
  );
  // Subtitle track state is merged separately (per-track semantics).
  Object.assign(projected, subtitleTrackState);

  return withSequenceBounds(projected, sequenceDuration);
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
  const subtitleTrackState = filterSubtitleTrackState(
    sequenceSegment,
    overlapsClip,
    toClipTime,
  );

  const projected: VideoSegment = {
    ...sequenceSegment,
    trimStart: timelineClip.clip.segment.trimStart,
    trimEnd: timelineClip.clip.segment.trimEnd,
    trimSegments: timelineClip.clip.segment.trimSegments,
    crop: timelineClip.clip.segment.crop,
    useCustomCursor: timelineClip.clip.segment.useCustomCursor,
  };
  // Each time-bearing field is filtered to the clip window, then offset to
  // clip-source time.
  assignTimeFields(
    projected,
    buildTimeFieldValues((descriptor) =>
      filterMapTimeField(sequenceSegment, descriptor, overlapsClip, toClipTime),
    ),
  );
  Object.assign(projected, subtitleTrackState);

  return projected;
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
    ...createSubtitleTrackStateFromSegments([]),
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
    // `merged` initializes every time field as `[]`, so each can be appended
    // uniformly from the (possibly undefined) projected field.
    const mergedFields = merged as unknown as Record<string, unknown>;
    for (const descriptor of TIME_FIELDS) {
      const target = mergedFields[descriptor.field] as (
        | PointItem
        | StartEndItem
      )[];
      target.push(...(readTimeField(projected, descriptor) ?? []));
    }
    Object.assign(
      merged,
      appendProjectedSubtitleTrackState(merged, projected),
    );
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

  return normalizeSubtitleTrackState(merged);
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
  const subtitleTrackState = replaceProjectedSubtitleTrackState(
    globalSegment,
    projectedClipSegment,
    overlapsClip,
  );

  const result: VideoSegment = {
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
  };
  // For each time field: drop the global items overlapping this clip window,
  // splice in the projected clip items, and re-sort by time.
  assignTimeFields(
    result,
    buildTimeFieldValues((descriptor) =>
      replaceTimeField(
        globalSegment,
        projectedClipSegment,
        descriptor,
        overlapsClip,
      ),
    ),
  );
  Object.assign(result, subtitleTrackState);

  return result;
}
