import { useState, useEffect, useCallback, useRef, useMemo, useLayoutEffect, type CSSProperties } from "react";
import { videoTimeToWallClock } from '@/lib/exportEstimator';
import { Wand2, MousePointer2, Volume2, Keyboard } from "lucide-react";
import { invoke } from '@/lib/ipc';
import "./App.css";
import { Button } from "@/components/ui/button";
import { videoRenderer, type KeystrokeOverlayEditBounds } from '@/lib/videoRenderer';
import { BackgroundConfig, MousePosition, VideoSegment, KeystrokeMode, RecordingMode } from '@/types/video';
import { projectManager } from '@/lib/projectManager';
import { TimelineArea } from '@/components/timeline';
import { useUndoRedo } from '@/hooks/useUndoRedo';
import { useHotkeys, useMonitors, useWindows } from '@/hooks/useAppHooks';
import {
  useVideoPlayback, useRecording, useProjects, useExport,
  useZoomKeyframes, useTextOverlays, useAutoZoom, useCursorHiding,
  getSavedKeystrokeLanguage, saveKeystrokeLanguage, getSavedCropPref, saveCropPref,
} from '@/hooks/useVideoState';

import { Header } from '@/components/Header';
import { Placeholder, CropOverlay, PlaybackControls, CanvasResizeOverlay, SeekIndicator } from '@/components/VideoPreview';
import { SidePanel, type ActivePanel } from '@/components/sidepanel/index';
import {
  ProcessingOverlay, ExportDialog, WindowSelectDialog,
  HotkeyDialog, RawVideoDialog, ExportSuccessDialog
} from '@/components/dialogs';
import { ProjectsView } from '@/components/ProjectsView';
import { SettingsContext, useSettingsProvider } from '@/hooks/useSettings';
import { clampVisibilitySegmentsToDuration } from '@/lib/cursorHiding';
import {
  ensureKeystrokeVisibilitySegments,
  getKeystrokeVisibilitySegmentsForMode,
  rebuildKeystrokeVisibilitySegmentsForMode,
  withKeystrokeVisibilitySegmentsForMode
} from '@/lib/keystrokeVisibility';
import { ResizeBorders } from '@/components/layout/ResizeBorders';
import { useAppShortcuts } from '@/hooks/useAppShortcuts';
import { useRawVideoHandler } from '@/hooks/useRawVideoHandler';
import { useKeystrokeDrag } from '@/hooks/useKeystrokeDrag';

const LAST_BG_CONFIG_KEY = 'screen-record-last-background-config-v1';
const RECENT_UPLOADS_KEY = 'screen-record-recent-uploads-v1';
const RECORDING_MODE_KEY = 'screen-record-recording-mode-v1';
const CAPTURE_SOURCE_KEY = 'screen-record-capture-source-v1';
const KEYSTROKE_DELAY_KEY = 'screen-record-keystroke-delay-v1';
const KEYSTROKE_MODE_PREF_KEY = 'screen-record-keystroke-mode-pref-v1';
const KEYSTROKE_OVERLAY_PREF_KEY = 'screen-record-keystroke-overlay-pref-v1';
const PROJECT_SAVE_DEBUG = true;
const DEFAULT_KEYSTROKE_DELAY_SEC = 0;
const sv = (v: number, min: number, max: number): CSSProperties =>
  ({ '--value-pct': `${((v - min) / (max - min)) * 100}%` } as CSSProperties);

const DEFAULT_BACKGROUND_CONFIG: BackgroundConfig = {
  scale: 90,
  borderRadius: 32,
  backgroundType: 'gradient2',
  shadow: 100,
  volume: 1,
  cursorScale: 5,
  cursorMovementDelay: 0,
  cursorShadow: 100,
  cursorWiggleStrength: 0.30,
  cursorTiltAngle: -10,
  motionBlurCursor: 25,
  motionBlurZoom: 10,
  motionBlurPan: 10,
  cursorPack: 'macos26',
  cursorDefaultVariant: 'macos26',
  cursorTextVariant: 'macos26',
  cursorPointerVariant: 'macos26',
  cursorOpenHandVariant: 'macos26'
};

function getInitialBackgroundConfig(): BackgroundConfig {
  try {
    const raw = localStorage.getItem(LAST_BG_CONFIG_KEY);
    if (!raw) return DEFAULT_BACKGROUND_CONFIG;
    const parsed = JSON.parse(raw) as Partial<BackgroundConfig>;
    return {
      ...DEFAULT_BACKGROUND_CONFIG,
      ...parsed,
    };
  } catch {
    return DEFAULT_BACKGROUND_CONFIG;
  }
}

function getInitialRecentUploads(): string[] {
  try {
    const raw = localStorage.getItem(RECENT_UPLOADS_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((v): v is string => typeof v === 'string' && v.length > 0).slice(0, 12);
  } catch {
    return [];
  }
}

function getInitialRecordingMode(): RecordingMode {
  try {
    const raw = localStorage.getItem(RECORDING_MODE_KEY);
    if (raw === 'withCursor' || raw === 'withoutCursor') return raw;
  } catch {
    // ignore
  }
  return 'withoutCursor';
}

function getInitialCaptureSource(): 'monitor' | 'window' {
  try {
    const raw = localStorage.getItem(CAPTURE_SOURCE_KEY);
    if (raw === 'monitor' || raw === 'window') return raw;
  } catch {
    // ignore
  }
  return 'monitor';
}

function getSavedKeystrokeModePref(): KeystrokeMode {
  try {
    const raw = localStorage.getItem(KEYSTROKE_MODE_PREF_KEY);
    if (raw === 'keyboard' || raw === 'keyboardMouse' || raw === 'off') return raw;
  } catch {
    // ignore
  }
  return 'off';
}

function getSavedKeystrokeOverlayPref(): { x: number; y: number; scale: number } {
  try {
    const raw = localStorage.getItem(KEYSTROKE_OVERLAY_PREF_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<{ x: number; y: number; scale: number }>;
      if (typeof parsed === 'object' && parsed !== null) {
        return {
          x: typeof parsed.x === 'number' ? parsed.x : 50,
          y: typeof parsed.y === 'number' ? parsed.y : 100,
          scale: typeof parsed.scale === 'number' ? parsed.scale : 1,
        };
      }
    }
  } catch {
    // ignore
  }
  return { x: 50, y: 100, scale: 1 };
}


function App() {
  const settings = useSettingsProvider();
  const { t } = settings;
  // Core state
  const { state: segment, setState: setSegment, undo, redo, canUndo, canRedo, beginBatch, commitBatch } = useUndoRedo<VideoSegment | null>(null);
  const [activePanel, setActivePanel] = useState<ActivePanel>('zoom');
  const [isCropping, setIsCropping] = useState(false);
  const [recentUploads, setRecentUploads] = useState<string[]>(getInitialRecentUploads);
  const [backgroundConfig, setBackgroundConfig] = useState<BackgroundConfig>(getInitialBackgroundConfig);
  const [selectedRecordingMode, setSelectedRecordingMode] = useState<RecordingMode>(getInitialRecordingMode);
  const[captureSource, setCaptureSource] = useState<'monitor' | 'window'>(getInitialCaptureSource);
  const[captureTargetId, setCaptureTargetId] = useState<string>('0');
  const [captureFps, setCaptureFps] = useState<number | null>(() => {
    try {
      const saved = localStorage.getItem('screen-record-capture-fps-v1');
      return saved ? parseInt(saved, 10) : null;
    } catch { return null; }
  });
  const captureFpsRef = useRef<number | null>(captureFps);
  const [currentRecordingMode, setCurrentRecordingMode] = useState<RecordingMode>('withoutCursor');
  const rawVideo = useRawVideoHandler();
  const {
    currentRawVideoPath, setCurrentRawVideoPath,
    lastRawSavedPath, setLastRawSavedPath,
    showRawVideoDialog, setShowRawVideoDialog,
    rawAutoCopyEnabled,
    rawSaveDir,
    isRawActionBusy, setIsRawActionBusy,
    rawButtonSavedFlash, setRawButtonSavedFlash,
    flashRawSavedButton,
    handleOpenRawVideoDialog,
    handleToggleRawAutoCopy,
  } = rawVideo;
  const [isBackgroundUploadProcessing, setIsBackgroundUploadProcessing] = useState(false);

  const timelineRef = useRef<HTMLDivElement>(null);
  const previewContainerRef = useRef<HTMLDivElement>(null);
  const mousePositionsRef = useRef<MousePosition[]>([]);
  const wheelBatchActiveRef = useRef(false);
  const wheelBatchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const restoreImageRef = useRef<string | null>(null);
  const projectSaveSeqRef = useRef(0);
  const segmentRef = useRef<VideoSegment | null>(null);
  const isDraggingKeystrokeOverlayRef = useRef(false);
  const isResizingKeystrokeOverlayRef = useRef(false);
  const keystrokeOverlayDragStartRef = useRef<{
    pointerX: number;
    pointerY: number;
    anchorXPx: number;
    baselineYPx: number;
    startScale: number;
    centerX: number;
    centerY: number;
    startRadius: number;
  } | null>(null);
  const [isKeystrokeOverlaySelected, setIsKeystrokeOverlaySelected] = useState(false);
  const [isPreviewDragging, setIsPreviewDragging] = useState(false);
  const [isKeystrokeResizeHandleHover, setIsKeystrokeResizeHandleHover] = useState(false);
  const [isKeystrokeResizeDragging, setIsKeystrokeResizeDragging] = useState(false);
  const [seekIndicatorKey, setSeekIndicatorKey] = useState(0);
  const [seekIndicatorDir, setSeekIndicatorDir] = useState<'left' | 'right'>('right');
  const pendingWindowRecordingRef = useRef(false);
  // Stable ref for persist callback — avoids cascading useEffect re-triggers
  const persistRef = useRef<typeof persistCurrentProjectNow>(null!);
  const debugProject = useCallback((event: string, data?: Record<string, unknown>) => {
    if (!PROJECT_SAVE_DEBUG) return;
    const ts = new Date().toISOString();
    console.log(`[ProjectSave][${ts}] ${event}`, data || {});
  }, []);

  // Utility hooks
  const { hotkeys, showHotkeyDialog, handleRemoveHotkey, openHotkeyDialog, closeHotkeyDialog } = useHotkeys();
  const { monitors, getMonitors } = useMonitors();
  const { windows, showWindowSelect, setShowWindowSelect, getWindows } = useWindows();

  // Video playback — mousePositionsRef is shared so useVideoPlayback always reads latest
  const playback = useVideoPlayback({ segment, backgroundConfig, mousePositionsRef, isCropping });
  const {
    currentTime, setCurrentTime, duration, setDuration, isPlaying, isVideoReady, setIsVideoReady,
    thumbnails, setThumbnails, currentVideo, setCurrentVideo, currentAudio, setCurrentAudio,
    videoRef, audioRef, canvasRef, tempCanvasRef, videoControllerRef,
    renderFrame, togglePlayPause, seek, flushSeek, generateThumbnail, generateThumbnails
  } = playback;

  // Recording
  const recording = useRecording({
    videoControllerRef, videoRef, canvasRef, tempCanvasRef, backgroundConfig,
    setSegment, setCurrentVideo, setCurrentAudio, setIsVideoReady, setThumbnails,
    setDuration, setCurrentTime, generateThumbnails, generateThumbnail,
    renderFrame, currentVideo, currentAudio
  });
  const {
    isRecording, recordingDuration, isLoadingVideo, loadingProgress,
    mousePositions, setMousePositions, audioFilePath, videoFilePath, videoFilePathOwnerUrl, error, setError,
    startNewRecording, handleStopRecording
  } = recording;

  // Sync mouse positions to ref before paint so useVideoPlayback always reads
  // the latest positions without causing stale-closure bugs in Concurrent Mode.
  useLayoutEffect(() => {
    mousePositionsRef.current = mousePositions;
  }, [mousePositions]);

  // Projects
  const handleProjectRawVideoPathChange = useCallback((path: string) => {
    setCurrentRawVideoPath(path);
    setLastRawSavedPath('');
  }, []);
  const projects = useProjects({
    videoControllerRef, setCurrentVideo, setCurrentAudio, setSegment,
    setBackgroundConfig, setMousePositions, setThumbnails,
    setCurrentRecordingMode, setCurrentRawVideoPath: handleProjectRawVideoPathChange,
    currentVideo, currentAudio
  });

  // FPS of the most-recent recording (set on stop, cleared when a different project loads).
  const [lastCaptureFps, setLastCaptureFps] = useState<number | null>(null);

  // Export
  const exportHook = useExport({
    videoRef, canvasRef, tempCanvasRef, audioRef, segment, backgroundConfig,
    isRecording,
    mousePositions,
    audioFilePath,
    videoFilePath,
    videoFilePathOwnerUrl,
    rawVideoPath: currentRawVideoPath,
    savedRawVideoPath: lastRawSavedPath,
    currentVideo,
    lastCaptureFps
  });

  const handleExportSuccessPathChange = useCallback(async (newPath: string) => {
    exportHook.setLastExportedPath(newPath);
  }, [exportHook]);

  // Zoom keyframes
  const zoomKeyframes = useZoomKeyframes({
    segment, setSegment, videoRef, currentTime, isVideoReady, renderFrame, activePanel, setActivePanel
  });
  const { editingKeyframeId, setEditingKeyframeId, zoomFactor, setZoomFactor,
    handleAddKeyframe, handleDeleteKeyframe, throttledUpdateZoom } = zoomKeyframes;

  // Text overlays
  const textOverlays = useTextOverlays({ segment, setSegment, currentTime, duration, setActivePanel });
  const { editingTextId, setEditingTextId, handleAddText, handleDeleteText, handleTextDragMove } = textOverlays;
  const [editingKeystrokeSegmentId, setEditingKeystrokeSegmentId] = useState<string | null>(null);

  // Auto zoom
  const { handleAutoZoom } = useAutoZoom({
    segment, setSegment, videoRef, mousePositions, duration,
    currentProjectId: projects.currentProjectId, backgroundConfig,
    loadProjects: projects.loadProjects, setActivePanel
  });

  // Cursor hiding
  const cursorHiding = useCursorHiding({
    segment, setSegment, mousePositions, currentTime, duration, videoRef, backgroundConfig
  });
  const { editingPointerId, setEditingPointerId, handleSmartPointerHiding,
    handleAddPointerSegment, handleDeletePointerSegment } = cursorHiding;
  const isOverlayMode = projects.showProjectsDialog || isCropping;

  // Wall-clock times (adjusted for speed curve) for display in controls and ruler.
  const wallClockDuration = useMemo(() => {
    const pts = segment?.speedPoints;
    if (!pts?.length || !duration) return duration;
    return videoTimeToWallClock(duration, pts);
  }, [duration, segment?.speedPoints]);

  const wallClockCurrentTime = useMemo(() => {
    const pts = segment?.speedPoints;
    if (!pts?.length) return currentTime;
    return videoTimeToWallClock(currentTime, pts);
  }, [currentTime, segment?.speedPoints]);

  useEffect(() => {
    segmentRef.current = segment;
  }, [segment]);

  // Persist last-used background config so new projects inherit previous project settings.
  useEffect(() => {
    try {
      localStorage.setItem(LAST_BG_CONFIG_KEY, JSON.stringify(backgroundConfig));
    } catch {
      // ignore persistence failures
    }
  }, [backgroundConfig]);

  useEffect(() => {
    try {
      localStorage.setItem(RECENT_UPLOADS_KEY, JSON.stringify(recentUploads));
    } catch {
      // ignore persistence failures
    }
  }, [recentUploads]);

  useEffect(() => {
    try {
      localStorage.setItem(RECORDING_MODE_KEY, selectedRecordingMode);
    } catch {
      // ignore persistence failures
    }
  }, [selectedRecordingMode]);

  useEffect(() => {
    try {
      localStorage.setItem(CAPTURE_SOURCE_KEY, captureSource);
    } catch {
      // ignore persistence failures
    }
  }, [captureSource]);

  useEffect(() => {
    try {
      localStorage.setItem('screen-record-capture-target-v1', captureTargetId);
    } catch {}
  }, [captureTargetId]);

  useEffect(() => {
    try {
      if (captureFps === null) localStorage.removeItem('screen-record-capture-fps-v1');
      else localStorage.setItem('screen-record-capture-fps-v1', captureFps.toString());
    } catch {}
    captureFpsRef.current = captureFps;
  }, [captureFps]);

  useEffect(() => {
    try {
      const saved = localStorage.getItem('screen-record-capture-target-v1');
      if (saved) setCaptureTargetId(saved);
    } catch {}
  }, []);

  useEffect(() => {
    if (!showWindowSelect) return;

    let cancelled = false;
    let inFlight = false;

    const refreshWindows = async () => {
      if (inFlight || cancelled) return;
      inFlight = true;
      try {
        await getWindows();
      } catch {
        // noop
      } finally {
        inFlight = false;
      }
    };

    void refreshWindows();
    const timer = window.setInterval(() => {
      void refreshWindows();
    }, 1200);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [showWindowSelect, getWindows]);

  // Handlers
  const handleToggleCrop = useCallback(() => {
    if (isCropping) {
      setIsCropping(false);
      setActivePanel('zoom');
      setZoomFactor(1.0);
      setEditingKeyframeId(null);
    } else {
      setIsCropping(true);
      if (isPlaying) togglePlayPause();
    }
  }, [isCropping, isPlaying, togglePlayPause, setZoomFactor, setEditingKeyframeId]);

  // Track active preview drag listeners for cleanup on unmount.
  const previewDragCleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    return () => {
      previewDragCleanupRef.current?.();
    };
  }, []);

  const handlePreviewMouseDown = useCallback((e: React.MouseEvent) => {
    if (!currentVideo || isCropping || activePanel === 'text') return;
    e.preventDefault();
    e.stopPropagation();
    if (isPlaying) togglePlayPause();

    const startX = e.clientX;
    const startY = e.clientY;
    const lastState = videoRenderer.getLastCalculatedState();
    if (!lastState) return;

    const { positionX: startPosX, positionY: startPosY, zoomFactor: z } = lastState;
    const rect = e.currentTarget.getBoundingClientRect();
    beginBatch();
    setIsPreviewDragging(true);
    let lockedAxis: 'x' | 'y' | null = null;

    const handleMouseMove = (me: MouseEvent) => {
      let dx = me.clientX - startX;
      let dy = me.clientY - startY;

      if (me.shiftKey) {
        if (!lockedAxis) {
          if (Math.abs(dx) > Math.abs(dy)) lockedAxis = 'x';
          else lockedAxis = 'y';
        }
        if (lockedAxis === 'x') dy = 0;
        if (lockedAxis === 'y') dx = 0;
      } else {
        lockedAxis = null;
      }

      handleAddKeyframe({
        zoomFactor: z,
        positionX: Math.max(0, Math.min(1, startPosX - (dx / rect.width) / z)),
        positionY: Math.max(0, Math.min(1, startPosY - (dy / rect.height) / z))
      });
      setActivePanel('zoom');
    };

    const handleMouseUp = () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
      previewDragCleanupRef.current = null;
      setIsPreviewDragging(false);
      commitBatch();
    };

    // Store cleanup so unmount can remove listeners if mouseup never fires.
    previewDragCleanupRef.current = () => {
      window.removeEventListener('mousemove', handleMouseMove);
      window.removeEventListener('mouseup', handleMouseUp);
    };

    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
  }, [currentVideo, isCropping, activePanel, isPlaying, togglePlayPause, handleAddKeyframe, beginBatch, commitBatch]);

  const previewCursorClass = useMemo(() => {
    if (isKeystrokeResizeDragging || isKeystrokeResizeHandleHover) return 'cursor-nwse-resize';
    if (isPreviewDragging) return 'cursor-grabbing';
    if (currentVideo && !isCropping) return 'cursor-grab';
    return 'cursor-default';
  }, [isKeystrokeResizeDragging, isKeystrokeResizeHandleHover, isPreviewDragging, currentVideo, isCropping]);

  const hasAppliedCrop = useMemo(() => {
    const crop = segment?.crop;
    if (!crop) return false;
    return (
      Math.abs(crop.x) > 0.0005 ||
      Math.abs(crop.y) > 0.0005 ||
      Math.abs(crop.width - 1) > 0.0005 ||
      Math.abs(crop.height - 1) > 0.0005
    );
  }, [segment?.crop?.x, segment?.crop?.y, segment?.crop?.width, segment?.crop?.height]);

  const handleBackgroundUpload = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const inputEl = e.currentTarget;
    const file = e.target.files?.[0];
    if (file) {
      setIsBackgroundUploadProcessing(true);
      const img = new Image();

      img.onload = async () => {
        try {
          // Cap backgrounds at 2.5K to ensure instant decode and zero lag.
          // The GPU shader scales it up using object-fit: cover.
          const MAX_DIM = 2560;
          let w = img.naturalWidth;
          let h = img.naturalHeight;
          if (w > MAX_DIM || h > MAX_DIM) {
            const ratio = Math.min(MAX_DIM / w, MAX_DIM / h);
            w = Math.round(w * ratio);
            h = Math.round(h * ratio);
          }

          const canvas = document.createElement('canvas');
          canvas.width = w;
          canvas.height = h;
          const ctx = canvas.getContext('2d');
          if (!ctx) throw new Error('Failed to get 2D canvas context');
          ctx.imageSmoothingEnabled = true;
          ctx.imageSmoothingQuality = 'high';
          ctx.drawImage(img, 0, 0, w, h);

          // Convert to JPEG to reduce IPC payload size (backgrounds do not need alpha).
          const dataUrl = canvas.toDataURL('image/jpeg', 0.92);
          const imageUrl = await invoke<string>('save_uploaded_bg_data_url', { dataUrl });
          await invoke('prewarm_custom_background', { url: imageUrl });
          setBackgroundConfig(prev => ({ ...prev, backgroundType: 'custom', customBackground: imageUrl }));
          setRecentUploads(prev => [imageUrl, ...prev.filter(v => v !== imageUrl)].slice(0, 12));
        } catch (err) {
          console.error('[Background] Failed to persist uploaded image:', err);
        } finally {
          URL.revokeObjectURL(img.src);
          setIsBackgroundUploadProcessing(false);
          inputEl.value = '';
        }
      };

      img.onerror = () => {
        URL.revokeObjectURL(img.src);
        setIsBackgroundUploadProcessing(false);
        inputEl.value = '';
      };

      img.src = URL.createObjectURL(file);
    }
  }, [setBackgroundConfig, setRecentUploads]);

  const handleRemoveRecentUpload = useCallback((imageUrl: string) => {
    setRecentUploads(prev => prev.filter(v => v !== imageUrl));
    setBackgroundConfig(prev => {
      if (prev.backgroundType === 'custom' && prev.customBackground === imageUrl) {
        return { ...prev, backgroundType: 'gradient2', customBackground: undefined };
      }
      return prev;
    });
  }, []);

  const getKeystrokeTimelineDuration = useCallback((s: VideoSegment) => {
    const segmentDuration = Math.max(
      s.trimEnd,
      ...(s.trimSegments || []).map((trimSegment) => trimSegment.endTime),
      duration
    );
    // Timeline tracks are rendered against `duration`; visibility segments must stay inside it.
    if (duration > 0) return duration;
    return segmentDuration;
  }, [duration]);

  const keystrokeOverlayEditBounds = useMemo<KeystrokeOverlayEditBounds | null>(() => {
    if (!segment || !canvasRef.current || (segment.keystrokeMode ?? 'off') === 'off') return null;
    return videoRenderer.getKeystrokeOverlayEditBounds(
      segment,
      canvasRef.current,
      currentTime,
      getKeystrokeTimelineDuration(segment)
    );
  }, [segment, currentTime, getKeystrokeTimelineDuration]);

  const keystrokeOverlayEditFrame = useMemo(() => {
    if (!keystrokeOverlayEditBounds || !canvasRef.current || !previewContainerRef.current) return null;
    const canvasRect = canvasRef.current.getBoundingClientRect();
    const previewRect = previewContainerRef.current.getBoundingClientRect();
    const scaleX = canvasRect.width / Math.max(1, canvasRef.current.width);
    const scaleY = canvasRect.height / Math.max(1, canvasRef.current.height);
    return {
      left: (canvasRect.left - previewRect.left) + keystrokeOverlayEditBounds.x * scaleX,
      top: (canvasRect.top - previewRect.top) + keystrokeOverlayEditBounds.y * scaleY,
      width: keystrokeOverlayEditBounds.width * scaleX,
      height: keystrokeOverlayEditBounds.height * scaleY,
      handleSize: Math.max(8, keystrokeOverlayEditBounds.handleSize * Math.min(scaleX, scaleY)),
    };
  }, [keystrokeOverlayEditBounds]);

  useEffect(() => {
    if (!segment || (segment.keystrokeMode ?? 'off') === 'off') {
      setIsKeystrokeOverlaySelected(false);
    }
  }, [segment]);

  const handleAddKeystrokeSegment = useCallback((atTime?: number) => {
    if (!segment || (segment.keystrokeMode ?? 'off') === 'off') return;
    const prepared = ensureKeystrokeVisibilitySegments(segment, getKeystrokeTimelineDuration(segment));
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

    setSegment(withKeystrokeVisibilitySegmentsForMode(prepared, [...currentSegments, newSeg]));
    setEditingKeystrokeSegmentId(null);
  }, [segment, currentTime, getKeystrokeTimelineDuration, setSegment]);

  const handleDeleteKeystrokeSegment = useCallback(() => {
    if (!segment || !editingKeystrokeSegmentId || (segment.keystrokeMode ?? 'off') === 'off') return;
    const prepared = ensureKeystrokeVisibilitySegments(segment, getKeystrokeTimelineDuration(segment));
    const currentSegments = getKeystrokeVisibilitySegmentsForMode(prepared);
    const remaining = currentSegments.filter((s) => s.id !== editingKeystrokeSegmentId);
    setSegment(withKeystrokeVisibilitySegmentsForMode(prepared, remaining));
    setEditingKeystrokeSegmentId(null);
  }, [segment, editingKeystrokeSegmentId, getKeystrokeTimelineDuration, setSegment]);

  const handleToggleKeystrokeMode = useCallback(() => {
    if (!segment) return;
    const timelineDuration = getKeystrokeTimelineDuration(segment);
    let prepared = ensureKeystrokeVisibilitySegments(segment, timelineDuration);
    const current = segment.keystrokeMode ?? 'off';
    const next: KeystrokeMode =
      current === 'off' ? 'keyboard' : current === 'keyboard' ? 'keyboardMouse' : 'off';

    if (next === 'keyboard' || next === 'keyboardMouse') {
      // Toggle intent = reset to fresh auto-generated visibility ranges for that mode.
      prepared = rebuildKeystrokeVisibilitySegmentsForMode(prepared, next, timelineDuration);
    }

    setSegment({
      ...prepared,
      keystrokeMode: next,
      keystrokeDelaySec: prepared.keystrokeDelaySec ?? DEFAULT_KEYSTROKE_DELAY_SEC,
      keystrokeEvents: prepared.keystrokeEvents ?? [],
    });
    setEditingKeystrokeSegmentId(null);
  }, [segment, setSegment, getKeystrokeTimelineDuration]);

  const handleKeystrokeDelayChange = useCallback((value: number) => {
    if (!segment) return;
    const clamped = Math.max(-1, Math.min(1, value));
    const prevDelay = Math.max(-1, Math.min(1, segment.keystrokeDelaySec ?? DEFAULT_KEYSTROKE_DELAY_SEC));
    const delta = clamped - prevDelay;
    const mode = segment.keystrokeMode ?? 'off';

    let nextSegment: VideoSegment = {
      ...segment,
      keystrokeDelaySec: clamped,
    };

    if ((mode === 'keyboard' || mode === 'keyboardMouse') && Math.abs(delta) > 0.0005) {
      const duration = getKeystrokeTimelineDuration(segment);
      const shifted = getKeystrokeVisibilitySegmentsForMode(segment)
        .map((range) => {
          const startTime = Math.max(0, Math.min(duration, range.startTime + delta));
          const endTime = Math.max(0, Math.min(duration, range.endTime + delta));
          if (endTime - startTime <= 0.001) return null;
          return {
            ...range,
            startTime,
            endTime,
          };
        })
        .filter((range): range is NonNullable<typeof range> => Boolean(range));
      nextSegment = withKeystrokeVisibilitySegmentsForMode(
        nextSegment,
        clampVisibilitySegmentsToDuration(shifted, duration)
      );
    } else if (mode === 'keyboard' || mode === 'keyboardMouse') {
      const duration = getKeystrokeTimelineDuration(segment);
      nextSegment = withKeystrokeVisibilitySegmentsForMode(
        nextSegment,
        clampVisibilitySegmentsToDuration(getKeystrokeVisibilitySegmentsForMode(segment), duration)
      );
    }

    setSegment(nextSegment);
    try { localStorage.setItem(KEYSTROKE_DELAY_KEY, String(clamped)); } catch { /* ignore */ }
  }, [segment, setSegment, getKeystrokeTimelineDuration]);

  // Persist keystroke mode preference so new recordings remember the last setting.
  useEffect(() => {
    if (!segment?.keystrokeMode) return;
    try { localStorage.setItem(KEYSTROKE_MODE_PREF_KEY, segment.keystrokeMode); } catch { /* ignore */ }
  }, [segment?.keystrokeMode]);

  // Persist keystroke overlay position/scale so new recordings inherit the last layout.
  useEffect(() => {
    if (!segment?.keystrokeOverlay) return;
    try { localStorage.setItem(KEYSTROKE_OVERLAY_PREF_KEY, JSON.stringify(segment.keystrokeOverlay)); } catch { /* ignore */ }
  }, [segment?.keystrokeOverlay]);

  // Persist crop preference so newly recorded/imported videos inherit the last crop.
  useEffect(() => {
    if (!segment) return;
    saveCropPref(segment.crop);
  }, [segment?.crop?.x, segment?.crop?.y, segment?.crop?.width, segment?.crop?.height, segment]);


  const persistCurrentProjectNow = useCallback(async (options?: { refreshList?: boolean; includeMedia?: boolean }) => {
    if (!projects.currentProjectId || !currentVideo || !segment) return;
    const projectId = projects.currentProjectId;
    const saveSeq = ++projectSaveSeqRef.current;
    const includeMedia = options?.includeMedia !== false;
    debugProject('persist:start', {
      saveSeq,
      projectId,
      refreshList: options?.refreshList ?? true,
      includeMedia,
      canvasMode: backgroundConfig.canvasMode,
      canvasWidth: backgroundConfig.canvasWidth,
      canvasHeight: backgroundConfig.canvasHeight
    });
    try {
      let videoBlob: Blob | undefined;
      let thumbnail: string | undefined;
      if (includeMedia) {
        // Use the currently rendered preview frame whenever possible so the
        // project card thumbnail matches exactly what the user just saw.
        const canvasSnapshot = (() => {
          try { return canvasRef.current?.toDataURL('image/jpeg', 0.8); }
          catch { return undefined; }
        })();

        const response = await fetch(currentVideo);
        videoBlob = await response.blob();
        const generated = await videoControllerRef.current?.generateThumbnail({
          segment, backgroundConfig, mousePositions
        });
        thumbnail = canvasSnapshot || generated || generateThumbnail();
      }
      // Drop stale in-flight saves so older state never overwrites newer edits.
      if (saveSeq !== projectSaveSeqRef.current) {
        debugProject('persist:stale-before-write', { saveSeq, latestSeq: projectSaveSeqRef.current, projectId });
        return;
      }
      await projectManager.updateProject(projectId, {
        name: projects.projects.find(p => p.id === projectId)?.name || "Auto Saved",
        videoBlob, segment, backgroundConfig, mousePositions, thumbnail,
        duration: videoControllerRef.current?.duration || duration,
        recordingMode: currentRecordingMode,
        rawVideoPath: currentRawVideoPath || undefined
      });
      if (saveSeq !== projectSaveSeqRef.current) {
        debugProject('persist:stale-after-write', { saveSeq, latestSeq: projectSaveSeqRef.current, projectId });
        return;
      }
      debugProject('persist:committed', {
        saveSeq,
        projectId,
        canvasMode: backgroundConfig.canvasMode,
        canvasWidth: backgroundConfig.canvasWidth,
        canvasHeight: backgroundConfig.canvasHeight
      });
      if (options?.refreshList !== false) {
        await projects.loadProjects();
        debugProject('persist:projects-refreshed', { saveSeq, projectId });
      }
    } catch (error) {
      debugProject('persist:error', { saveSeq, projectId, error: String(error) });
    }
  }, [
    projects.currentProjectId, projects.projects, projects.loadProjects,
    currentVideo, segment, backgroundConfig, mousePositions,
    generateThumbnail, duration, debugProject, currentRecordingMode, currentRawVideoPath
  ]);
  persistRef.current = persistCurrentProjectNow;

  const handleLoadProjectFromGrid = useCallback(async (projectId: string) => {
    // Always persist the currently open project before loading another one.
    debugProject('grid-load:start', { targetProjectId: projectId, currentProjectId: projects.currentProjectId });
    if (projectId === projects.currentProjectId) {
      projects.setShowProjectsDialog(false);
      debugProject('grid-load:same-project-close', { targetProjectId: projectId });
      return;
    }
    void persistCurrentProjectNow({ refreshList: false, includeMedia: false });
    setLastCaptureFps(null); // loading a different project — probe should determine its FPS
    await projects.handleLoadProject(projectId);
    debugProject('grid-load:done', { targetProjectId: projectId });
  }, [persistCurrentProjectNow, projects, debugProject]);

  const requestCloseProjects = useCallback(() => {
    if (!projects.showProjectsDialog) return;
    window.dispatchEvent(new CustomEvent('sr-close-projects'));
  }, [projects.showProjectsDialog]);

  const handleToggleProjects = useCallback(async () => {
    if (projects.showProjectsDialog) {
      debugProject('projects-toggle:close');
      requestCloseProjects();
      return;
    }

    debugProject('projects-toggle:open:start', {
      currentProjectId: projects.currentProjectId,
      canvasMode: backgroundConfig.canvasMode,
      canvasWidth: backgroundConfig.canvasWidth,
      canvasHeight: backgroundConfig.canvasHeight
    });
    // Persist in background to keep opening Projects instant.
    void persistCurrentProjectNow({ refreshList: true, includeMedia: false });

    if (canvasRef.current && currentVideo) {
      try { restoreImageRef.current = canvasRef.current.toDataURL('image/jpeg', 0.8); }
      catch { restoreImageRef.current = null; }
    } else {
      restoreImageRef.current = null;
    }
    projects.setShowProjectsDialog(true);
    debugProject('projects-toggle:open:done', { currentProjectId: projects.currentProjectId });
  }, [projects.showProjectsDialog, projects.currentProjectId, currentVideo, backgroundConfig.canvasMode, backgroundConfig.canvasWidth, backgroundConfig.canvasHeight, persistCurrentProjectNow, debugProject, projects, requestCloseProjects]);

  // Persist canvas mode/size changes quickly so reopening projects can't
  // resurrect stale custom-canvas settings from an older autosave.
  useEffect(() => {
    if (!projects.currentProjectId || !currentVideo || !segment) return;
    const timer = setTimeout(() => {
      void persistRef.current?.({ refreshList: false, includeMedia: false });
    }, 500);
    return () => clearTimeout(timer);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    projects.currentProjectId,
    currentVideo,
    backgroundConfig.canvasMode,
    backgroundConfig.canvasWidth,
    backgroundConfig.canvasHeight,
  ]);

  const handleSelectMonitorCapture = useCallback((monitorId: string, fps: number | null) => {
    setCaptureSource('monitor');
    setCaptureFps(fps);
    captureFpsRef.current = fps;
    setCaptureTargetId(monitorId);
  }, []);

  const handleSelectWindowCapture = useCallback((fps: number | null) => {
    setCaptureSource('window');
    setCaptureFps(fps);
    captureFpsRef.current = fps;
    setCaptureTargetId('0');
  }, []);

  const finalizeStartRecording = useCallback(async (
    targetId: string,
    targetType: 'monitor' | 'window'
  ) => {
    await startNewRecording(
      targetId,
      selectedRecordingMode,
      targetType,
      captureFpsRef.current ?? undefined
    );
  }, [startNewRecording, selectedRecordingMode]);

  const handleSelectWindowForRecording = useCallback(async (
    windowId: string,
    _captureMethod: 'game' | 'window'
  ) => {
    setShowWindowSelect(false);
    setCaptureTargetId(windowId);
    if (!pendingWindowRecordingRef.current) return;
    pendingWindowRecordingRef.current = false;
    try {
      await finalizeStartRecording(windowId, 'window');
    } catch (err) {
      setError(err as string);
    }
  }, [finalizeStartRecording, setError]);

  const handleStartRecording = useCallback(async () => {
    if (isRecording || pendingWindowRecordingRef.current) return;
    try {
      if (projects.currentProjectId && currentVideo && segment) {
        await persistCurrentProjectNow({ refreshList: false, includeMedia: false });
      }
      projects.setCurrentProjectId(null);
      setCurrentRecordingMode(selectedRecordingMode);
      setCurrentRawVideoPath('');
      setLastRawSavedPath('');
      setRawButtonSavedFlash(false);

      let finalTargetId = captureTargetId;
      if (captureSource === 'monitor' && (!finalTargetId || finalTargetId === '0')) {
        const monitorList = monitors.length > 0 ? monitors : await getMonitors();
        const primary = monitorList.find(m => m.is_primary) ?? monitorList[0];
        finalTargetId = primary?.id ?? '0';
      }

      if (captureSource === 'window') {
        pendingWindowRecordingRef.current = true;
        try {
          await invoke('show_window_selector');
        } catch (err) {
          pendingWindowRecordingRef.current = false;
          throw err;
        }
        return;
      }

      await finalizeStartRecording(finalTargetId, captureSource);
    } catch (err) { setError(err as string); }
  }, [
    isRecording,
    projects,
    currentVideo,
    segment,
    persistCurrentProjectNow,
    selectedRecordingMode,
    captureSource,
    captureTargetId,
    monitors,
    getMonitors,
    finalizeStartRecording,
    setError
  ]);

  // Listen for window selections dispatched by the native overlay via IPC.
  useEffect(() => {
    const handler = (event: Event) => {
      const { windowId } = (event as CustomEvent<{ windowId: string }>).detail;
      handleSelectWindowForRecording(windowId, 'window');
    };
    window.addEventListener('external-window-selected', handler);
    return () => window.removeEventListener('external-window-selected', handler);
  }, [handleSelectWindowForRecording]);

  useEffect(() => {
    const handler = () => {
      pendingWindowRecordingRef.current = false;
    };
    window.addEventListener('external-window-selection-cancelled', handler);
    return () => window.removeEventListener('external-window-selection-cancelled', handler);
  }, []);

  const onStopRecording = useCallback(async () => {
    setShowRawVideoDialog(false);
    exportHook.setShowExportSuccessDialog(false);
    const result = await handleStopRecording();
    if (result) {
      requestCloseProjects();
      const { mouseData, initialSegment, videoUrl, recordingMode, rawVideoPath, capturedFps } = result;
      setLastCaptureFps(capturedFps);
      setCurrentRecordingMode(recordingMode);
      setCurrentRawVideoPath(rawVideoPath || '');
      setLastRawSavedPath('');

      let autoSavedPath = '';
      if (rawAutoCopyEnabled && rawVideoPath && rawSaveDir) {
        try {
          setIsRawActionBusy(true);
          const saved = await invoke<{ savedPath: string }>('save_raw_video_copy', {
            sourcePath: rawVideoPath,
            targetDir: rawSaveDir,
          });
          autoSavedPath = saved?.savedPath || '';
          if (autoSavedPath) {
            setLastRawSavedPath(autoSavedPath);
            await invoke('copy_video_file_to_clipboard', { filePath: autoSavedPath });
            flashRawSavedButton();
          }
        } catch (e) {
          console.error('[RawVideo] Auto-copy after recording failed:', e);
        } finally {
          setIsRawActionBusy(false);
        }
      }

      const response = await fetch(videoUrl);
      const videoBlob = await response.blob();
      const thumbnail = await videoControllerRef.current?.generateThumbnail({
        segment: initialSegment, backgroundConfig, mousePositions: mouseData
      }) || generateThumbnail();
      const project = await projectManager.saveProject({
        name: `Recording ${new Date().toLocaleString()}`,
        videoBlob, segment: initialSegment, backgroundConfig, mousePositions: mouseData, thumbnail,
        duration: initialSegment.trimEnd,
        recordingMode,
        rawVideoPath: rawVideoPath || undefined
      });
      projects.setCurrentProjectId(project.id);
      await projects.loadProjects();
    }
  }, [handleStopRecording, backgroundConfig, generateThumbnail, projects, rawAutoCopyEnabled, rawSaveDir, flashRawSavedButton, setShowRawVideoDialog, exportHook, requestCloseProjects]);

  // Effects
  useEffect(() => {
    const handleToggle = () => {
      if (showHotkeyDialog) return;
      if (isRecording) onStopRecording();
      else handleStartRecording();
    };
    window.addEventListener('toggle-recording', handleToggle);
    return () => window.removeEventListener('toggle-recording', handleToggle);
  }, [isRecording, showHotkeyDialog, onStopRecording, handleStartRecording]);

  // Keyboard shortcuts
  useAppShortcuts({
    togglePlayPause, currentTime, duration, seek, isCropping,
    isModalOpen: showRawVideoDialog || exportHook.showExportSuccessDialog,
    editingKeyframeId, editingTextId, editingKeystrokeSegmentId, editingPointerId,
    segment, setSegment, setEditingKeyframeId,
    handleDeleteText, handleDeleteKeystrokeSegment, handleDeletePointerSegment,
    canUndo, canRedo, undo, redo,
    setSeekIndicatorKey, setSeekIndicatorDir,
  });

  // Wheel zoom
  useEffect(() => {
    const container = previewContainerRef.current;
    if (!container) return;

    const handleWheel = (e: WheelEvent) => {
      if (!currentVideo || isCropping) return;
      e.preventDefault();
      const lastState = videoRenderer.getLastCalculatedState();
      if (!lastState) return;

      if (!wheelBatchActiveRef.current) {
        beginBatch();
        wheelBatchActiveRef.current = true;
      }
      if (wheelBatchTimerRef.current) clearTimeout(wheelBatchTimerRef.current);
      wheelBatchTimerRef.current = setTimeout(() => {
        commitBatch();
        wheelBatchActiveRef.current = false;
        wheelBatchTimerRef.current = null;
      }, 400);

      const newZoom = Math.max(1.0, Math.min(12.0, lastState.zoomFactor - e.deltaY * 0.002 * lastState.zoomFactor));
      handleAddKeyframe({ zoomFactor: newZoom, positionX: lastState.positionX, positionY: lastState.positionY });
      setActivePanel('zoom');
    };

    container.addEventListener('wheel', handleWheel, { passive: false });
    return () => container.removeEventListener('wheel', handleWheel);
  }, [currentVideo, isCropping, handleAddKeyframe, beginBatch, commitBatch]);

  // Initialize segment
  useEffect(() => {
    if (duration > 0 && !segment) {
      const initialSegment: VideoSegment = {
          trimStart: 0,
          trimEnd: duration,
          trimSegments: [{
            id: crypto.randomUUID(),
            startTime: 0,
            endTime: duration,
          }],
          zoomKeyframes: [],
          textSegments: [],
          speedPoints: [{ time: 0, speed: 1 }, { time: duration, speed: 1 }],
          keystrokeMode: getSavedKeystrokeModePref(),
          keystrokeDelaySec: DEFAULT_KEYSTROKE_DELAY_SEC,
          keystrokeLanguage: getSavedKeystrokeLanguage(),
          keystrokeEvents: [],
          keyboardVisibilitySegments: [],
          keyboardMouseVisibilitySegments: [],
          keystrokeOverlay: getSavedKeystrokeOverlayPref(),
          crop: getSavedCropPref(),
          useCustomCursor: true,
        };
        setSegment(initialSegment);
      setTimeout(() => {
        if (videoRef.current && canvasRef.current && videoRef.current.readyState >= 2) {
          videoRenderer.drawFrame({
            video: videoRef.current, canvas: canvasRef.current, tempCanvas: tempCanvasRef.current,
            segment: initialSegment, backgroundConfig, mousePositions, currentTime: 0
          });
        }
      }, 0);
    }
  }, [duration, segment, backgroundConfig, mousePositions, setSegment, videoRef, canvasRef, tempCanvasRef]);

  // Auto-save — debounced, skips during playback/export/recording to avoid jank
  useEffect(() => {
    if (!projects.currentProjectId || !currentVideo || !segment) return;
    const timer = setTimeout(() => {
      // Skip save during activities that need smooth performance
      if (videoRef.current && !videoRef.current.paused) return;
      if (exportHook.isProcessing) return;
      if (isRecording) return;
      void persistRef.current?.({ refreshList: true, includeMedia: true });
    }, 3000);
    return () => clearTimeout(timer);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [segment, backgroundConfig, mousePositions, projects.currentProjectId, currentVideo]);

  // Text & keystroke overlay drag listeners
  useKeystrokeDrag({
    segment, setSegment, canvasRef, segmentRef,
    isDraggingKeystrokeOverlayRef, isResizingKeystrokeOverlayRef, keystrokeOverlayDragStartRef,
    currentTime, getKeystrokeTimelineDuration,
    setIsPreviewDragging, setIsKeystrokeResizeDragging, setIsKeystrokeResizeHandleHover,
    setIsKeystrokeOverlaySelected, setEditingTextId, setActivePanel,
    handleTextDragMove, beginBatch, commitBatch,
  });

  return (
    <SettingsContext.Provider value={settings}>
    <div className="app-container min-h-screen bg-[var(--surface)]">
      <ResizeBorders />
      <Header
        isRecording={isRecording} recordingDuration={recordingDuration} currentVideo={currentVideo}
        isProcessing={exportHook.isProcessing} hotkeys={hotkeys}
        onRemoveHotkey={handleRemoveHotkey} onOpenHotkeyDialog={openHotkeyDialog}
        recordingMode={selectedRecordingMode}
        onRecordingModeChange={setSelectedRecordingMode}
        rawButtonLabel={rawButtonSavedFlash ? t.rawVideoSavedButton : t.saveRawVideo}
        rawButtonPulse={currentRecordingMode === 'withCursor'}
        rawButtonDisabled={!currentRawVideoPath && !lastRawSavedPath}
        onOpenRawVideoDialog={handleOpenRawVideoDialog}
        onExport={exportHook.handleExport}
        onOpenProjects={handleToggleProjects}
        onOpenCursorLab={() => { window.location.hash = 'cursor-lab'; }}
        hideExport={isOverlayMode}
        hideRawVideo={projects.showProjectsDialog}
        captureSource={captureSource}
        captureFps={captureFps}
        monitors={monitors}
        onSelectMonitorCapture={handleSelectMonitorCapture}
        onSelectWindowCapture={handleSelectWindowCapture}
      />

      <main className="app-main flex flex-col px-3 py-3 overflow-hidden" style={{ height: 'calc(100vh - 44px)' }}>
        {error && <p className="error-message text-[var(--tertiary-color)] mb-2 flex-shrink-0">{error}</p>}

        <div className="content-layout flex gap-4 flex-1 min-h-0 pb-1">
          <div className="preview-and-controls flex-1 flex flex-col min-w-0 gap-3 relative">
            {/* Video Preview */}
            <div className="video-preview-container flex-1 min-h-0 overflow-hidden bg-[var(--surface-dim)]/80 backdrop-blur-2xl flex items-center justify-center shadow-[0_2px_12px_rgba(0,0,0,0.06)] dark:shadow-[0_4px_18px_rgba(0,0,0,0.3)] border border-[var(--glass-border)]">
              <div className="preview-inner relative w-full h-full flex justify-center items-center">
                <div
                  ref={previewContainerRef}
                  className={`preview-canvas relative flex items-center justify-center ${previewCursorClass} group w-full h-full`}
                  onMouseDown={handlePreviewMouseDown}
                >
                  <canvas ref={canvasRef} className="preview-canvas-element absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 max-w-full max-h-full" />
                  <canvas ref={tempCanvasRef} className="hidden" />
                  <video ref={videoRef} className="hidden" playsInline preload="auto" />
                  <audio ref={audioRef} className="hidden" />
                  {keystrokeOverlayEditFrame && (isKeystrokeOverlaySelected || isDraggingKeystrokeOverlayRef.current || isResizingKeystrokeOverlayRef.current) && (
                    <div
                      className="keystroke-overlay-edit-frame absolute z-30 pointer-events-none"
                      style={{
                        left: `${keystrokeOverlayEditFrame.left}px`,
                        top: `${keystrokeOverlayEditFrame.top}px`,
                        width: `${keystrokeOverlayEditFrame.width}px`,
                        height: `${keystrokeOverlayEditFrame.height}px`,
                      }}
                    >
                      <div className="keystroke-overlay-edit-outline absolute inset-0 rounded-lg border border-emerald-300/85 bg-emerald-400/8 shadow-[0_0_0_1px_rgba(0,0,0,0.28)]" />
                      <div
                        className="keystroke-overlay-edit-handle absolute rounded-sm border border-emerald-100/90 bg-emerald-300/95 shadow-[0_2px_8px_rgba(0,0,0,0.35)]"
                        style={{
                          width: `${keystrokeOverlayEditFrame.handleSize}px`,
                          height: `${keystrokeOverlayEditFrame.handleSize}px`,
                          right: `${Math.max(-keystrokeOverlayEditFrame.handleSize * 0.35, -6)}px`,
                          bottom: `${Math.max(-keystrokeOverlayEditFrame.handleSize * 0.35, -6)}px`,
                        }}
                      />
                    </div>
                  )}

                  {(!currentVideo || isLoadingVideo) && (
                    <Placeholder isLoadingVideo={isLoadingVideo} loadingProgress={loadingProgress}
                      isRecording={isRecording} recordingDuration={recordingDuration} />
                  )}

                  {isCropping && currentVideo && segment && (
                    <CropOverlay segment={segment}
                      mousePositions={mousePositions}
                      currentTime={currentTime}
                      previewContainerRef={previewContainerRef as React.RefObject<HTMLDivElement>}
                      videoRef={videoRef as React.RefObject<HTMLVideoElement>}
                      onUpdateSegment={setSegment}
                      beginBatch={beginBatch} commitBatch={commitBatch} />
                  )}

                  {!isCropping && currentVideo && backgroundConfig.canvasMode === 'custom' && backgroundConfig.canvasWidth && backgroundConfig.canvasHeight && (
                    <CanvasResizeOverlay
                      previewContainerRef={previewContainerRef as React.RefObject<HTMLDivElement>}
                      backgroundConfig={backgroundConfig}
                      setBackgroundConfig={setBackgroundConfig}
                      beginBatch={beginBatch}
                      commitBatch={commitBatch}
                    />
                  )}

                  <SeekIndicator dir={seekIndicatorDir} showKey={seekIndicatorKey} />
                </div>

              </div>
            </div>

            <div className={`playback-controls-row flex-shrink-0 flex justify-center pb-1 min-h-[56px] transition-opacity duration-200 ${currentVideo && !isLoadingVideo && !projects.showProjectsDialog ? 'opacity-100' : 'opacity-0 pointer-events-none'}`}>
              {currentVideo && !isLoadingVideo && (
                <PlaybackControls isPlaying={isPlaying} isProcessing={exportHook.isProcessing}
                  isVideoReady={isVideoReady} isCropping={isCropping} hasAppliedCrop={hasAppliedCrop} currentTime={currentTime}
                  duration={duration} wallClockCurrentTime={wallClockCurrentTime} wallClockDuration={wallClockDuration}
                  onTogglePlayPause={togglePlayPause} onToggleCrop={handleToggleCrop}
                  canvasModeToggle={
                  <div className="playback-canvas-mode-toggle flex rounded-lg border border-[var(--overlay-divider)] overflow-hidden">
                    {(['auto', 'custom'] as const).map((mode) => {
                      const isActive = (backgroundConfig.canvasMode ?? 'auto') === mode;
                      return (
                        <button
                          key={mode}
                          type="button"
                          aria-pressed={isActive}
                          data-active={isActive ? 'true' : 'false'}
                          onClick={() => {
                            if (mode === 'custom') {
                              setBackgroundConfig((prev) => {
                                const w = prev.canvasWidth ?? canvasRef.current?.width ?? 1920;
                                const h = prev.canvasHeight ?? canvasRef.current?.height ?? 1080;
                                return { ...prev, canvasMode: 'custom', canvasWidth: w, canvasHeight: h };
                              });
                            } else {
                              setBackgroundConfig((prev) => ({ ...prev, canvasMode: 'auto' }));
                            }
                          }}
                          className={`playback-canvas-mode-btn playback-canvas-mode-btn-${mode} ${isActive ? 'playback-canvas-mode-btn-active' : 'playback-canvas-mode-btn-inactive'} px-2 py-1 text-[10px] font-semibold transition-colors ${
                            isActive
                              ? 'bg-[var(--primary-color)] text-white ring-1 ring-white/45 shadow-[inset_0_0_0_1px_rgba(255,255,255,0.26),0_0_0_1px_rgba(0,0,0,0.28)]'
                              : 'bg-transparent text-[var(--overlay-panel-fg)]/70 hover:bg-[var(--glass-bg)]/70 hover:text-[var(--overlay-panel-fg)]'
                          }`}
                        >
                          {mode === 'auto' ? t.canvasAuto : t.canvasCustom}
                        </button>
                      );
                    })}
                  </div>
                }
                  keystrokeToggle={
                  <div className="playback-keystroke-control relative">
                    <div className="playback-keystroke-delay-hover-bridge absolute left-0 right-0 bottom-full h-3" />
                    <Button
                      onClick={handleToggleKeystrokeMode}
                      disabled={!segment}
                      className={`playback-keystroke-toggle-btn h-7 text-[11px] transition-colors ${
                        !segment
                          ? 'text-[var(--overlay-panel-fg)]/40 cursor-not-allowed'
                          : (segment.keystrokeMode ?? 'off') === 'off'
                            ? 'text-[var(--overlay-panel-fg)]/85 bg-transparent hover:bg-[var(--glass-bg)]'
                            : 'text-white bg-[var(--primary-color)] hover:bg-[var(--primary-color)]/85'
                      }`}
                    >
                      <Keyboard className="playback-keystroke-toggle-icon w-3.5 h-3.5 mr-1.5" />
                      <span className="playback-keystroke-toggle-label">
                        {(segment?.keystrokeMode ?? 'off') === 'keyboard'
                          ? t.keystrokeModeKeyboard
                          : (segment?.keystrokeMode ?? 'off') === 'keyboardMouse'
                            ? t.keystrokeModeKeyboardMouse
                            : t.keystrokeModeOff}
                      </span>
                    </Button>
                    <div
                      className="playback-keystroke-delay-popover absolute left-1/2 -translate-x-1/2 bottom-[calc(100%+4px)] min-w-[172px] px-2 py-1.5 rounded-lg border pointer-events-none opacity-0 translate-y-1 transition-all duration-150 group-hover/playback-keystroke:opacity-100 group-hover/playback-keystroke:translate-y-0 group-hover/playback-keystroke:pointer-events-auto group-focus-within/playback-keystroke:opacity-100 group-focus-within/playback-keystroke:translate-y-0 group-focus-within/playback-keystroke:pointer-events-auto"
                    >
                      <div className="playback-keystroke-delay-row flex items-center gap-2">
                        <div className="playback-keystroke-delay-slider-shell flex-1 rounded-full px-1 py-[3px]">
                          <input
                            type="range"
                            min="-1"
                            max="1"
                            step="0.01"
                            disabled={!segment}
                            value={segment?.keystrokeDelaySec ?? DEFAULT_KEYSTROKE_DELAY_SEC}
                            style={sv(segment?.keystrokeDelaySec ?? DEFAULT_KEYSTROKE_DELAY_SEC, -1, 1)}
                            onChange={(e) => handleKeystrokeDelayChange(Number(e.target.value))}
                            className="playback-keystroke-delay-slider block w-full"
                          />
                        </div>
                        <span className="playback-keystroke-delay-value text-[10px] tabular-nums text-[var(--overlay-panel-fg)]/86 w-11 text-right">
                          {(segment?.keystrokeDelaySec ?? DEFAULT_KEYSTROKE_DELAY_SEC).toFixed(2)}s
                        </span>
                      </div>
                      <div className="playback-keystroke-language-row flex items-center gap-2 mt-1">
                        <span className="playback-keystroke-language-label text-[10px] text-[var(--overlay-panel-fg)]/60 flex-1">{t.keystrokeLanguageLabel}</span>
                        <div className="playback-keystroke-language-toggle flex flex-wrap rounded-md overflow-hidden border border-[var(--glass-border)]">
                          {(['en', 'ko', 'vi', 'es', 'ja', 'zh'] as const).map(lang => (
                            <button
                              key={lang}
                              className={`playback-keystroke-language-btn px-2 py-0.5 text-[10px] uppercase transition-colors ${(segment?.keystrokeLanguage ?? 'en') === lang ? 'bg-[var(--primary-color)] text-white' : 'text-[var(--overlay-panel-fg)]/70 hover:bg-[var(--glass-bg)]'}`}
                              onClick={() => {
                                if (!segment) return;
                                saveKeystrokeLanguage(lang);
                                setSegment({ ...segment, keystrokeLanguage: lang });
                              }}
                              disabled={!segment}
                            >{lang}</button>
                          ))}
                        </div>
                      </div>
                    </div>
                  </div>
                  }
                  autoZoomButton={
                  <Button onClick={handleAutoZoom}
                    disabled={exportHook.isProcessing || !currentVideo || (!mousePositions.length && !segment?.smoothMotionPath?.length)}
                    className={`flex items-center px-2.5 py-1 h-7 text-xs font-medium transition-colors whitespace-nowrap rounded-lg ${
                      !currentVideo || exportHook.isProcessing || (!mousePositions.length && !segment?.smoothMotionPath?.length)
                        ? 'bg-[var(--surface-container)]/50 text-[var(--on-surface)]/35 cursor-not-allowed'
                        : segment?.smoothMotionPath?.length
                          ? 'bg-[var(--success-color)] hover:bg-[var(--success-color)]/85 text-white'
                          : 'bg-[var(--glass-bg)] hover:bg-[var(--glass-bg-hover)] text-[var(--on-surface)]'
                    }`}>
                    <Wand2 className="w-3 h-3 mr-1" />{t.autoZoom}
                  </Button>
                  }
                  smartPointerButton={
                  <Button onClick={handleSmartPointerHiding}
                    disabled={exportHook.isProcessing || !currentVideo || (() => {
                      const segs = segment?.cursorVisibilitySegments;
                      const isActive = !!segs?.length && !(
                        segs.length === 1 &&
                        Math.abs(segs[0].startTime - 0) < 0.01 &&
                        Math.abs(segs[0].endTime - duration) < 0.01
                      );
                      return !mousePositions.length && !isActive;
                    })()}
                    className={`flex items-center px-2.5 py-1 h-7 text-xs font-medium transition-colors whitespace-nowrap rounded-lg ${
                      (() => {
                        const segs = segment?.cursorVisibilitySegments;
                        const isActive = !!segs?.length && !(
                          segs.length === 1 &&
                          Math.abs(segs[0].startTime - 0) < 0.01 &&
                          Math.abs(segs[0].endTime - duration) < 0.01
                        );
                        if (!currentVideo || exportHook.isProcessing || (!mousePositions.length && !isActive))
                          return 'bg-[var(--surface-container)]/50 text-[var(--on-surface)]/35 cursor-not-allowed';
                        return isActive
                          ? 'bg-[var(--success-color)] hover:bg-[var(--success-color)]/85 text-white'
                          : 'bg-[var(--glass-bg)] hover:bg-[var(--glass-bg-hover)] text-[var(--on-surface)]';
                      })()
                    }`}>
                    <MousePointer2 className="w-3 h-3 mr-1" />{t.smartPointer}
                  </Button>
                  }
                  volumeControl={
                  <div className="playback-volume-control flex items-center gap-1.5">
                    <Volume2 className="w-3.5 h-3.5 text-[var(--overlay-panel-fg)]/80 flex-shrink-0" />
                    <input
                      type="range"
                      min="0"
                      max="1"
                      step="0.01"
                      value={backgroundConfig.volume ?? 1}
                      style={sv(backgroundConfig.volume ?? 1, 0, 1)}
                      onChange={(e) => setBackgroundConfig((prev) => ({ ...prev, volume: Number(e.target.value) }))}
                      className="playback-volume-slider w-20"
                    />
                  </div>
                } />
              )}
            </div>
          </div>

          {/* Side Panel */}
          <div className="side-panel-container w-[24rem] flex-shrink-0 min-h-0 relative overflow-visible">
            <SidePanel
              activePanel={activePanel} setActivePanel={setActivePanel} segment={segment}
              editingKeyframeId={editingKeyframeId} zoomFactor={zoomFactor} setZoomFactor={setZoomFactor}
              onDeleteKeyframe={handleDeleteKeyframe} onUpdateZoom={throttledUpdateZoom}
              backgroundConfig={backgroundConfig} setBackgroundConfig={setBackgroundConfig}
              recentUploads={recentUploads} onRemoveRecentUpload={handleRemoveRecentUpload}
              onBackgroundUpload={handleBackgroundUpload}
              isBackgroundUploadProcessing={isBackgroundUploadProcessing}
              editingTextId={editingTextId} onUpdateSegment={setSegment}
              beginBatch={beginBatch} commitBatch={commitBatch}
            />
            {isOverlayMode && <div className="panel-block-overlay absolute inset-0 bg-[var(--surface)] z-50 rounded-xl" />}
          </div>
        </div>

        {/* Timeline */}
        <div className={`timeline-container mt-3 flex-shrink-0 relative ${isOverlayMode ? 'overflow-hidden' : ''}`}>
          <TimelineArea
            duration={duration} currentTime={currentTime} segment={segment} thumbnails={thumbnails}
            timelineRef={timelineRef} videoRef={videoRef} editingKeyframeId={editingKeyframeId}
            editingTextId={editingTextId} editingKeystrokeSegmentId={editingKeystrokeSegmentId} setCurrentTime={setCurrentTime}
            setEditingKeyframeId={setEditingKeyframeId} setEditingTextId={setEditingTextId}
            setEditingKeystrokeSegmentId={setEditingKeystrokeSegmentId}
            setEditingPointerId={setEditingPointerId}
            setActivePanel={setActivePanel} setSegment={setSegment} onSeek={seek} onSeekEnd={flushSeek}
            onAddText={handleAddText} onAddKeystrokeSegment={handleAddKeystrokeSegment} onAddPointerSegment={handleAddPointerSegment}
            isPlaying={isPlaying} beginBatch={beginBatch} commitBatch={commitBatch}
          />
          {isOverlayMode && <div className="timeline-block-overlay absolute inset-0 bg-[var(--surface)] z-50" />}
        </div>
      </main>

      {/* Absolute Projects View covering full screen below header */}
      {projects.showProjectsDialog && (
        <div className="absolute inset-0 top-[44px] z-[90]">
          <ProjectsView
            projects={projects.projects}
            onLoadProject={handleLoadProjectFromGrid}
            onProjectsChange={projects.loadProjects}
            onClose={() => projects.setShowProjectsDialog(false)}
            currentProjectId={projects.currentProjectId}
            restoreImage={restoreImageRef.current}
          />
        </div>
      )}

      {/* Dialogs */}
      <ProcessingOverlay show={exportHook.isProcessing} exportProgress={0} onCancel={exportHook.cancelExport} />
      <WindowSelectDialog
        show={showWindowSelect}
        onClose={() => setShowWindowSelect(false)}
        windows={windows}
        onSelectWindow={handleSelectWindowForRecording}
      />
      {currentVideo && !isVideoReady && !projects.showProjectsDialog && (
        <div className="video-loading-overlay absolute inset-0 flex items-center justify-center bg-black/50 backdrop-blur-sm">
          <div className="loading-message text-[var(--on-surface)]">{t.preparingVideoOverlay}</div>
        </div>
      )}
      <ExportDialog show={exportHook.showExportDialog} onClose={() => exportHook.setShowExportDialog(false)}
        onExport={exportHook.startExport} exportOptions={exportHook.exportOptions}
        setExportOptions={exportHook.setExportOptions} segment={segment}
        videoRef={videoRef} backgroundConfig={backgroundConfig} hasAudio={exportHook.hasAudio}
        sourceVideoFps={exportHook.sourceVideoFps}
        autoCopyEnabled={exportHook.exportAutoCopyEnabled}
        onToggleAutoCopy={exportHook.setExportAutoCopyEnabled} />
      <RawVideoDialog
        show={showRawVideoDialog}
        onClose={() => setShowRawVideoDialog(false)}
        savedPath={lastRawSavedPath}
        autoCopyEnabled={rawAutoCopyEnabled}
        isBusy={isRawActionBusy}
        onChangePath={(newPath: string) => setLastRawSavedPath(newPath)}
        onToggleAutoCopy={handleToggleRawAutoCopy}
      />
      <ExportSuccessDialog
        show={exportHook.showExportSuccessDialog}
        onClose={() => exportHook.setShowExportSuccessDialog(false)}
        filePath={exportHook.lastExportedPath}
        onFilePathChange={handleExportSuccessPathChange}
        autoCopyEnabled={exportHook.exportAutoCopyEnabled}
        onToggleAutoCopy={exportHook.setExportAutoCopyEnabled}
      />
      <HotkeyDialog show={showHotkeyDialog} onClose={closeHotkeyDialog} />
    </div>
    </SettingsContext.Provider>
  );
}

export default App;
