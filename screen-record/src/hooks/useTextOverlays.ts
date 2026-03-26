import { useState, useCallback } from "react";
import {
  VideoSegment,
  TextSegment,
} from "@/types/video";

// ============================================================================
// useTextOverlays
// ============================================================================
interface UseTextOverlaysProps {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null) => void;
  currentTime: number;
  duration: number;
  setActivePanel: (panel: "zoom" | "background" | "cursor" | "text") => void;
}

export function useTextOverlays(props: UseTextOverlaysProps) {
  const [editingTextId, setEditingTextId] = useState<string | null>(null);

  const handleAddText = useCallback(
    (atTime?: number) => {
      if (!props.segment) return;
      const t0 = atTime ?? props.currentTime;
      const segDur = 3;
      const startTime = Math.max(0, t0 - segDur / 2);

      const newText: TextSegment = {
        id: crypto.randomUUID(),
        startTime,
        endTime: Math.min(startTime + segDur, props.duration),
        text: "New Text",
        style: {
          fontSize: 116,
          color: "#ffffff",
          x: 50,
          y: 50,
          fontVariations: { wght: 693, wdth: 96, slnt: 0, ROND: 100 },
          textAlign: "center",
          opacity: 1,
          letterSpacing: 1,
          background: {
            enabled: true,
            color: "#000000",
            opacity: 0.6,
            paddingX: 16,
            paddingY: 8,
            borderRadius: 32,
          },
        },
      };

      props.setSegment({
        ...props.segment,
        textSegments: [...(props.segment.textSegments || []), newText],
      });
      setEditingTextId(newText.id);
      props.setActivePanel("text");
    },
    [
      props.segment,
      props.currentTime,
      props.duration,
      props.setSegment,
      props.setActivePanel,
    ],
  );

  const handleTextDragMove = useCallback(
    (id: string, x: number, y: number) => {
      if (!props.segment) return;
      props.setSegment({
        ...props.segment,
        textSegments: props.segment.textSegments.map((t) =>
          t.id === id ? { ...t, style: { ...t.style, x, y } } : t,
        ),
      });
    },
    [props.segment, props.setSegment],
  );

  const handleDeleteText = useCallback(() => {
    if (!props.segment || !editingTextId) return;
    props.setSegment({
      ...props.segment,
      textSegments: props.segment.textSegments.filter(
        (ts) => ts.id !== editingTextId,
      ),
    });
    setEditingTextId(null);
  }, [props.segment, editingTextId, props.setSegment]);

  return {
    editingTextId,
    setEditingTextId,
    handleAddText,
    handleDeleteText,
    handleTextDragMove,
  };
}
