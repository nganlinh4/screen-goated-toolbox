import { useEffect } from "react";
import { VideoSegment } from '@/types/video';

export interface UseAppShortcutsParams {
  togglePlayPause: () => void;
  currentTime: number;
  duration: number;
  seek: (time: number) => void;
  flushSeek?: () => void;
  isCropping: boolean;
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
  setSeekIndicatorDir: (dir: 'left' | 'right') => void;
}

export function useAppShortcuts({
  togglePlayPause,
  currentTime,
  duration,
  seek,
  isCropping,
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
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      const targetType = (e.target as HTMLInputElement).type;
      const isTextInput = (tag === 'INPUT' && ['text', 'number', 'password', 'search', 'email'].includes(targetType))
                          || tag === 'TEXTAREA'
                          || (e.target as HTMLElement).isContentEditable;

      if (e.code === 'Space' && !isTextInput) {
        e.preventDefault();
        e.stopImmediatePropagation();
        if (isCropping) return; // Block play/pause during crop mode
        if (e.target instanceof HTMLElement) e.target.blur(); // Unfocus anything so Space keyup doesn't activate it
        togglePlayPause();
      }
      if (e.code === 'ArrowLeft' && !isTextInput) {
        e.preventDefault();
        const next = Math.max(0, currentTime - 5);
        seek(next);
        setSeekIndicatorDir('left');
        setSeekIndicatorKey(Date.now());
      }
      if (e.code === 'ArrowRight' && !isTextInput) {
        e.preventDefault();
        const next = Math.min(duration, currentTime + 5);
        seek(next);
        setSeekIndicatorDir('right');
        setSeekIndicatorKey(Date.now());
      }
      if ((e.code === 'Delete' || e.code === 'Backspace') && !isTextInput) {
        if (editingKeystrokeSegmentId) {
          handleDeleteKeystrokeSegment();
        } else if (editingPointerId) {
          handleDeletePointerSegment();
        } else if (editingTextId && !editingKeyframeId) {
          handleDeleteText();
        } else if (editingKeyframeId !== null && segment?.zoomKeyframes[editingKeyframeId]) {
          setSegment({ ...segment, zoomKeyframes: segment.zoomKeyframes.filter((_, i) => i !== editingKeyframeId) });
          setEditingKeyframeId(null);
        }
      }
      if ((e.ctrlKey || e.metaKey) && e.code === 'KeyZ') {
        e.preventDefault();
        e.shiftKey ? (canRedo && redo()) : (canUndo && undo());
      }
      if ((e.ctrlKey || e.metaKey) && e.code === 'KeyY') { e.preventDefault(); canRedo && redo(); }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [editingKeyframeId, editingTextId, editingPointerId, editingKeystrokeSegmentId, handleDeleteText, handleDeletePointerSegment, handleDeleteKeystrokeSegment, segment, canUndo, canRedo, undo, redo, setSegment, setEditingKeyframeId, togglePlayPause, isCropping, currentTime, duration, seek, setSeekIndicatorKey, setSeekIndicatorDir]);
}
