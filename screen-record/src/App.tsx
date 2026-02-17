import { useState, useEffect, useCallback, useRef, type CSSProperties } from "react";
import { Wand2, MousePointer2, Volume2, Keyboard } from "lucide-react";
import "./App.css";
import { Button } from "@/components/ui/button";
import { videoRenderer } from '@/lib/videoRenderer';
import { BackgroundConfig, MousePosition, VideoSegment, KeystrokeMode } from '@/types/video';
import { projectManager } from '@/lib/projectManager';
import { TimelineArea } from '@/components/timeline';
import { useUndoRedo } from '@/hooks/useUndoRedo';
import { useFfmpegSetup, useHotkeys, useMonitors } from '@/hooks/useAppHooks';
import {
  useVideoPlayback, useRecording, useProjects, useExport,
  useZoomKeyframes, useTextOverlays, useAutoZoom, useCursorHiding
} from '@/hooks/useVideoState';

import { Header } from '@/components/Header';
import { Placeholder, CropOverlay, PlaybackControls, CanvasResizeOverlay } from '@/components/VideoPreview';
import { SidePanel, ActivePanel } from '@/components/SidePanel';
import {
  ProcessingOverlay, ExportDialog,
  MonitorSelectDialog, HotkeyDialog, FfmpegSetupDialog
} from '@/components/Dialogs';
import { ProjectsView } from '@/components/ProjectsView';
import { SettingsContext, useSettingsProvider } from '@/hooks/useSettings';
import {
  ensureKeystrokeVisibilitySegments,
  filterKeystrokeEventsByMode,
  getKeystrokeVisibilitySegmentsForMode,
  rebuildKeystrokeVisibilitySegmentsForMode,
  withKeystrokeVisibilitySegmentsForMode
} from '@/lib/keystrokeVisibility';

const ipc = (msg: string) => (window as any).ipc.postMessage(msg);
const LAST_BG_CONFIG_KEY = 'screen-record-last-background-config-v1';
const RECENT_UPLOADS_KEY = 'screen-record-recent-uploads-v1';
const PROJECT_SAVE_DEBUG = true;
const sv = (v: number, min: number, max: number): CSSProperties =>
  ({ '--value-pct': `${((v - min) / (max - min)) * 100}%` } as CSSProperties);

const DEFAULT_BACKGROUND_CONFIG: BackgroundConfig = {
  scale: 90,
  borderRadius: 48,
  backgroundType: 'gradient2',
  shadow: 100,
  volume: 1,
  cursorScale: 5,
  cursorMovementDelay: 0.03,
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

function ResizeBorders() {
  const resize = (dir: string) => (e: React.MouseEvent) => { e.preventDefault(); ipc(`resize_${dir}`); };
  return (
    <>
      {/* Edges: left / right full-height, bottom full-width (top handled by Header) */}
      <div className="resize-border-left fixed top-0 left-0 bottom-0 w-[6px] z-50 cursor-ew-resize" onMouseDown={resize('w')} />
      <div className="resize-border-right fixed top-0 right-0 bottom-0 w-[6px] z-50 cursor-ew-resize" onMouseDown={resize('e')} />
      <div className="resize-border-bottom fixed bottom-0 left-[14px] right-[14px] h-[6px] z-50 cursor-ns-resize" onMouseDown={resize('s')} />
      {/* Corners */}
      <div className="resize-corner-sw fixed bottom-0 left-0 w-[14px] h-[14px] z-50 cursor-nesw-resize" onMouseDown={resize('sw')} />
      <div className="resize-corner-se fixed bottom-0 right-0 w-[14px] h-[14px] z-50 cursor-nwse-resize" onMouseDown={resize('se')} />
    </>
  );
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

  const timelineRef = useRef<HTMLDivElement>(null);
  const previewContainerRef = useRef<HTMLDivElement>(null);
  const mousePositionsRef = useRef<MousePosition[]>([]);
  const wheelBatchActiveRef = useRef(false);
  const wheelBatchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const restoreImageRef = useRef<string | null>(null);
  const projectSaveSeqRef = useRef(0);
  // Stable ref for persist callback — avoids cascading useEffect re-triggers
  const persistRef = useRef<typeof persistCurrentProjectNow>(null!);
  const debugProject = useCallback((event: string, data?: Record<string, unknown>) => {
    if (!PROJECT_SAVE_DEBUG) return;
    const ts = new Date().toISOString();
    console.log(`[ProjectSave][${ts}] ${event}`, data || {});
  }, []);

  // Utility hooks
  const { needsSetup, ffmpegInstallStatus, handleCancelInstall } = useFfmpegSetup();
  const { hotkeys, showHotkeyDialog, handleRemoveHotkey, openHotkeyDialog, closeHotkeyDialog } = useHotkeys();
  const { monitors, showMonitorSelect, setShowMonitorSelect, getMonitors } = useMonitors();

  // Video playback — mousePositionsRef is shared so useVideoPlayback always reads latest
  const playback = useVideoPlayback({ segment, backgroundConfig, mousePositionsRef, isCropping });
  const {
    currentTime, setCurrentTime, duration, isPlaying, isVideoReady, setIsVideoReady,
    thumbnails, setThumbnails, currentVideo, setCurrentVideo, currentAudio, setCurrentAudio,
    videoRef, audioRef, canvasRef, tempCanvasRef, videoControllerRef,
    renderFrame, togglePlayPause, seek, flushSeek, generateThumbnail, generateThumbnails
  } = playback;

  // Recording
  const recording = useRecording({
    videoControllerRef, videoRef, canvasRef, tempCanvasRef, backgroundConfig,
    setSegment, setCurrentVideo, setCurrentAudio, setIsVideoReady, setThumbnails,
    setDuration: () => {}, setCurrentTime, generateThumbnails, generateThumbnail,
    renderFrame, currentVideo, currentAudio
  });
  const {
    isRecording, recordingDuration, isLoadingVideo, loadingProgress,
    mousePositions, setMousePositions, audioFilePath, videoFilePath, videoFilePathOwnerUrl, error, setError,
    startNewRecording, handleStopRecording
  } = recording;

  // Sync mouse positions to shared ref synchronously during render (NOT in
  // an effect) so useVideoPlayback's effects always read the latest positions.
  // Previously this was a useEffect which ran AFTER useVideoPlayback's effects,
  // causing stale/empty mouse positions on project load → frozen cursor.
  mousePositionsRef.current = mousePositions;

  // Projects
  const projects = useProjects({
    videoControllerRef, setCurrentVideo, setCurrentAudio, setSegment,
    setBackgroundConfig, setMousePositions, setThumbnails, currentVideo, currentAudio
  });

  // Export
  const exportHook = useExport({
    videoRef, canvasRef, tempCanvasRef, audioRef, segment, backgroundConfig,
    mousePositions, audioFilePath, videoFilePath, videoFilePathOwnerUrl, currentVideo
  });

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
    segment, setSegment, mousePositions, currentTime, duration
  });
  const { editingPointerId, setEditingPointerId, handleSmartPointerHiding,
    handleAddPointerSegment, handleDeletePointerSegment } = cursorHiding;
  const isOverlayMode = projects.showProjectsDialog || isCropping;

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

    const handleMouseMove = (me: MouseEvent) => {
      const dx = me.clientX - startX, dy = me.clientY - startY;
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
      commitBatch();
    };
    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', handleMouseUp);
  }, [currentVideo, isCropping, activePanel, isPlaying, togglePlayPause, handleAddKeyframe, beginBatch, commitBatch]);

  const handleBackgroundUpload = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) {
      const reader = new FileReader();
      reader.onload = (event) => {
        const imageUrl = event.target?.result as string;
        setBackgroundConfig(prev => ({ ...prev, backgroundType: 'custom', customBackground: imageUrl }));
        setRecentUploads(prev => [imageUrl, ...prev.filter(v => v !== imageUrl)].slice(0, 12));
      };
      reader.readAsDataURL(file);
    }
  }, []);

  const handleRemoveRecentUpload = useCallback((imageUrl: string) => {
    setRecentUploads(prev => prev.filter(v => v !== imageUrl));
    setBackgroundConfig(prev => {
      if (prev.backgroundType === 'custom' && prev.customBackground === imageUrl) {
        return { ...prev, backgroundType: 'gradient2', customBackground: undefined };
      }
      return prev;
    });
  }, []);

  const handleOpenCursorLab = useCallback(() => {
    window.location.hash = '#cursor-lab';
  }, []);

  const getKeystrokeTimelineDuration = useCallback((s: VideoSegment) => {
    return Math.max(
      s.trimEnd,
      ...(s.trimSegments || []).map((trimSegment) => trimSegment.endTime),
      duration
    );
  }, [duration]);

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
      const modeEvents = filterKeystrokeEventsByMode(prepared.keystrokeEvents ?? [], next);
      const modeSegments = next === 'keyboard'
        ? (prepared.keyboardVisibilitySegments ?? [])
        : (prepared.keyboardMouseVisibilitySegments ?? []);

      if (modeSegments.length === 0 && modeEvents.length > 0) {
        prepared = rebuildKeystrokeVisibilitySegmentsForMode(prepared, next, timelineDuration);
      }
    }

    setSegment({
      ...prepared,
      keystrokeMode: next,
      keystrokeEvents: prepared.keystrokeEvents ?? [],
    });
    setEditingKeystrokeSegmentId(null);
  }, [segment, setSegment, getKeystrokeTimelineDuration]);

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
        duration: videoControllerRef.current?.duration || duration
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
    generateThumbnail, duration, debugProject
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
    await projects.handleLoadProject(projectId);
    debugProject('grid-load:done', { targetProjectId: projectId });
  }, [persistCurrentProjectNow, projects, debugProject]);

  const handleToggleProjects = useCallback(async () => {
    if (projects.showProjectsDialog) {
      debugProject('projects-toggle:close');
      window.dispatchEvent(new CustomEvent('sr-close-projects'));
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
  }, [projects.showProjectsDialog, projects.currentProjectId, currentVideo, backgroundConfig.canvasMode, backgroundConfig.canvasWidth, backgroundConfig.canvasHeight, persistCurrentProjectNow, debugProject, projects]);

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

  const handleStartRecording = useCallback(async () => {
    if (isRecording) return;
    try {
      const monitorList = await getMonitors();
      if (monitorList.length > 1) setShowMonitorSelect(true);
      else await startNewRecording('0');
    } catch (err) { setError(err as string); }
  }, [isRecording, getMonitors, setShowMonitorSelect, startNewRecording, setError]);

  const onStopRecording = useCallback(async () => {
    const result = await handleStopRecording();
    if (result) {
      const { mouseData, initialSegment, videoUrl } = result;
      const response = await fetch(videoUrl);
      const videoBlob = await response.blob();
      const thumbnail = await videoControllerRef.current?.generateThumbnail({
        segment: initialSegment, backgroundConfig, mousePositions: mouseData
      }) || generateThumbnail();
      const project = await projectManager.saveProject({
        name: `Recording ${new Date().toLocaleString()}`,
        videoBlob, segment: initialSegment, backgroundConfig, mousePositions: mouseData, thumbnail,
        duration: initialSegment.trimEnd
      });
      projects.setCurrentProjectId(project.id);
      await projects.loadProjects();
    }
  }, [handleStopRecording, backgroundConfig, generateThumbnail, projects]);

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
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement).tagName;
      const isInput = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || (e.target as HTMLElement).isContentEditable;
      if (e.code === 'Space' && !isInput) {
        e.preventDefault();
        e.stopImmediatePropagation();
        if (isCropping) return; // Block play/pause during crop mode
        // Blur focused buttons so keyup doesn't re-trigger their click
        if (tag === 'BUTTON' || tag === 'A') (e.target as HTMLElement).blur();
        togglePlayPause();
      }
      if ((e.code === 'Delete' || e.code === 'Backspace') && !isInput) {
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
  }, [editingKeyframeId, editingTextId, editingPointerId, editingKeystrokeSegmentId, handleDeleteText, handleDeletePointerSegment, handleDeleteKeystrokeSegment, segment, canUndo, canRedo, undo, redo, setSegment, setEditingKeyframeId, togglePlayPause, isCropping]);

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
          keystrokeMode: 'off',
          keystrokeEvents: [],
          keyboardVisibilitySegments: [],
          keyboardMouseVisibilitySegments: [],
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

  // Text drag listeners
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !segment) return;
    const onDown = (e: MouseEvent) => {
      const hitId = videoRenderer.handleMouseDown(e, segment, canvas);
      if (hitId) {
        e.stopPropagation();
        e.preventDefault();
        setEditingTextId(hitId);
        setActivePanel('text');
      }
    };
    const onMove = (e: MouseEvent) => videoRenderer.handleMouseMove(e, segment, canvas, handleTextDragMove);
    const onUp = () => videoRenderer.handleMouseUp();
    canvas.addEventListener('mousedown', onDown);
    canvas.addEventListener('mousemove', onMove);
    canvas.addEventListener('mouseup', onUp);
    canvas.addEventListener('mouseleave', onUp);
    return () => {
      canvas.removeEventListener('mousedown', onDown);
      canvas.removeEventListener('mousemove', onMove);
      canvas.removeEventListener('mouseup', onUp);
      canvas.removeEventListener('mouseleave', onUp);
    };
  }, [segment, handleTextDragMove, canvasRef, setEditingTextId, setActivePanel]);

  return (
    <SettingsContext.Provider value={settings}>
    <div className="app-container min-h-screen bg-[var(--surface)]">
      <ResizeBorders />
      <Header
        isRecording={isRecording} recordingDuration={recordingDuration} currentVideo={currentVideo}
        isProcessing={exportHook.isProcessing} hotkeys={hotkeys}
        onRemoveHotkey={handleRemoveHotkey} onOpenHotkeyDialog={openHotkeyDialog}
        onExport={exportHook.handleExport}
        onOpenCursorLab={handleOpenCursorLab}
        onOpenProjects={handleToggleProjects}
        hideExport={isOverlayMode}
      />

      <main className="app-main flex flex-col px-3 py-3 overflow-hidden" style={{ height: 'calc(100vh - 44px)' }}>
        {error && <p className="error-message text-[var(--tertiary-color)] mb-2 flex-shrink-0">{error}</p>}

        <div className="content-layout flex gap-3 flex-1 min-h-0">
          {/* Video Preview */}
          <div className="video-preview-container flex-1 min-w-0 rounded-xl overflow-hidden bg-[var(--surface-dim)]/80 backdrop-blur-2xl flex items-center justify-center shadow-[0_8px_32px_rgba(0,0,0,0.3)]">
            <div className="preview-inner relative w-full h-full flex justify-center items-center">
              <div
                ref={previewContainerRef}
                className="preview-canvas relative flex items-center justify-center cursor-crosshair group w-full h-full"
                onMouseDown={handlePreviewMouseDown}
              >
                <canvas ref={canvasRef} className="preview-canvas-element absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 max-w-full max-h-full" />
                <canvas ref={tempCanvasRef} className="hidden" />
                <video ref={videoRef} className="hidden" playsInline preload="auto" />
                <audio ref={audioRef} className="hidden" />

                {(!currentVideo || isLoadingVideo) && (
                  <Placeholder isLoadingVideo={isLoadingVideo} loadingProgress={loadingProgress}
                    isRecording={isRecording} recordingDuration={recordingDuration} />
                )}

                {isCropping && currentVideo && segment && (
                  <CropOverlay segment={segment}
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
              </div>

              {/* Projects view — lives inside the preview area for native FLIP animation */}
              {projects.showProjectsDialog && (
                <ProjectsView
                  projects={projects.projects}
                  onLoadProject={handleLoadProjectFromGrid}
                  onProjectsChange={projects.loadProjects}
                  onClose={() => projects.setShowProjectsDialog(false)}
                  currentProjectId={projects.currentProjectId}
                  restoreImage={restoreImageRef.current}
                />
              )}
            </div>
          </div>

          {/* Side Panel */}
          <div className={`side-panel-container w-80 flex-shrink-0 min-h-0 relative ${isOverlayMode ? 'overflow-hidden' : 'overflow-y-auto thin-scrollbar'}`}>
            <SidePanel
              activePanel={activePanel} setActivePanel={setActivePanel} segment={segment}
              editingKeyframeId={editingKeyframeId} zoomFactor={zoomFactor} setZoomFactor={setZoomFactor}
              onDeleteKeyframe={handleDeleteKeyframe} onUpdateZoom={throttledUpdateZoom}
              backgroundConfig={backgroundConfig} setBackgroundConfig={setBackgroundConfig}
              recentUploads={recentUploads} onRemoveRecentUpload={handleRemoveRecentUpload}
              onBackgroundUpload={handleBackgroundUpload}
              editingTextId={editingTextId} onUpdateSegment={setSegment}
              beginBatch={beginBatch} commitBatch={commitBatch}
            />
            {isOverlayMode && <div className="panel-block-overlay absolute inset-0 bg-[var(--surface)] z-50" />}
          </div>
        </div>

        {currentVideo && !isLoadingVideo && !projects.showProjectsDialog && (
          <div className="playback-controls-row mt-2 flex-shrink-0 flex justify-center">
            <PlaybackControls isPlaying={isPlaying} isProcessing={exportHook.isProcessing}
              isVideoReady={isVideoReady} isCropping={isCropping} currentTime={currentTime}
              duration={duration} onTogglePlayPause={togglePlayPause} onToggleCrop={handleToggleCrop}
              canvasModeToggle={
                <div className="playback-canvas-mode-toggle flex rounded-lg border border-[var(--overlay-divider)] overflow-hidden">
                  {(['auto', 'custom'] as const).map((mode) => {
                    const isActive = (backgroundConfig.canvasMode ?? 'auto') === mode;
                    return (
                      <button
                        key={mode}
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
                        className={`playback-canvas-mode-btn playback-canvas-mode-btn-${mode} px-2 py-1 text-[10px] font-medium transition-colors ${
                          isActive
                            ? 'bg-[var(--primary-color)] text-white shadow-[inset_0_0_0_1px_rgba(255,255,255,0.24)]'
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
              }
              autoZoomButton={
                <Button onClick={handleAutoZoom}
                  disabled={exportHook.isProcessing || !currentVideo || !mousePositions.length}
                  className={`flex items-center px-2.5 py-1 h-7 text-xs font-medium transition-colors whitespace-nowrap rounded-lg ${
                    !currentVideo || exportHook.isProcessing || !mousePositions.length
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
                  disabled={exportHook.isProcessing || !currentVideo || !mousePositions.length}
                  className={`flex items-center px-2.5 py-1 h-7 text-xs font-medium transition-colors whitespace-nowrap rounded-lg ${
                    !currentVideo || exportHook.isProcessing || !mousePositions.length
                      ? 'bg-[var(--surface-container)]/50 text-[var(--on-surface)]/35 cursor-not-allowed'
                      : (() => {
                          const segs = segment?.cursorVisibilitySegments;
                          const isDefault = !segs || segs.length === 0 || (
                            segs.length === 1 &&
                            Math.abs(segs[0].startTime - 0) < 0.01 &&
                            Math.abs(segs[0].endTime - duration) < 0.01
                          );
                          return isDefault
                            ? 'bg-[var(--glass-bg)] hover:bg-[var(--glass-bg-hover)] text-[var(--on-surface)]'
                            : 'bg-[var(--success-color)] hover:bg-[var(--success-color)]/85 text-white';
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
          </div>
        )}

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

      {/* Dialogs */}
      <ProcessingOverlay show={exportHook.isProcessing} exportProgress={0} onCancel={exportHook.cancelExport} />
      <MonitorSelectDialog show={showMonitorSelect} onClose={() => setShowMonitorSelect(false)}
        monitors={monitors} onSelectMonitor={startNewRecording} />
      {currentVideo && !isVideoReady && !projects.showProjectsDialog && (
        <div className="video-loading-overlay absolute inset-0 flex items-center justify-center bg-black/50 backdrop-blur-sm">
          <div className="loading-message text-[var(--on-surface)]">{t.preparingVideoOverlay}</div>
        </div>
      )}
      <ExportDialog show={exportHook.showExportDialog} onClose={() => exportHook.setShowExportDialog(false)}
        onExport={exportHook.startExport} exportOptions={exportHook.exportOptions}
        setExportOptions={exportHook.setExportOptions} segment={segment}
        videoRef={videoRef} backgroundConfig={backgroundConfig} />
      <HotkeyDialog show={showHotkeyDialog} onClose={closeHotkeyDialog} />
      <FfmpegSetupDialog show={needsSetup} ffmpegInstallStatus={ffmpegInstallStatus}
        onCancelInstall={handleCancelInstall} />
    </div>
    </SettingsContext.Provider>
  );
}

export default App;
