import { useEffect, useRef } from "react";
import { VideoSegment } from "@/types/video";
import { isTimelineKeyOwner } from "@/components/timeline/timelineKeyboard";
import { removeZoomBlockAt } from "@/components/timeline/zoomBlockEdits";

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
  editingSubtitleId: string | null;
  editingKeystrokeSegmentId: string | null;
  editingPointerId: string | null;
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment) => void;
  setEditingKeyframeId: (id: number | null) => void;
  handleDeleteText: () => void;
  handleDeleteSubtitle: () => void;
  handleDeleteKeystrokeSegment: () => void;
  handleDeletePointerSegment: () => void;
  canUndo: boolean;
  canRedo: boolean;
  undo: () => void;
  redo: () => void;
  setSeekIndicatorKey: (key: number) => void;
  setSeekIndicatorDir: (dir: "left" | "right") => void;
}

const TEXT_INPUT_TYPES = new Set(["text", "number", "password", "search", "email", "url", "tel"]);

/** Targets that consume typed characters, Delete/Backspace and their own undo history. */
function isTextInputTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    (target.tagName === "INPUT" && TEXT_INPUT_TYPES.has((target as HTMLInputElement).type)) ||
    target.tagName === "TEXTAREA" ||
    target.isContentEditable
  );
}

/** Targets that own arrow keys: every form control (sliders, selects, inputs), ARIA widgets and focused timeline items. */
function ownsArrowKeys(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (isTextInputTarget(target) || isTimelineKeyOwner(target)) return true;
  if (["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName)) return true;
  const role = target.getAttribute("role");
  return role === "slider" || role === "listbox" || role === "menu" || role === "spinbutton";
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
  editingSubtitleId,
  editingKeystrokeSegmentId,
  editingPointerId,
  segment,
  setSegment,
  setEditingKeyframeId,
  handleDeleteText,
  handleDeleteSubtitle,
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
    editingSubtitleId,
    editingKeystrokeSegmentId,
    editingPointerId,
    segment,
    setSegment,
    setEditingKeyframeId,
    handleDeleteText,
    handleDeleteSubtitle,
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
    editingSubtitleId,
    editingKeystrokeSegmentId,
    editingPointerId,
    segment,
    setSegment,
    setEditingKeyframeId,
    handleDeleteText,
    handleDeleteSubtitle,
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
        editingSubtitleId,
        editingKeystrokeSegmentId,
        editingPointerId,
        segment,
        setSegment,
        setEditingKeyframeId,
        handleDeleteText,
        handleDeleteSubtitle,
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
      const arrowsOwnedByTarget = ownsArrowKeys(e.target);

      if (e.code === "Space" && !isTextInput) {
        if (isModalOpen) return; // Let the dialog's native video controls handle Space
        e.preventDefault();
        e.stopPropagation();
        if (e.repeat) return;
        if (isCropping) return; // Block play/pause during crop mode
        togglePlayPause();
      }
      if (e.code === "ArrowLeft" && !arrowsOwnedByTarget) {
        if (isModalOpen) return; // Let the dialog's native video controls handle arrow keys
        e.preventDefault();
        const next = Math.max(0, currentTime - 5);
        seek(next);
        setSeekIndicatorDir("left");
        setSeekIndicatorKey(Date.now());
      }
      if (e.code === "ArrowRight" && !arrowsOwnedByTarget) {
        if (isModalOpen) return; // Let the dialog's native video controls handle arrow keys
        e.preventDefault();
        const next = Math.min(duration, currentTime + 5);
        seek(next);
        setSeekIndicatorDir("right");
        setSeekIndicatorKey(Date.now());
      }
      // Text fields and open dialogs keep their own editing and undo keys.
      if (isTextInput || isModalOpen) return;
      // A focused timeline item deletes itself, not the selection in another lane.
      if ((e.code === "Delete" || e.code === "Backspace") && !isTimelineKeyOwner(e.target)) {
        // One priority order: keystroke, pointer, subtitle, zoom block, then text.
        if (editingKeystrokeSegmentId) {
          handleDeleteKeystrokeSegment();
        } else if (editingPointerId) {
          handleDeletePointerSegment();
        } else if (editingSubtitleId) {
          handleDeleteSubtitle();
        } else if (
          editingKeyframeId !== null &&
          segment?.zoomBlocks?.[editingKeyframeId]
        ) {
          const removed = removeZoomBlockAt(segment, editingKeyframeId, editingKeyframeId);
          setSegment(removed.segment);
          setEditingKeyframeId(removed.editingIndex);
        } else if (editingTextId) {
          handleDeleteText();
        }
      }
      if ((e.ctrlKey || e.metaKey) && e.code === "KeyZ") {
        e.preventDefault();
        if (e.shiftKey) {
          if (canRedo) redo();
        } else if (canUndo) {
          undo();
        }
      }
      if ((e.ctrlKey || e.metaKey) && e.code === "KeyY") {
        e.preventDefault();
        if (canRedo) redo();
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
