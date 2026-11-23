import type { CSSProperties } from "react";
import {
  buildContentMaskStyle,
  getOverlapRange,
  isSegmentStackedAbove,
  rangesOverlap,
} from "./subtitleTrackLayout";

interface TimeRange {
  startTime: number;
  endTime: number;
}

export interface SegmentStackOverlap {
  /** Another overlapping segment is drawn underneath this one. */
  hasCoveredSegmentUnderneath: boolean;
  /** Masks this segment's content where a segment stacked above covers it. */
  contentMaskStyle: CSSProperties | undefined;
}

/**
 * Overlap and stacking info for every segment of a lane, by position in
 * `segments`. A start-time sweep only compares segments that can overlap, so a
 * lane of mostly sequential segments costs O(N log N) instead of O(N^2).
 */
export function computeSegmentStackOverlaps(
  segments: readonly TimeRange[],
  getRank: (index: number) => number,
): SegmentStackOverlap[] {
  const covered = new Array<boolean>(segments.length).fill(false);
  const maskRanges: Array<Array<{ startPct: number; endPct: number }>> = segments.map(() => []);
  const ranks = segments.map((_, index) => getRank(index));

  const applyPair = (index: number, otherIndex: number) => {
    if (isSegmentStackedAbove(index, ranks[index], otherIndex, ranks[otherIndex])) {
      const range = getOverlapRange(segments[index], segments[otherIndex]);
      if (range) maskRanges[index].push(range);
    } else {
      covered[index] = true;
    }
  };

  const order = segments
    .map((_, index) => index)
    .sort((a, b) => segments[a].startTime - segments[b].startTime);
  let active: number[] = [];
  for (const index of order) {
    const segment = segments[index];
    // Later segments start no earlier, so an expired segment never overlaps again.
    active = active.filter((other) => segments[other].endTime > segment.startTime);
    for (const other of active) {
      if (!rangesOverlap(segment, segments[other])) continue;
      applyPair(index, other);
      applyPair(other, index);
    }
    active.push(index);
  }

  return segments.map((_, index) => ({
    hasCoveredSegmentUnderneath: covered[index],
    contentMaskStyle: buildContentMaskStyle(maskRanges[index]),
  }));
}
