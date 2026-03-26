import { useState, useRef, useEffect, useCallback } from "react";
import {
  VideoSegment,
  ZoomKeyframe,
} from "@/types/video";
import { getKeyframeRange } from "@/utils/helpers";
import { useThrottle } from "./useAppHooks";

// ============================================================================
// useZoomKeyframes
// ============================================================================
interface UseZoomKeyframesProps {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null, addToHistory?: boolean) => void;
  videoRef: React.RefObject<HTMLVideoElement | null>;
  currentTime: number;
  isVideoReady: boolean;
  renderFrame: () => void;
  activePanel: string;
  setActivePanel: (panel: "zoom" | "background" | "cursor" | "text") => void;
}

export function useZoomKeyframes(props: UseZoomKeyframesProps) {
  const [editingKeyframeId, setEditingKeyframeId] = useState<number | null>(
    null,
  );
  const [zoomFactor, setZoomFactor] = useState(1.5);
  // Stable ref so handleAddKeyframe always reads the latest timeline position
  // without needing currentTime in its dependency array (which changes 60fps).
  const currentTimeRef = useRef(props.currentTime);
  currentTimeRef.current = props.currentTime;

  const handleAddKeyframe = useCallback(
    (override?: Partial<ZoomKeyframe>) => {
      if (!props.segment || !props.videoRef.current) return;

      // Use the React-state currentTime (what the user sees on the timeline),
      // NOT videoRef.current.currentTime which can silently diverge when
      // throttledUpdateZoom seeks the video element to an editing keyframe's time.
      const currentVideoTime = currentTimeRef.current;
      const nearbyIndex = props.segment.zoomKeyframes.findIndex(
        (k) => Math.abs(k.time - currentVideoTime) < 0.2,
      );
      let updatedKeyframes: ZoomKeyframe[];

      if (nearbyIndex !== -1) {
        const existing = props.segment.zoomKeyframes[nearbyIndex];
        updatedKeyframes = [...props.segment.zoomKeyframes];
        updatedKeyframes[nearbyIndex] = {
          ...existing,
          zoomFactor: override?.zoomFactor ?? existing.zoomFactor,
          positionX: override?.positionX ?? existing.positionX,
          positionY: override?.positionY ?? existing.positionY,
        };
        setEditingKeyframeId(nearbyIndex);
      } else {
        const previousKeyframe = [...props.segment.zoomKeyframes]
          .sort((a, b) => b.time - a.time)
          .find((k) => k.time < currentVideoTime);

        const newKeyframe: ZoomKeyframe = {
          time: currentVideoTime,
          duration: 2.0,
          zoomFactor:
            override?.zoomFactor ?? previousKeyframe?.zoomFactor ?? 1.5,
          positionX: override?.positionX ?? previousKeyframe?.positionX ?? 0.5,
          positionY: override?.positionY ?? previousKeyframe?.positionY ?? 0.5,
          easingType: "easeInOut",
        };

        updatedKeyframes = [...props.segment.zoomKeyframes, newKeyframe].sort(
          (a, b) => a.time - b.time,
        );
        setEditingKeyframeId(updatedKeyframes.indexOf(newKeyframe));
      }

      props.setSegment({ ...props.segment, zoomKeyframes: updatedKeyframes });
      const finalFactor =
        override?.zoomFactor ??
        updatedKeyframes[updatedKeyframes.length - 1]?.zoomFactor;
      if (finalFactor !== undefined) setZoomFactor(finalFactor);
    },
    [props.segment, props.videoRef, props.setSegment],
  );

  const handleDeleteKeyframe = useCallback(() => {
    if (props.segment && editingKeyframeId !== null) {
      props.setSegment({
        ...props.segment,
        zoomKeyframes: props.segment.zoomKeyframes.filter(
          (_, i) => i !== editingKeyframeId,
        ),
      });
      setEditingKeyframeId(null);
    }
  }, [props.segment, editingKeyframeId, props.setSegment]);

  const throttledUpdateZoom = useThrottle((updates: Partial<ZoomKeyframe>) => {
    if (!props.segment || editingKeyframeId === null) return;

    const updatedKeyframes = props.segment.zoomKeyframes.map((kf, i) =>
      i === editingKeyframeId ? { ...kf, ...updates } : kf,
    );

    props.setSegment(
      { ...props.segment, zoomKeyframes: updatedKeyframes },
      false,
    );

    if (props.videoRef.current) {
      const kf = updatedKeyframes[editingKeyframeId];
      if (Math.abs(props.videoRef.current.currentTime - kf.time) > 0.1) {
        props.videoRef.current.currentTime = kf.time;
      }
    }

    requestAnimationFrame(() => props.renderFrame());
  }, 32);

  // Active keyframe tracking
  useEffect(() => {
    if (!props.segment || !props.isVideoReady) return;

    const sortedKeyframes = [...props.segment.zoomKeyframes].sort(
      (a, b) => a.time - b.time,
    );
    for (let i = 0; i < sortedKeyframes.length; i++) {
      const { rangeStart, rangeEnd } = getKeyframeRange(sortedKeyframes, i);
      if (props.currentTime >= rangeStart && props.currentTime <= rangeEnd) {
        if (editingKeyframeId !== i) {
          setEditingKeyframeId(i);
          setZoomFactor(sortedKeyframes[i].zoomFactor);
          if (props.activePanel !== "zoom") props.setActivePanel("zoom");
        }
        return;
      }
    }
    if (editingKeyframeId !== null) setEditingKeyframeId(null);
  }, [props.currentTime, props.segment, props.isVideoReady]);

  // Sync zoomFactor with editing keyframe
  useEffect(() => {
    if (props.segment && editingKeyframeId !== null) {
      const kf = props.segment.zoomKeyframes[editingKeyframeId];
      if (kf) setZoomFactor(kf.zoomFactor);
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
