import type { CursorVisibilitySegment, TrimSegment, VideoSegment } from "@/types/video";
import { clampVisibilitySegmentsToDuration, mergePointerSegments } from "@/lib/cursorHiding";
import {
  getKeystrokeVisibilitySegmentsForMode,
  withKeystrokeVisibilitySegmentsForMode,
} from "@/lib/keystrokeVisibility";
import { updateSubtitleTimingAcrossTracks } from "@/lib/subtitleTrackMutations";
import { getTrimBounds, getTrimSegments, mergeTrimSegments } from "@/lib/trimSegments";
import { clampSegmentEdge } from "./trackDragUtils";

export type SegmentEdge = "start" | "end" | "body";
type Range = { startTime: number; endTime: number };

/** Moves a whole range or one of its edges, staying inside [0, duration] like a pointer drag. */
export function nudgeRange(range: Range, edge: SegmentEdge, deltaSec: number, duration: number): Range {
  if (edge === "start") {
    return { startTime: clampSegmentEdge(range, "start", range.startTime + deltaSec, duration), endTime: range.endTime };
  }
  if (edge === "end") {
    return { startTime: range.startTime, endTime: clampSegmentEdge(range, "end", range.endTime + deltaSec, duration) };
  }
  const length = range.endTime - range.startTime;
  const startTime = Math.min(Math.max(0, range.startTime + deltaSec), Math.max(0, duration - length));
  return { startTime, endTime: startTime + length };
}

/** Reads and writes one visibility lane's ranges, the same way for pointer drags and keyboard edits. */
export interface VisibilityLaneAccess {
  read: (segment: VideoSegment, duration: number) => CursorVisibilitySegment[];
  /** Writes edited ranges back, merging touching ranges and clamping to the video. */
  write: (segment: VideoSegment, ranges: CursorVisibilitySegment[], duration: number) => VideoSegment;
}

const tidy = (ranges: CursorVisibilitySegment[], duration: number) =>
  clampVisibilitySegmentsToDuration(mergePointerSegments(ranges), duration);

export const VISIBILITY_LANES = {
  keystroke: {
    read: (segment, duration) =>
      clampVisibilitySegmentsToDuration(getKeystrokeVisibilitySegmentsForMode(segment), duration),
    write: (segment, ranges, duration) =>
      withKeystrokeVisibilitySegmentsForMode(segment, tidy(ranges, duration)),
  },
  pointer: {
    read: (segment, duration) => clampVisibilitySegmentsToDuration(segment.cursorVisibilitySegments, duration),
    write: (segment, ranges, duration) => ({ ...segment, cursorVisibilitySegments: tidy(ranges, duration) }),
  },
  webcam: {
    read: (segment, duration) => clampVisibilitySegmentsToDuration(segment.webcamVisibilitySegments, duration),
    write: (segment, ranges, duration) => ({ ...segment, webcamVisibilitySegments: tidy(ranges, duration) }),
  },
} satisfies Record<string, VisibilityLaneAccess>;

/** Writes trim segments and the trimStart/trimEnd bounds derived from them. */
export function withTrimSegments(segment: VideoSegment, trimSegments: TrimSegment[], duration: number): VideoSegment {
  const bounds = getTrimBounds({ ...segment, trimSegments }, duration);
  return { ...segment, trimSegments, trimStart: bounds.trimStart, trimEnd: bounds.trimEnd };
}

export type RangeLane = "text" | "subtitle" | "trim" | keyof typeof VISIBILITY_LANES;

/** The items with `id` nudged, or null when no item has that id (so nothing is written). */
function nudgeById<T extends Range & { id: string }>(
  items: T[], id: string, edge: SegmentEdge, deltaSec: number, duration: number,
): T[] | null {
  if (!items.some((item) => item.id === id)) return null;
  return items.map((item) => (item.id === id ? { ...item, ...nudgeRange(item, edge, deltaSec, duration) } : item));
}

/** Applies one keyboard nudge to a range segment in any range lane. */
export function nudgeLaneRange(
  segment: VideoSegment,
  lane: RangeLane,
  id: string,
  edge: SegmentEdge,
  deltaSec: number,
  duration: number,
): VideoSegment {
  switch (lane) {
    case "text": {
      const textSegments = nudgeById(segment.textSegments ?? [], id, edge, deltaSec, duration);
      return textSegments ? { ...segment, textSegments } : segment;
    }
    case "subtitle":
      return updateSubtitleTimingAcrossTracks(segment, id, (subtitle) => ({
        ...subtitle,
        ...nudgeRange(subtitle, edge, deltaSec, duration),
      }));
    case "trim": {
      // Trim ranges have no body drag: only their edges move.
      const trimmed = edge === "body" ? null : nudgeById(getTrimSegments(segment, duration), id, edge, deltaSec, duration);
      return trimmed ? withTrimSegments(segment, mergeTrimSegments(trimmed), duration) : segment;
    }
    default: {
      const access = VISIBILITY_LANES[lane];
      const ranges = nudgeById(access.read(segment, duration), id, edge, deltaSec, duration);
      return ranges ? access.write(segment, ranges, duration) : segment;
    }
  }
}
