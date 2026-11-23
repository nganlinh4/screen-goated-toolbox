import React from 'react';
import { ZoomBlock } from '@/types/video';
import { startTimelinePointerDrag } from './timelinePointerDrag';
import { resizeZoomBlockEdge, resizeZoomTransition } from './zoomBlockEdits';

const ZOOM_BLOCK_DRAG_COMMIT_INTERVAL_MS = 32;

interface UseZoomBlockRangeEditingOptions {
  duration: number;
  blocks: ZoomBlock[];
  onUpdateBlocks: (blocks: ZoomBlock[]) => void;
  beginBatch: () => void;
  commitBatch: () => void;
}

export const useZoomBlockRangeEditing = ({
  duration,
  blocks,
  onUpdateBlocks,
  beginBatch,
  commitBatch,
}: UseZoomBlockRangeEditingOptions) => {
  const blocksRef = React.useRef(blocks);
  blocksRef.current = blocks;
  const callbacksRef = React.useRef({ onUpdateBlocks });
  callbacksRef.current = { onUpdateBlocks };
  const pendingBlockRangeUpdateRef = React.useRef<ZoomBlock[] | null>(null);
  const blockRangeUpdateFrameRef = React.useRef<number | null>(null);
  const lastBlockRangeUpdateAtRef = React.useRef(0);

  const flushPendingBlockRangeUpdate = () => {
    const pending = pendingBlockRangeUpdateRef.current;
    pendingBlockRangeUpdateRef.current = null;
    if (blockRangeUpdateFrameRef.current !== null) {
      cancelAnimationFrame(blockRangeUpdateFrameRef.current);
      blockRangeUpdateFrameRef.current = null;
    }
    if (!pending) return;
    lastBlockRangeUpdateAtRef.current = performance.now();
    callbacksRef.current.onUpdateBlocks(pending);
  };

  const scheduleBlockRangeUpdate = (nextBlocks: ZoomBlock[]) => {
    pendingBlockRangeUpdateRef.current = nextBlocks;
    if (blockRangeUpdateFrameRef.current !== null) return;

    const pump = () => {
      const now = performance.now();
      if (now - lastBlockRangeUpdateAtRef.current < ZOOM_BLOCK_DRAG_COMMIT_INTERVAL_MS) {
        blockRangeUpdateFrameRef.current = requestAnimationFrame(pump);
        return;
      }

      blockRangeUpdateFrameRef.current = null;
      const pending = pendingBlockRangeUpdateRef.current;
      pendingBlockRangeUpdateRef.current = null;
      if (!pending) return;
      lastBlockRangeUpdateAtRef.current = now;
      callbacksRef.current.onUpdateBlocks(pending);
    };

    blockRangeUpdateFrameRef.current = requestAnimationFrame(pump);
  };

  React.useEffect(() => () => {
    if (blockRangeUpdateFrameRef.current !== null) {
      cancelAnimationFrame(blockRangeUpdateFrameRef.current);
      blockRangeUpdateFrameRef.current = null;
    }
    pendingBlockRangeUpdateRef.current = null;
  }, []);

  const startResizeBlock = (
    index: number,
    edge: 'start' | 'end',
    trackElement: HTMLElement,
  ) => {
    beginBatch();
    let draftBlocks = blocksRef.current;
    const onMove = (me: PointerEvent, rect: DOMRect) => {
      if (rect.width <= 0) return;
      const t = ((me.clientX - rect.left) / rect.width) * duration;
      const nextBlocks = resizeZoomBlockEdge(draftBlocks, index, edge, t, duration);
      if (!nextBlocks) return;
      draftBlocks = nextBlocks;
      scheduleBlockRangeUpdate(nextBlocks);
    };
    startTimelinePointerDrag({
      element: trackElement,
      onMove,
      onEnd: () => {
        flushPendingBlockRangeUpdate();
        commitBatch();
      },
    });
  };

  const startResizeTransition = (
    index: number,
    side: 'in' | 'out',
    trackElement: HTMLElement,
  ) => {
    beginBatch();
    let draftBlocks = blocksRef.current;
    const onMove = (me: PointerEvent, rect: DOMRect) => {
      if (rect.width <= 0 || duration <= 0) return;
      const t = Math.max(0, Math.min(duration, ((me.clientX - rect.left) / rect.width) * duration));
      const nextBlocks = resizeZoomTransition(draftBlocks, index, side, t);
      if (!nextBlocks) return;
      draftBlocks = nextBlocks;
      scheduleBlockRangeUpdate(nextBlocks);
    };
    startTimelinePointerDrag({
      element: trackElement,
      onMove,
      onEnd: () => {
        flushPendingBlockRangeUpdate();
        commitBatch();
      },
    });
  };

  return { startResizeBlock, startResizeTransition };
};
