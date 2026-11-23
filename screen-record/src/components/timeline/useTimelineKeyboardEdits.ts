import { useMemo, useRef } from "react";
import { timelineRangeKeyProps } from "./timelineKeyboard";
import type { VideoSegment } from "@/types/video";
import { nudgeLaneRange, type RangeLane, type SegmentEdge } from "./timelineLaneEdits";
import {
  editingIndexAfterRemoval,
  removeZoomBlockAt,
  resizeZoomBlockEdge,
  resizeZoomTransition,
  zoomBlockStartRange,
} from "./zoomBlockEdits";
import { ZOOM_KEYFRAME_UNTOUCHABLE_GAP_SEC } from "./useTimelineDragTypes";

export type { SegmentEdge } from "./timelineLaneEdits";

/** Keyboard edits the timeline lanes apply to their items; each call is one undo step. */
export interface TimelineKeyboardEdits {
  getPlayheadTime: () => number;
  /** Moves a range segment (or one of its edges) in a text, subtitle, trim or visibility lane. */
  nudge: (lane: RangeLane, id: string, edge: SegmentEdge, deltaSec: number) => void;
  nudgeZoomBlock: (index: number, edge: SegmentEdge, deltaSec: number) => void;
  /** Moves a zoom block's ease-in or ease-out boundary. */
  nudgeZoomEase: (index: number, side: "in" | "out", deltaSec: number) => void;
  deleteZoomBlock: (index: number) => void;
}

interface Options {
  currentTime: number;
  duration: number;
  editingKeyframeId: number | null;
  setSegment: (update: (prev: VideoSegment | null) => VideoSegment | null) => void;
  setEditingKeyframeId: (index: number | null) => void;
}

function nudgeZoomBlock(segment: VideoSegment, index: number, edge: SegmentEdge, deltaSec: number, duration: number) {
  const blocks = segment.zoomBlocks ?? [];
  const block = blocks[index];
  if (!block) return segment;
  if (edge !== "body") {
    const time = (edge === "start" ? block.startTime : block.endTime) + deltaSec;
    const resized = resizeZoomBlockEdge(blocks, index, edge, time, duration);
    return resized ? { ...segment, zoomBlocks: resized } : segment;
  }
  const room = zoomBlockStartRange(blocks, index, duration, ZOOM_KEYFRAME_UNTOUCHABLE_GAP_SEC);
  if (!room) return segment;
  const startTime = Math.max(room.lower, Math.min(room.upper, block.startTime + deltaSec));
  if (startTime === block.startTime) return segment;
  const moved = { ...block, startTime, endTime: startTime + (block.endTime - block.startTime) };
  return { ...segment, zoomBlocks: blocks.map((b, i) => (i === index ? moved : b)) };
}

export function useTimelineKeyboardEdits(options: Options): TimelineKeyboardEdits {
  // Read through a ref so the returned object stays stable for the memoized track stack.
  const latest = useRef(options);
  latest.current = options;

  return useMemo<TimelineKeyboardEdits>(() => ({
    getPlayheadTime: () => latest.current.currentTime,
    nudge: (lane, id, edge, deltaSec) => {
      const { duration, setSegment } = latest.current;
      setSegment((prev) => prev && nudgeLaneRange(prev, lane, id, edge, deltaSec, duration));
    },
    nudgeZoomBlock: (index, edge, deltaSec) => {
      const { duration, setSegment } = latest.current;
      setSegment((prev) => prev && nudgeZoomBlock(prev, index, edge, deltaSec, duration));
    },
    nudgeZoomEase: (index, side, deltaSec) => {
      latest.current.setSegment((prev) => {
        const block = prev?.zoomBlocks?.[index];
        if (!prev || !block) return prev;
        const boundary = side === "in" ? block.startTime + block.easeIn : block.endTime - block.easeOut;
        const zoomBlocks = resizeZoomTransition(prev.zoomBlocks ?? [], index, side, boundary + deltaSec);
        return zoomBlocks ? { ...prev, zoomBlocks } : prev;
      });
    },
    deleteZoomBlock: (index) => {
      const { editingKeyframeId, setSegment, setEditingKeyframeId } = latest.current;
      setSegment((prev) => prev && removeZoomBlockAt(prev, index, editingKeyframeId).segment);
      setEditingKeyframeId(editingIndexAfterRemoval(index, editingKeyframeId));
    },
  }), []);
}

/** The keyboard nudge for one segment of a range lane, or undefined when the lane has no keyboard edits. */
export function laneNudge(edits: TimelineKeyboardEdits | undefined, lane: RangeLane, id: string) {
  return edits && ((edge: SegmentEdge, deltaSec: number) => edits.nudge(lane, id, edge, deltaSec));
}

interface RangeLaneKeyOptions {
  lane: RangeLane;
  /** Lane name; each segment is announced as "<name> <n>". */
  name: string;
  startLabel: string;
  endLabel: string;
  duration: number;
  keyboardEdits?: TimelineKeyboardEdits;
  selectedIds?: ReadonlySet<string>;
  onSelect?: (id: string) => void;
  onDelete?: (ids: string[]) => void;
  onSplit?: (id: string, time: number) => void;
  onAdd?: (time: number) => void;
  addFromEdges?: boolean;
}

/** Keyboard props for every segment of a range lane: one builder shared by the visibility and trim lanes. */
export function rangeLaneKeys(options: RangeLaneKeyOptions) {
  const { lane, keyboardEdits, selectedIds, onSelect, onDelete, onSplit } = options;
  return (range: { id: string; startTime: number; endTime: number }, index: number) =>
    timelineRangeKeyProps({
      name: `${options.name} ${index + 1}`,
      startLabel: options.startLabel,
      endLabel: options.endLabel,
      range,
      duration: options.duration,
      nudge: laneNudge(keyboardEdits, lane, range.id),
      onActivate: onSelect && (() => onSelect(range.id)),
      onDelete: onDelete && (() => onDelete(selectedIds?.has(range.id) ? [...selectedIds] : [range.id])),
      onSplit: onSplit && ((time) => onSplit(range.id, time)),
      onAdd: options.onAdd,
      addFromEdges: options.addFromEdges,
      getPlayheadTime: keyboardEdits?.getPlayheadTime,
    });
}
