import React from 'react';
import { ZoomBlock } from '@/types/video';

const ZOOM_BLOCK_DRAG_COMMIT_INTERVAL_MS = 32;
const MIN_BLOCK_WIDTH_SEC = 0.2;

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
    rect: DOMRect,
  ) => {
    beginBatch();
    let draftBlocks = blocksRef.current;
    const onMove = (me: MouseEvent) => {
      const current = draftBlocks;
      const block = current[index];
      if (!block || rect.width <= 0 || duration <= 0) return;
      const t = Math.max(
        0,
        Math.min(duration, ((me.clientX - rect.left) / rect.width) * duration),
      );
      const prev = index > 0 ? current[index - 1] : null;
      const next = index < current.length - 1 ? current[index + 1] : null;

      let updated: ZoomBlock;
      if (edge === 'start') {
        const lower = prev ? prev.endTime + MIN_BLOCK_WIDTH_SEC : 0;
        const newStart = Math.max(
          lower,
          Math.min(block.endTime - MIN_BLOCK_WIDTH_SEC, t),
        );
        const span = block.endTime - newStart;
        updated = {
          ...block,
          easeIn: Math.min(block.easeIn, span),
          startTime: newStart,
        };
      } else {
        const upper = next ? next.startTime - MIN_BLOCK_WIDTH_SEC : duration;
        const newEnd = Math.min(
          upper,
          Math.max(block.startTime + MIN_BLOCK_WIDTH_SEC, t),
        );
        const span = newEnd - block.startTime;
        updated = {
          ...block,
          easeOut: Math.min(block.easeOut, span),
          endTime: newEnd,
        };
      }
      if (
        updated.startTime === block.startTime &&
        updated.endTime === block.endTime &&
        updated.easeIn === block.easeIn &&
        updated.easeOut === block.easeOut
      ) {
        return;
      }
      const nextBlocks = current.map((b, i) => (i === index ? updated : b));
      draftBlocks = nextBlocks;
      scheduleBlockRangeUpdate(nextBlocks);
    };
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      flushPendingBlockRangeUpdate();
      commitBatch();
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  };

  const startResizeTransition = (
    index: number,
    side: 'in' | 'out',
    rect: DOMRect,
  ) => {
    beginBatch();
    let draftBlocks = blocksRef.current;
    const onMove = (me: MouseEvent) => {
      const current = draftBlocks;
      const block = current[index];
      if (!block || rect.width <= 0 || duration <= 0) return;
      const t = Math.max(
        0,
        Math.min(duration, ((me.clientX - rect.left) / rect.width) * duration),
      );
      const span = block.endTime - block.startTime;
      const updated =
        side === 'in'
          ? {
              ...block,
              easeIn: Math.max(
                0,
                Math.min(span - block.easeOut, t - block.startTime),
              ),
            }
          : {
              ...block,
              easeOut: Math.max(
                0,
                Math.min(span - block.easeIn, block.endTime - t),
              ),
            };
      if (updated.easeIn === block.easeIn && updated.easeOut === block.easeOut) return;
      const nextBlocks = current.map((b, i) => (i === index ? updated : b));
      draftBlocks = nextBlocks;
      scheduleBlockRangeUpdate(nextBlocks);
    };
    const onUp = () => {
      window.removeEventListener('mousemove', onMove);
      window.removeEventListener('mouseup', onUp);
      flushPendingBlockRangeUpdate();
      commitBatch();
    };
    window.addEventListener('mousemove', onMove);
    window.addEventListener('mouseup', onUp);
  };

  return { startResizeBlock, startResizeTransition };
};
