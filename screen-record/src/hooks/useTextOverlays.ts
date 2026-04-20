import { useState, useCallback } from "react";
import {
  VideoSegment,
  TextSegment,
} from "@/types/video";
import {
  DEFAULT_TEXT_ANIMATION,
  DEFAULT_TEXT_LINE_HEIGHT,
  DEFAULT_TEXT_SHADOW,
  DEFAULT_TEXT_STROKE,
  DEFAULT_TEXT_WRAP,
  defaultTextBackground,
} from "@/lib/textStyleDefaults";

// ============================================================================
// useTextOverlays
// ============================================================================
interface UseTextOverlaysProps {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null) => void;
  currentTime: number;
  duration: number;
  setActivePanel: (panel: "zoom" | "background" | "cursor" | "text" | "subtitles") => void;
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
          lineHeight: DEFAULT_TEXT_LINE_HEIGHT,
          wrap: { ...DEFAULT_TEXT_WRAP },
          stroke: { ...DEFAULT_TEXT_STROKE },
          shadow: { ...DEFAULT_TEXT_SHADOW },
          animation: { ...DEFAULT_TEXT_ANIMATION },
          background: defaultTextBackground({ opacity: 0.6 }),
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
