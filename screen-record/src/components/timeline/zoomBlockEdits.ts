import type { VideoSegment, ZoomBlock } from "@/types/video";

/** Shortest zoom block an edge resize may leave. */
export const MIN_ZOOM_BLOCK_WIDTH_SEC = 0.2;

/**
 * Moves one edge of block `index` to `time`, keeping MIN_ZOOM_BLOCK_WIDTH_SEC
 * from the block's other edge and from its neighbours, and shrinking its ease
 * on that side to fit. Returns null when nothing changes.
 */
export function resizeZoomBlockEdge(
  blocks: readonly ZoomBlock[],
  index: number,
  edge: "start" | "end",
  time: number,
  duration: number,
): ZoomBlock[] | null {
  const block = blocks[index];
  if (!block || duration <= 0) return null;
  const t = Math.max(0, Math.min(duration, time));
  const prev = index > 0 ? blocks[index - 1] : null;
  const next = index < blocks.length - 1 ? blocks[index + 1] : null;

  let updated: ZoomBlock;
  if (edge === "start") {
    const lower = prev ? prev.endTime + MIN_ZOOM_BLOCK_WIDTH_SEC : 0;
    const startTime = Math.max(lower, Math.min(block.endTime - MIN_ZOOM_BLOCK_WIDTH_SEC, t));
    updated = { ...block, easeIn: Math.min(block.easeIn, block.endTime - startTime), startTime };
  } else {
    const upper = next ? next.startTime - MIN_ZOOM_BLOCK_WIDTH_SEC : duration;
    const endTime = Math.min(upper, Math.max(block.startTime + MIN_ZOOM_BLOCK_WIDTH_SEC, t));
    updated = { ...block, easeOut: Math.min(block.easeOut, endTime - block.startTime), endTime };
  }
  if (
    updated.startTime === block.startTime &&
    updated.endTime === block.endTime &&
    updated.easeIn === block.easeIn &&
    updated.easeOut === block.easeOut
  ) {
    return null;
  }
  return blocks.map((b, i) => (i === index ? updated : b));
}

/**
 * Moves block `index`'s ease-in ('in') or ease-out ('out') boundary to `time`,
 * keeping both eases inside the block. Returns null when nothing changes.
 */
export function resizeZoomTransition(
  blocks: readonly ZoomBlock[],
  index: number,
  side: "in" | "out",
  time: number,
): ZoomBlock[] | null {
  const block = blocks[index];
  if (!block) return null;
  const span = block.endTime - block.startTime;
  const updated =
    side === "in"
      ? { ...block, easeIn: Math.max(0, Math.min(span - block.easeOut, time - block.startTime)) }
      : { ...block, easeOut: Math.max(0, Math.min(span - block.easeIn, block.endTime - time)) };
  if (updated.easeIn === block.easeIn && updated.easeOut === block.easeOut) return null;
  return blocks.map((b, i) => (i === index ? updated : b));
}

/** The start times block `index` may take without touching its neighbours, or null when it cannot move. */
export function zoomBlockStartRange(
  blocks: readonly ZoomBlock[],
  index: number,
  duration: number,
  gapSec: number,
): { lower: number; upper: number } | null {
  const block = blocks[index];
  if (!block) return null;
  const width = block.endTime - block.startTime;
  const prev = index > 0 ? blocks[index - 1] : null;
  const next = index < blocks.length - 1 ? blocks[index + 1] : null;
  const lower = prev ? prev.endTime + gapSec : 0;
  const upper = (next ? next.startTime - gapSec : duration) - width;
  return upper < lower ? null : { lower, upper };
}

/** The editing index that still points at the same block after block `removed` is deleted. */
export function editingIndexAfterRemoval(removed: number, editingIndex: number | null): number | null {
  if (editingIndex === null || editingIndex === removed) return null;
  return editingIndex > removed ? editingIndex - 1 : editingIndex;
}

/** Removes zoom block `index` and returns the editing index that still points at the same block. */
export function removeZoomBlockAt(
  segment: VideoSegment,
  index: number,
  editingIndex: number | null,
): { segment: VideoSegment; editingIndex: number | null } {
  const blocks = segment.zoomBlocks ?? [];
  if (!blocks[index]) return { segment, editingIndex };
  return {
    segment: { ...segment, zoomBlocks: blocks.filter((_, i) => i !== index) },
    editingIndex: editingIndexAfterRemoval(index, editingIndex),
  };
}
