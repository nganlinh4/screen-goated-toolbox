import { useState, useCallback } from "react";
import {
  BackgroundConfig,
  VideoSegment,
  MousePosition,
  CursorVisibilitySegment,
} from "@/types/video";
import {
  clampVisibilitySegmentsToDuration,
  generateCursorVisibility,
  mergePointerSegments,
} from "@/lib/cursorHiding";
import { normalizeMousePositionsToVideoSpace } from "@/lib/dynamicCapture";
import { saveSmartPointerPref } from "./videoStatePreferences";

// ============================================================================
// useCursorHiding
// ============================================================================
interface UseCursorHidingProps {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null) => void;
  mousePositions: MousePosition[];
  currentTime: number;
  duration: number;
  videoRef: React.RefObject<HTMLVideoElement | null>;
  backgroundConfig: BackgroundConfig;
}

export function useCursorHiding(props: UseCursorHidingProps) {
  const [editingPointerId, setEditingPointerId] = useState<string | null>(null);

  const handleSmartPointerHiding = useCallback(() => {
    if (!props.segment) return;

    const segs = props.segment.cursorVisibilitySegments;
    // Check if current state is "default" (single full-duration segment) or empty
    const isDefault =
      !segs ||
      segs.length === 0 ||
      (segs.length === 1 &&
        Math.abs(segs[0].startTime - 0) < 0.01 &&
        Math.abs(segs[0].endTime - props.duration) < 0.01);

    if (!isDefault) {
      // Has customized/generated segments → reset to default (cursor visible everywhere)
      saveSmartPointerPref(false);
      props.setSegment({
        ...props.segment,
        cursorVisibilitySegments: [
          {
            id: crypto.randomUUID(),
            startTime: 0,
            endTime: props.duration,
          },
        ],
      });
      setEditingPointerId(null);
      return;
    }

    // Default or empty → generate from mouse data
    saveSmartPointerPref(true);
    const seg = props.segment;
    const vidW = props.videoRef.current?.videoWidth || 0;
    const vidH = props.videoRef.current?.videoHeight || 0;
    const mp = props.mousePositions;
    const dur = props.duration;
    const bgCfg = props.backgroundConfig;

    // Yield to UI thread before heavy computation
    setTimeout(() => {
      const normalizedMousePositions = normalizeMousePositionsToVideoSpace(mp, vidW, vidH);
      const segments = generateCursorVisibility(
        seg,
        normalizedMousePositions,
        dur,
        vidW,
        vidH,
        bgCfg,
      );
      props.setSegment({
        ...seg,
        cursorVisibilitySegments: clampVisibilitySegmentsToDuration(segments, dur),
      });
    }, 0);
  }, [
    props.segment,
    props.mousePositions,
    props.setSegment,
    props.duration,
    props.videoRef,
    props.backgroundConfig,
  ]);

  const handleAddPointerSegment = useCallback(
    (atTime?: number) => {
      if (!props.segment) return;
      const t0 = atTime ?? props.currentTime;
      const segDur = 2;
      const startTime = Math.max(0, t0 - segDur / 2);

      const newSeg: CursorVisibilitySegment = {
        id: crypto.randomUUID(),
        startTime,
        endTime: Math.min(startTime + segDur, props.duration),
      };

      const allSegs = [
        ...(props.segment.cursorVisibilitySegments || []),
        newSeg,
      ];
      props.setSegment({
        ...props.segment,
        cursorVisibilitySegments: clampVisibilitySegmentsToDuration(
          mergePointerSegments(allSegs),
          props.duration,
        ),
      });
      setEditingPointerId(null);
    },
    [props.segment, props.currentTime, props.duration, props.setSegment],
  );

  const handleDeletePointerSegment = useCallback(() => {
    if (!props.segment || !editingPointerId) return;
    const remaining =
      props.segment.cursorVisibilitySegments?.filter(
        (s) => s.id !== editingPointerId,
      ) ?? [];
    props.setSegment({
      ...props.segment,
      cursorVisibilitySegments: clampVisibilitySegmentsToDuration(
        remaining,
        props.duration,
      ),
    });
    setEditingPointerId(null);
  }, [props.segment, editingPointerId, props.setSegment]);

  return {
    editingPointerId,
    setEditingPointerId,
    handleSmartPointerHiding,
    handleAddPointerSegment,
    handleDeletePointerSegment,
  };
}
