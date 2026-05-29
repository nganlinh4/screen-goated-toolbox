import { useState, useRef, useEffect, useCallback } from "react";
import { VideoSegment, ZoomBlock } from "@/types/video";
import { useThrottle } from "./useAppHooks";

// ============================================================================
// useZoomKeyframes — manages discrete zoom *blocks* (Screen Studio-style).
//
// Each block is a bounded region [startTime, endTime] with a hold target and
// eased in/out ramps. Gaps between blocks revert to the auto path, which is
// what lets auto-zoom live between two manual zooms. The exported names are
// kept stable (`editingKeyframeId`, `handleAddKeyframe`, ...) so the editor's
// many entry points (preview drag, wheel zoom, panel, shortcuts) keep working —
// they now all operate on "the block active at the playhead".
// ============================================================================

// Default geometry for a freshly-created block.
const DEFAULT_BLOCK_HALF_SEC = 1.0; // half-width around the playhead
const DEFAULT_BLOCK_RAMP_SEC = 0.6; // ease-in / ease-out
const MIN_BLOCK_GAP_SEC = 0.1; // keep a sliver between adjacent blocks

interface UseZoomKeyframesProps {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null, addToHistory?: boolean) => void;
  videoRef: React.RefObject<HTMLVideoElement | null>;
  currentTime: number;
  isVideoReady: boolean;
  renderFrame: () => void;
  activePanel: string;
  setActivePanel: (panel: "zoom" | "background" | "cursor" | "text" | "subtitles") => void;
}

type ZoomTarget = { zoomFactor?: number; positionX?: number; positionY?: number };

const getBlocks = (segment: VideoSegment): ZoomBlock[] => segment.zoomBlocks ?? [];

/** Index of the block whose span contains `time` (first match), or -1. */
function activeBlockIndexAt(blocks: ZoomBlock[], time: number): number {
  return blocks.findIndex(
    (b) => b.enabled !== false && time >= b.startTime && time <= b.endTime,
  );
}

export function useZoomKeyframes(props: UseZoomKeyframesProps) {
  // Index of the selected/active block within segment.zoomBlocks.
  const [editingKeyframeId, setEditingKeyframeId] = useState<number | null>(
    null,
  );
  const [zoomFactor, setZoomFactor] = useState(1.5);
  // Stable ref so handlers always read the latest timeline position without
  // needing currentTime in their dependency arrays (it changes at 60fps).
  const currentTimeRef = useRef(props.currentTime);
  currentTimeRef.current = props.currentTime;

  // Create a new block at the playhead, or update the block already covering it.
  const handleAddKeyframe = useCallback(
    (override?: ZoomTarget) => {
      if (!props.segment || !props.videoRef.current) return;

      const t = currentTimeRef.current;
      const blocks = getBlocks(props.segment);
      const activeIdx = activeBlockIndexAt(blocks, t);
      let updatedBlocks: ZoomBlock[];
      let selectIdx: number;

      if (activeIdx !== -1) {
        // Update the block under the playhead.
        const existing = blocks[activeIdx];
        const updated: ZoomBlock = {
          ...existing,
          zoomFactor: override?.zoomFactor ?? existing.zoomFactor,
          positionX: override?.positionX ?? existing.positionX,
          positionY: override?.positionY ?? existing.positionY,
        };
        updatedBlocks = blocks.map((b, i) => (i === activeIdx ? updated : b));
        selectIdx = activeIdx;
      } else {
        // Create a new bounded block centered on the playhead, clipped so it
        // never overlaps its neighbours.
        const duration = props.videoRef.current.duration || t + DEFAULT_BLOCK_HALF_SEC;
        const prev = blocks
          .filter((b) => b.endTime <= t)
          .sort((a, b) => b.endTime - a.endTime)[0];
        const next = blocks
          .filter((b) => b.startTime >= t)
          .sort((a, b) => a.startTime - b.startTime)[0];

        const lowerBound = prev ? prev.endTime + MIN_BLOCK_GAP_SEC : 0;
        const upperBound = next ? next.startTime - MIN_BLOCK_GAP_SEC : duration;
        const startTime = Math.max(lowerBound, t - DEFAULT_BLOCK_HALF_SEC);
        const endTime = Math.min(upperBound, t + DEFAULT_BLOCK_HALF_SEC);
        if (endTime - startTime < MIN_BLOCK_GAP_SEC) return; // no room

        const span = endTime - startTime;
        const ramp = Math.min(DEFAULT_BLOCK_RAMP_SEC, span / 2);
        const prevTarget = prev ?? next;
        const newBlock: ZoomBlock = {
          id: crypto.randomUUID(),
          startTime,
          endTime,
          easeIn: ramp,
          easeOut: ramp,
          zoomFactor: override?.zoomFactor ?? prevTarget?.zoomFactor ?? 1.5,
          positionX: override?.positionX ?? prevTarget?.positionX ?? 0.5,
          positionY: override?.positionY ?? prevTarget?.positionY ?? 0.5,
          followCursor: false,
          enabled: true,
        };
        updatedBlocks = [...blocks, newBlock].sort(
          (a, b) => a.startTime - b.startTime,
        );
        selectIdx = updatedBlocks.indexOf(newBlock);
      }

      setEditingKeyframeId(selectIdx);
      props.setSegment({ ...props.segment, zoomBlocks: updatedBlocks });
      const finalFactor =
        override?.zoomFactor ?? updatedBlocks[selectIdx]?.zoomFactor;
      if (finalFactor !== undefined) setZoomFactor(finalFactor);
    },
    [props],
  );

  const handleDeleteKeyframe = useCallback(() => {
    if (props.segment && editingKeyframeId !== null) {
      props.setSegment({
        ...props.segment,
        zoomBlocks: getBlocks(props.segment).filter(
          (_, i) => i !== editingKeyframeId,
        ),
      });
      setEditingKeyframeId(null);
    }
  }, [props, editingKeyframeId]);

  const throttledUpdateZoom = useThrottle((updates: Partial<ZoomBlock>) => {
    if (!props.segment || editingKeyframeId === null) return;

    const updatedBlocks = getBlocks(props.segment).map((b, i) =>
      i === editingKeyframeId ? { ...b, ...updates } : b,
    );

    props.setSegment({ ...props.segment, zoomBlocks: updatedBlocks }, false);

    // Keep the playhead inside the edited block so its effect stays visible.
    const block = updatedBlocks[editingKeyframeId];
    if (block && props.videoRef.current) {
      const t = props.videoRef.current.currentTime;
      if (t < block.startTime || t > block.endTime) {
        props.videoRef.current.currentTime = (block.startTime + block.endTime) / 2;
      }
    }

    requestAnimationFrame(() => props.renderFrame());
  }, 32);

  // Active-block tracking: select whichever block the playhead is inside.
  useEffect(() => {
    if (!props.segment || !props.isVideoReady) return;

    const blocks = getBlocks(props.segment);
    const idx = activeBlockIndexAt(blocks, props.currentTime);
    if (idx !== -1) {
      if (editingKeyframeId !== idx) {
        setEditingKeyframeId(idx);
        setZoomFactor(blocks[idx].zoomFactor);
        if (props.activePanel !== "zoom") props.setActivePanel("zoom");
      }
      return;
    }
    if (editingKeyframeId !== null) setEditingKeyframeId(null);
  }, [props.currentTime, props.segment, props.isVideoReady]);

  // Sync zoomFactor with the selected block.
  useEffect(() => {
    if (props.segment && editingKeyframeId !== null) {
      const block = getBlocks(props.segment)[editingKeyframeId];
      if (block) setZoomFactor(block.zoomFactor);
    }
  }, [editingKeyframeId, props.segment]);

  return {
    editingKeyframeId,
    setEditingKeyframeId,
    zoomFactor,
    setZoomFactor,
    handleAddKeyframe,
    handleDeleteKeyframe,
    throttledUpdateZoom,
  };
}
