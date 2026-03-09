import { useEffect, useRef } from "react";
import { VideoSegment } from "@/types/video";

export interface UseAppShortcutsParams {
  togglePlayPause: () => void;
  currentTime: number;
  duration: number;
  seek: (time: number) => void | Promise<void>;
  flushSeek?: () => void;
  isCropping: boolean;
  /** When true (a modal dialog is open), suppress play/seek shortcuts so the dialog video controls work normally */
  isModalOpen?: boolean;
  editingKeyframeId: number | null;
  editingTextId: string | null;
  editingKeystrokeSegmentId: string | null;
  editingPointerId: string | null;
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment) => void;
  setEditingKeyframeId: (id: number | null) => void;
  handleDeleteText: () => void;
  handleDeleteKeystrokeSegment: () => void;
  handleDeletePointerSegment: () => void;
  canUndo: boolean;
  canRedo: boolean;
  undo: () => void;
  redo: () => void;
  setSeekIndicatorKey: (key: number) => void;
  setSeekIndicatorDir: (dir: "left" | "right") => void;
}

export function useAppShortcuts({
  togglePlayPause,
  currentTime,
  duration,
  seek,
  flushSeek,
  isCropping,
  isModalOpen = false,
  editingKeyframeId,
  editingTextId,
  editingKeystrokeSegmentId,
  editingPointerId,
  segment,
  setSegment,
  setEditingKeyframeId,
  handleDeleteText,
  handleDeleteKeystrokeSegment,
  handleDeletePointerSegment,
  canUndo,
  canRedo,
  undo,
  redo,
  setSeekIndicatorKey,
  setSeekIndicatorDir,
}: UseAppShortcutsParams) {
  const latestRef = useRef<UseAppShortcutsParams>({
    togglePlayPause,
    currentTime,
    duration,
    seek,
    flushSeek: undefined,
    isCropping,
    isModalOpen,
    editingKeyframeId,
    editingTextId,
    editingKeystrokeSegmentId,
    editingPointerId,
    segment,
    setSegment,
    setEditingKeyframeId,
    handleDeleteText,
    handleDeleteKeystrokeSegment,
    handleDeletePointerSegment,
    canUndo,
    canRedo,
    undo,
    redo,
    setSeekIndicatorKey,
    setSeekIndicatorDir,
  });
  latestRef.current = {
    togglePlayPause,
    currentTime,
    duration,
    seek,
    flushSeek,
    isCropping,
    isModalOpen,
    editingKeyframeId,
    editingTextId,
    editingKeystrokeSegmentId,
    editingPointerId,
    segment,
    setSegment,
    setEditingKeyframeId,
    handleDeleteText,
    handleDeleteKeystrokeSegment,
    handleDeletePointerSegment,
    canUndo,
    canRedo,
    undo,
    redo,
    setSeekIndicatorKey,
    setSeekIndicatorDir,
  };

  useEffect(() => {
    const isTextInputTarget = (target: EventTarget | null) => {
      if (!(target instanceof HTMLElement)) return false;
      const tag = target.tagName;
      const targetType = (target as HTMLInputElement).type;
      return (
        (tag === "INPUT" &&
          ["text", "number", "password", "search", "email"].includes(
            targetType,
          )) ||
        tag === "TEXTAREA" ||
        target.isContentEditable
      );
    };

    const handleKeyDown = (e: KeyboardEvent) => {
      const {
        togglePlayPause,
        currentTime,
        duration,
        seek,
        isCropping,
        isModalOpen = false,
        editingKeyframeId,
        editingTextId,
        editingKeystrokeSegmentId,
        editingPointerId,
        segment,
        setSegment,
        setEditingKeyframeId,
        handleDeleteText,
        handleDeleteKeystrokeSegment,
        handleDeletePointerSegment,
        canUndo,
        canRedo,
        undo,
        redo,
        setSeekIndicatorKey,
        setSeekIndicatorDir,
      } = latestRef.current;
      const isTextInput = isTextInputTarget(e.target);

      if (e.code === "Space" && !isTextInput) {
        if (isModalOpen) return; // Let the dialog's native video controls handle Space
        e.preventDefault();
        e.stopPropagation();
        if (isCropping) return; // Block play/pause during crop mode
        togglePlayPause();
      }
      if (e.code === "ArrowLeft" && !isTextInput) {
        if (isModalOpen) return; // Let the dialog's native video controls handle arrow keys
        e.preventDefault();
        const next = Math.max(0, currentTime - 5);
        seek(next);
        setSeekIndicatorDir("left");
        setSeekIndicatorKey(Date.now());
      }
      if (e.code === "ArrowRight" && !isTextInput) {
        if (isModalOpen) return; // Let the dialog's native video controls handle arrow keys
        e.preventDefault();
        const next = Math.min(duration, currentTime + 5);
        seek(next);
        setSeekIndicatorDir("right");
        setSeekIndicatorKey(Date.now());
      }
      if ((e.code === "Delete" || e.code === "Backspace") && !isTextInput) {
        if (editingKeystrokeSegmentId) {
          handleDeleteKeystrokeSegment();
        } else if (editingPointerId) {
          handleDeletePointerSegment();
        } else if (editingTextId && !editingKeyframeId) {
          handleDeleteText();
        } else if (
          editingKeyframeId !== null &&
          segment?.zoomKeyframes[editingKeyframeId]
        ) {
          setSegment({
            ...segment,
            zoomKeyframes: segment.zoomKeyframes.filter(
              (_, i) => i !== editingKeyframeId,
            ),
          });
          setEditingKeyframeId(null);
        }
      }
      if ((e.ctrlKey || e.metaKey) && e.code === "KeyZ") {
        e.preventDefault();
        e.shiftKey ? canRedo && redo() : canUndo && undo();
      }
      if ((e.ctrlKey || e.metaKey) && e.code === "KeyY") {
        e.preventDefault();
        canRedo && redo();
      }
    };
    const handleKeyUp = (e: KeyboardEvent) => {
      const { isModalOpen = false } = latestRef.current;
      if (e.code !== "Space") return;
      if (isModalOpen) return;
      if (isTextInputTarget(e.target)) return;
      e.preventDefault();
      e.stopPropagation();
    };
    window.addEventListener("keydown", handleKeyDown, true);
    window.addEventListener("keyup", handleKeyUp, true);
    return () => {
      window.removeEventListener("keydown", handleKeyDown, true);
      window.removeEventListener("keyup", handleKeyUp, true);
    };
  }, []);
}
