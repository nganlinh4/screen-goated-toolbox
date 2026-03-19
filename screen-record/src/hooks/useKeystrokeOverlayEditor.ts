import {
  useState,
  useCallback,
  useMemo,
  useEffect,
  type RefObject,
} from "react";
import {
  videoRenderer,
  type KeystrokeOverlayEditBounds,
} from "@/lib/videoRenderer";
import {
  VideoSegment,
  KeystrokeMode,
} from "@/types/video";
import {
  ensureKeystrokeVisibilitySegments,
  getKeystrokeVisibilitySegmentsForMode,
  rebuildKeystrokeVisibilitySegmentsForMode,
  withKeystrokeVisibilitySegmentsForMode,
} from "@/lib/keystrokeVisibility";

export const KEYSTROKE_DELAY_KEY = "screen-record-keystroke-delay-v1";
export const KEYSTROKE_MODE_PREF_KEY = "screen-record-keystroke-mode-pref-v1";
export const KEYSTROKE_OVERLAY_PREF_KEY =
  "screen-record-keystroke-overlay-pref-v1";
export const DEFAULT_KEYSTROKE_DELAY_SEC = 0;

export function getSavedKeystrokeModePref(): KeystrokeMode {
  try {
    const raw = localStorage.getItem(KEYSTROKE_MODE_PREF_KEY);
    if (raw === "keyboard" || raw === "keyboardMouse" || raw === "off")
      return raw;
  } catch {
    // ignore
  }
  return "off";
}

export function getSavedKeystrokeOverlayPref(): {
  x: number;
  y: number;
  scale: number;
} {
  try {
    const raw = localStorage.getItem(KEYSTROKE_OVERLAY_PREF_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<{
        x: number;
        y: number;
        scale: number;
      }>;
      if (typeof parsed === "object" && parsed !== null) {
        return {
          x: typeof parsed.x === "number" ? parsed.x : 50,
          y: typeof parsed.y === "number" ? parsed.y : 100,
          scale: typeof parsed.scale === "number" ? parsed.scale : 1,
        };
      }
    }
  } catch {
    // ignore
  }
  return { x: 50, y: 100, scale: 1 };
}

interface UseKeystrokeOverlayEditorParams {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment) => void;
  currentTime: number;
  duration: number;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  previewContainerRef: RefObject<HTMLDivElement | null>;
}

export function useKeystrokeOverlayEditor({
  segment,
  setSegment,
  currentTime,
  duration,
  canvasRef,
  previewContainerRef,
}: UseKeystrokeOverlayEditorParams) {
  const [editingKeystrokeSegmentId, setEditingKeystrokeSegmentId] = useState<
    string | null
  >(null);
  const [isKeystrokeOverlaySelected, setIsKeystrokeOverlaySelected] =
    useState(false);
  const [isKeystrokeResizeHandleHover, setIsKeystrokeResizeHandleHover] =
    useState(false);
  const [isKeystrokeResizeDragging, setIsKeystrokeResizeDragging] =
    useState(false);

  const getKeystrokeTimelineDuration = useCallback(
    (s: VideoSegment) => {
      const segmentDuration = Math.max(
        s.trimEnd,
        ...(s.trimSegments || []).map((trimSegment) => trimSegment.endTime),
        duration,
      );
      // Timeline tracks are rendered against `duration`; visibility segments must stay inside it.
      if (duration > 0) return duration;
      return segmentDuration;
    },
    [duration],
  );

  const keystrokeOverlayEditBounds =
    useMemo<KeystrokeOverlayEditBounds | null>(() => {
      if (
        !segment ||
        !canvasRef.current ||
        (segment.keystrokeMode ?? "off") === "off"
      )
        return null;
      return videoRenderer.getKeystrokeOverlayEditBounds(
        segment,
        canvasRef.current,
        currentTime,
        getKeystrokeTimelineDuration(segment),
      );
    }, [segment, currentTime, getKeystrokeTimelineDuration, canvasRef]);

  const keystrokeOverlayEditFrame = useMemo(() => {
    if (
      !keystrokeOverlayEditBounds ||
      !canvasRef.current ||
      !previewContainerRef.current
    )
      return null;
    const canvasRect = canvasRef.current.getBoundingClientRect();
    const previewRect = previewContainerRef.current.getBoundingClientRect();
    const scaleX = canvasRect.width / Math.max(1, canvasRef.current.width);
    const scaleY = canvasRect.height / Math.max(1, canvasRef.current.height);
    return {
      left:
        canvasRect.left -
        previewRect.left +
        keystrokeOverlayEditBounds.x * scaleX,
      top:
        canvasRect.top -
        previewRect.top +
        keystrokeOverlayEditBounds.y * scaleY,
      width: keystrokeOverlayEditBounds.width * scaleX,
      height: keystrokeOverlayEditBounds.height * scaleY,
      handleSize: Math.max(
        8,
        keystrokeOverlayEditBounds.handleSize * Math.min(scaleX, scaleY),
      ),
    };
  }, [keystrokeOverlayEditBounds, canvasRef, previewContainerRef]);

  useEffect(() => {
    if (!segment || (segment.keystrokeMode ?? "off") === "off") {
      setIsKeystrokeOverlaySelected(false);
    }
  }, [segment]);

  const handleAddKeystrokeSegment = useCallback(
    (atTime?: number) => {
      if (!segment || (segment.keystrokeMode ?? "off") === "off") return;
      const prepared = ensureKeystrokeVisibilitySegments(
        segment,
        getKeystrokeTimelineDuration(segment),
      );
      const currentSegments = getKeystrokeVisibilitySegmentsForMode(prepared);
      const t0 = atTime ?? currentTime;
      const segmentDuration = getKeystrokeTimelineDuration(prepared);
      const segDur = 2;
      const startTime = Math.max(0, t0 - segDur / 2);

      const newSeg = {
        id: crypto.randomUUID(),
        startTime,
        endTime: Math.min(startTime + segDur, segmentDuration),
      };

      setSegment(
        withKeystrokeVisibilitySegmentsForMode(prepared, [
          ...currentSegments,
          newSeg,
        ]),
      );
      setEditingKeystrokeSegmentId(null);
    },
    [segment, currentTime, getKeystrokeTimelineDuration, setSegment],
  );

  const handleDeleteKeystrokeSegment = useCallback(() => {
    if (
      !segment ||
      !editingKeystrokeSegmentId ||
      (segment.keystrokeMode ?? "off") === "off"
    )
      return;
    const prepared = ensureKeystrokeVisibilitySegments(
      segment,
      getKeystrokeTimelineDuration(segment),
    );
    const currentSegments = getKeystrokeVisibilitySegmentsForMode(prepared);
    const remaining = currentSegments.filter(
      (s) => s.id !== editingKeystrokeSegmentId,
    );
    setSegment(withKeystrokeVisibilitySegmentsForMode(prepared, remaining));
    setEditingKeystrokeSegmentId(null);
  }, [
    segment,
    editingKeystrokeSegmentId,
    getKeystrokeTimelineDuration,
    setSegment,
  ]);

  const handleToggleKeystrokeMode = useCallback(() => {
    if (!segment) return;
    const timelineDuration = getKeystrokeTimelineDuration(segment);
    let prepared = ensureKeystrokeVisibilitySegments(segment, timelineDuration);
    const current = segment.keystrokeMode ?? "off";
    const next: KeystrokeMode =
      current === "off"
        ? "keyboard"
        : current === "keyboard"
          ? "keyboardMouse"
          : "off";

    if (next === "keyboard" || next === "keyboardMouse") {
      // Toggle intent = reset to fresh auto-generated visibility ranges for that mode.
      prepared = rebuildKeystrokeVisibilitySegmentsForMode(
        prepared,
        next,
        timelineDuration,
      );
    }

    setSegment({
      ...prepared,
      keystrokeMode: next,
      keystrokeDelaySec:
        prepared.keystrokeDelaySec ?? DEFAULT_KEYSTROKE_DELAY_SEC,
      keystrokeEvents: prepared.keystrokeEvents ?? [],
    });
    setEditingKeystrokeSegmentId(null);
  }, [segment, setSegment, getKeystrokeTimelineDuration]);

  const handleKeystrokeDelayChange = useCallback(
    (value: number) => {
      if (!segment) return;
      const snapped = Math.abs(value) <= 0.03 ? 0 : value;
      const clamped = Math.max(-1, Math.min(1, snapped));
      const prevDelay = Math.max(
        -1,
        Math.min(1, segment.keystrokeDelaySec ?? DEFAULT_KEYSTROKE_DELAY_SEC),
      );
      const delta = clamped - prevDelay;
      const mode = segment.keystrokeMode ?? "off";

      let nextSegment: VideoSegment = {
        ...segment,
        keystrokeDelaySec: clamped,
      };

      if (
        (mode === "keyboard" || mode === "keyboardMouse") &&
        Math.abs(delta) > 0.0005
      ) {
        const shifted = getKeystrokeVisibilitySegmentsForMode(segment)
          .map((range) => {
            const startTime = range.startTime + delta;
            const endTime = range.endTime + delta;
            if (endTime - startTime <= 0.001) return null;
            return {
              ...range,
              startTime,
              endTime,
            };
          })
          .filter((range): range is NonNullable<typeof range> =>
            Boolean(range),
          );
        nextSegment = withKeystrokeVisibilitySegmentsForMode(
          nextSegment,
          shifted,
          { merge: false },
        );
      }

      setSegment(nextSegment);
      try {
        localStorage.setItem(KEYSTROKE_DELAY_KEY, String(clamped));
      } catch {
        /* ignore */
      }
    },
    [segment, setSegment, getKeystrokeTimelineDuration],
  );

  // Persist keystroke mode preference so new recordings remember the last setting.
  useEffect(() => {
    if (!segment?.keystrokeMode) return;
    try {
      localStorage.setItem(KEYSTROKE_MODE_PREF_KEY, segment.keystrokeMode);
    } catch {
      /* ignore */
    }
  }, [segment?.keystrokeMode]);

  // Persist keystroke overlay position/scale so new recordings inherit the last layout.
  useEffect(() => {
    if (!segment?.keystrokeOverlay) return;
    try {
      localStorage.setItem(
        KEYSTROKE_OVERLAY_PREF_KEY,
        JSON.stringify(segment.keystrokeOverlay),
      );
    } catch {
      /* ignore */
    }
  }, [segment?.keystrokeOverlay]);

  return {
    editingKeystrokeSegmentId,
    setEditingKeystrokeSegmentId,
    isKeystrokeOverlaySelected,
    setIsKeystrokeOverlaySelected,
    isKeystrokeResizeHandleHover,
    setIsKeystrokeResizeHandleHover,
    isKeystrokeResizeDragging,
    setIsKeystrokeResizeDragging,
    getKeystrokeTimelineDuration,
    keystrokeOverlayEditBounds,
    keystrokeOverlayEditFrame,
    handleAddKeystrokeSegment,
    handleDeleteKeystrokeSegment,
    handleToggleKeystrokeMode,
    handleKeystrokeDelayChange,
  };
}
