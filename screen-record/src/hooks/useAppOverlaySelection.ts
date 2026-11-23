import { useCallback, useRef } from "react";
import { updateSubtitleStylesAcrossTracks } from "@/lib/subtitleTrackMutations";
import { saveStyleDefault } from "@/lib/stylePreferences";
import { useCompositionSubtitleEditor, type CompositionSubtitleEditorOptions } from './useCompositionSubtitleEditor';

interface OverlayDragMove {
  kind: "text" | "subtitle";
  id: string;
  x: number;
  y: number;
}

export function useAppOverlaySelection(options: CompositionSubtitleEditorOptions) {
  const subtitleEditor = useCompositionSubtitleEditor(options);
  const { segmentRef, setSegment } = subtitleEditor;
  const selectedTextIdsRef = useRef<string[]>([]);
  const selectedSubtitleIdsRef = useRef<string[]>([]);

  const handleSelectedTextIdsChange = useCallback((ids: string[]) => {
    selectedTextIdsRef.current = ids;
  }, []);

  const handleSelectedSubtitleIdsChange = useCallback((ids: string[]) => {
    selectedSubtitleIdsRef.current = ids;
  }, []);

  const handleOverlayDragMove = useCallback((moves: OverlayDragMove[]) => {
    const liveSegment = segmentRef.current;
    if (!liveSegment || moves.length === 0) return;

    const textMoves = new Map<string, { x: number; y: number }>();
    const subtitleMoves = new Map<string, { x: number; y: number }>();
    for (const move of moves) {
      if (move.kind === "subtitle") {
        subtitleMoves.set(move.id, { x: move.x, y: move.y });
      } else {
        textMoves.set(move.id, { x: move.x, y: move.y });
      }
    }

    let nextSegment = liveSegment;
    if (textMoves.size > 0) {
      nextSegment = {
        ...nextSegment,
        textSegments: (nextSegment.textSegments ?? []).map((text) => {
          const move = textMoves.get(text.id);
          return move
            ? {
                ...text,
                style: {
                  ...text.style,
                  x: move.x,
                  y: move.y,
                },
              }
            : text;
        }),
      };
    }

    if (subtitleMoves.size > 0) {
      let savedStyle = false;
      nextSegment = updateSubtitleStylesAcrossTracks(
        nextSegment,
        new Set(subtitleMoves.keys()),
        (subtitle) => {
          const move = subtitleMoves.get(subtitle.id);
          if (move && !savedStyle) {
            saveStyleDefault("subtitle", { ...subtitle.style, ...move });
            savedStyle = true;
          }
          return move
            ? {
                ...subtitle,
                style: {
                  ...subtitle.style,
                  x: move.x,
                  y: move.y,
                },
              }
            : subtitle;
        },
      );
    }

    const movedText = nextSegment.textSegments.find((text) => textMoves.has(text.id));
    if (movedText) saveStyleDefault("text", movedText.style);
    setSegment(nextSegment);
  }, [segmentRef, setSegment]);

  return {
    subtitleEditor,
    selectedTextIdsRef,
    selectedSubtitleIdsRef,
    handleSelectedTextIdsChange,
    handleSelectedSubtitleIdsChange,
    handleOverlayDragMove,
  };
}
