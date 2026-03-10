import {
  useState,
  useEffect,
  useCallback,
  useRef,
  useMemo,
  useLayoutEffect,
  type CSSProperties,
} from "react";
import { videoTimeToWallClock } from "@/lib/exportEstimator";
import { Wand2, MousePointer2, Volume2, Keyboard } from "lucide-react";
import { invoke } from "@/lib/ipc";
import "./App.css";
import { Button } from "@/components/ui/button";
import {
  videoRenderer,
  type KeystrokeOverlayEditBounds,
} from "@/lib/videoRenderer";
import {
  BackgroundConfig,
  MousePosition,
  Project,
  ProjectComposition,
  ProjectCompositionClip,
  ProjectCompositionMode,
  VideoSegment,
  KeystrokeMode,
  RecordingMode,
} from "@/types/video";
import { projectManager } from "@/lib/projectManager";
import { TimelineArea } from "@/components/timeline";
import { useUndoRedo } from "@/hooks/useUndoRedo";
import { useHotkeys, useMonitors, useWindows } from "@/hooks/useAppHooks";
import {
  useVideoPlayback,
  useRecording,
  useProjects,
  useExport,
  useZoomKeyframes,
  useTextOverlays,
  useAutoZoom,
  useCursorHiding,
  getSavedKeystrokeLanguage,
  saveKeystrokeLanguage,
  getSavedCropPref,
  saveCropPref,
} from "@/hooks/useVideoState";
import { DEFAULT_BUILT_IN_BACKGROUND_ID } from "@/lib/backgroundPresets";

import { Header } from "@/components/Header";
import {
  Placeholder,
  PlaybackControls,
  CanvasResizeOverlay,
  SeekIndicator,
} from "@/components/VideoPreview";
import { CropWorkspace } from "@/components/CropWorkspace";
import { SequencePillChain } from "@/components/SequencePillChain";
import { SidePanel, type ActivePanel } from "@/components/sidepanel/index";
import {
  ProcessingOverlay,
  ExportDialog,
  WindowSelectDialog,
  HotkeyDialog,
  RawVideoDialog,
  ExportSuccessDialog,
} from "@/components/dialogs";
import { ProjectsView } from "@/components/ProjectsView";
import { SettingsContext, useSettingsProvider } from "@/hooks/useSettings";
import {
  ensureKeystrokeVisibilitySegments,
  getKeystrokeVisibilitySegmentsForMode,
  rebuildKeystrokeVisibilitySegmentsForMode,
  withKeystrokeVisibilitySegmentsForMode,
} from "@/lib/keystrokeVisibility";
import { ResizeBorders } from "@/components/layout/ResizeBorders";
import { useAppShortcuts } from "@/hooks/useAppShortcuts";
import { useRawVideoHandler } from "@/hooks/useRawVideoHandler";
import { useKeystrokeDrag } from "@/hooks/useKeystrokeDrag";
import {
  applyCanvasConfig,
  createCompositionSnapshotClip,
  ensureProjectComposition,
  extractCanvasConfig,
  getCompositionAutoSourceClipId,
  getCompositionAdjacentClipIds,
  getCompositionClip,
  getCompositionResolvedBackgroundConfig,
  insertCompositionClip,
  normalizeCompositionClipToCanvas,
  removeCompositionClip,
  setCompositionMode,
  syncCompositionCanvasConfig,
  updateCompositionClip,
  withCompositionSelection,
} from "@/lib/projectComposition";
import {
  getMediaServerUrl,
  isManagedCompositionSnapshotPath,
  writeBlobToTempMediaFile,
} from "@/lib/mediaServer";

const LAST_BG_CONFIG_KEY = "screen-record-last-background-config-v1";
const RECENT_UPLOADS_KEY = "screen-record-recent-uploads-v1";
const RECORDING_MODE_KEY = "screen-record-recording-mode-v1";
const CAPTURE_SOURCE_KEY = "screen-record-capture-source-v1";
const KEYSTROKE_DELAY_KEY = "screen-record-keystroke-delay-v1";
const KEYSTROKE_MODE_PREF_KEY = "screen-record-keystroke-mode-pref-v1";
const KEYSTROKE_OVERLAY_PREF_KEY = "screen-record-keystroke-overlay-pref-v1";
const PROJECT_SAVE_DEBUG = false;
const PLAYBACK_RESET_DEBUG = false;
const DEFAULT_KEYSTROKE_DELAY_SEC = 0;
const sv = (v: number, min: number, max: number): CSSProperties =>
  ({ "--value-pct": `${((v - min) / (max - min)) * 100}%` }) as CSSProperties;

type PreloadSlotKey = "previous" | "next";

interface ClipMediaAssets {
  videoBlob: Blob | null;
  audioBlob: Blob | null;
  customBackground: string | null;
}

const DEFAULT_BACKGROUND_CONFIG: BackgroundConfig = {
  scale: 90,
  borderRadius: 32,
  backgroundType: DEFAULT_BUILT_IN_BACKGROUND_ID,
  shadow: 100,
  volume: 1,
  cursorScale: 5,
  cursorMovementDelay: 0,
  cursorShadow: 100,
  cursorWiggleStrength: 0.3,
  cursorTiltAngle: -10,
  motionBlurCursor: 25,
  motionBlurZoom: 10,
  motionBlurPan: 10,
  cursorPack: "macos26",
  cursorDefaultVariant: "macos26",
  cursorTextVariant: "macos26",
  cursorPointerVariant: "macos26",
  cursorOpenHandVariant: "macos26",
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
    return parsed
      .filter((v): v is string => typeof v === "string" && v.length > 0)
      .slice(0, 12);
  } catch {
    return [];
  }
}

function getInitialRecordingMode(): RecordingMode {
  try {
    const raw = localStorage.getItem(RECORDING_MODE_KEY);
    if (raw === "withCursor" || raw === "withoutCursor") return raw;
  } catch {
    // ignore
  }
  return "withoutCursor";
}

function getInitialCaptureSource(): "monitor" | "window" {
  try {
    const raw = localStorage.getItem(CAPTURE_SOURCE_KEY);
    if (raw === "monitor" || raw === "window") return raw;
  } catch {
    // ignore
  }
  return "monitor";
}

function getSavedKeystrokeModePref(): KeystrokeMode {
  try {
    const raw = localStorage.getItem(KEYSTROKE_MODE_PREF_KEY);
    if (raw === "keyboard" || raw === "keyboardMouse" || raw === "off")
      return raw;
  } catch {
    // ignore
  }
  return "off";
}

function getSavedKeystrokeOverlayPref(): {
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

function App() {
  const settings = useSettingsProvider();
  const { t } = settings;
  // Core state
  const {
    state: segment,
    setState: setSegment,
    undo,
    redo,
    canUndo,
    canRedo,
    beginBatch,
    commitBatch,
  } = useUndoRedo<VideoSegment | null>(null);
  const [activePanel, setActivePanel] = useState<ActivePanel>("background");
  const [isCropping, setIsCropping] = useState(false);
  const [recentUploads, setRecentUploads] = useState<string[]>(
    getInitialRecentUploads,
  );
  const [backgroundConfig, setBackgroundConfig] = useState<BackgroundConfig>(
    getInitialBackgroundConfig,
  );
  const [selectedRecordingMode, setSelectedRecordingMode] =
    useState<RecordingMode>(getInitialRecordingMode);
  const [captureSource, setCaptureSource] = useState<"monitor" | "window">(
    getInitialCaptureSource,
  );
  const [captureTargetId, setCaptureTargetId] = useState<string>("0");
  const [captureFps, setCaptureFps] = useState<number | null>(() => {
    try {
      const saved = localStorage.getItem("screen-record-capture-fps-v1");
      return saved ? parseInt(saved, 10) : null;
    } catch {
      return null;
    }
  });
  const captureFpsRef = useRef<number | null>(captureFps);
  const [currentRecordingMode, setCurrentRecordingMode] =
    useState<RecordingMode>("withoutCursor");
  const [currentProjectData, setCurrentProjectData] = useState<Project | null>(
    null,
  );
  const [composition, setComposition] = useState<ProjectComposition | null>(
    null,
  );
  const [projectPickerMode, setProjectPickerMode] = useState<
    "insertBefore" | "insertAfter" | null
  >(null);
  const [sequenceTargetClipId, setSequenceTargetClipId] = useState<
    string | null
  >(null);
  const rawVideo = useRawVideoHandler();
  const {
    currentRawVideoPath,
    setCurrentRawVideoPath,
    lastRawSavedPath,
    setLastRawSavedPath,
    showRawVideoDialog,
    setShowRawVideoDialog,
    rawAutoCopyEnabled,
    rawSaveDir,
    isRawActionBusy,
    setIsRawActionBusy,
    rawButtonSavedFlash,
    setRawButtonSavedFlash,
    flashRawSavedButton,
    handleOpenRawVideoDialog,
    handleToggleRawAutoCopy,
  } = rawVideo;
  const [isBackgroundUploadProcessing, setIsBackgroundUploadProcessing] =
    useState(false);

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
  const [isKeystrokeOverlaySelected, setIsKeystrokeOverlaySelected] =
    useState(false);
  const [isPreviewDragging, setIsPreviewDragging] = useState(false);
  const [isKeystrokeResizeHandleHover, setIsKeystrokeResizeHandleHover] =
    useState(false);
  const [isKeystrokeResizeDragging, setIsKeystrokeResizeDragging] =
    useState(false);
  const [seekIndicatorKey, setSeekIndicatorKey] = useState(0);
  const [seekIndicatorDir, setSeekIndicatorDir] = useState<"left" | "right">(
    "right",
  );
  const [isCanvasResizeDragging, setIsCanvasResizeDragging] = useState(false);
  const [spreadFromClipId, setSpreadFromClipId] = useState<string | null>(null);
  const pendingWindowRecordingRef = useRef(false);
  const isSwitchingCompositionClipRef = useRef(false);
  const spreadAnimationTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
    null,
  );
  const previousPreloadVideoRef = useRef<HTMLVideoElement | null>(null);
  const previousPreloadAudioRef = useRef<HTMLAudioElement | null>(null);
  const nextPreloadVideoRef = useRef<HTMLVideoElement | null>(null);
  const nextPreloadAudioRef = useRef<HTMLAudioElement | null>(null);
  const clipAssetCacheRef = useRef<Map<string, ClipMediaAssets>>(new Map());
  const clipUrlCacheRef = useRef<
    Map<string, { videoUrl: string; audioUrl: string | null }>
  >(new Map());
  const clipExportSourcePathCacheRef = useRef<Map<string, string>>(new Map());
  const preloadedSlotClipIdsRef = useRef<Record<PreloadSlotKey, string | null>>(
    {
      previous: null,
      next: null,
    },
  );
  const clipLoadRequestSeqRef = useRef(0);
  const [loadedClipId, setLoadedClipId] = useState<string | null>(null);
  const playbackResetPrevTimeRef = useRef(0);
  const playbackResetLastSignatureRef = useRef<string | null>(null);
  const playbackResetLastAtRef = useRef(0);
  // Stable ref for persist callback — avoids cascading useEffect re-triggers
  const persistRef = useRef<typeof persistCurrentProjectNow>(null!);
  const debugProject = useCallback(
    (event: string, data?: Record<string, unknown>) => {
      if (!PROJECT_SAVE_DEBUG) return;
      const ts = new Date().toISOString();
      console.log(`[ProjectSave][${ts}] ${event}`, data || {});
    },
    [],
  );

  // Utility hooks
  const {
    hotkeys,
    showHotkeyDialog,
    handleRemoveHotkey,
    openHotkeyDialog,
    closeHotkeyDialog,
  } = useHotkeys();
  const { monitors, getMonitors } = useMonitors();
  const { windows, showWindowSelect, setShowWindowSelect, getWindows } =
    useWindows();

  // Video playback — mousePositionsRef is shared so useVideoPlayback always reads latest
  const playback = useVideoPlayback({
    segment,
    backgroundConfig,
    mousePositionsRef,
    isCropping,
    interactiveBackgroundPreview: isCanvasResizeDragging,
  });
  const {
    currentTime: previewCurrentTime,
    setCurrentTime: setPreviewCurrentTime,
    duration: previewDuration,
    setDuration: setPreviewDuration,
    isPlaying: isPreviewPlaying,
    isVideoReady,
    setIsVideoReady,
    thumbnails,
    setThumbnails,
    currentVideo,
    setCurrentVideo,
    currentAudio,
    setCurrentAudio,
    videoRef,
    audioRef,
    canvasRef,
    tempCanvasRef,
    videoControllerRef,
    renderFrame,
    togglePlayPause: togglePlayback,
    seek,
    flushSeek,
    generateThumbnail,
    generateThumbnailsForSource,
    invalidateThumbnails,
  } = playback;

  // Recording
  const recording = useRecording({
    videoControllerRef,
    videoRef,
    canvasRef,
    tempCanvasRef,
    backgroundConfig,
    setSegment,
    setCurrentVideo,
    setCurrentAudio,
    setIsVideoReady,
    setThumbnails,
    invalidateThumbnails,
    setDuration: setPreviewDuration,
    setCurrentTime: setPreviewCurrentTime,
    generateThumbnailsForSource,
    generateThumbnail,
    renderFrame,
    currentVideo,
    currentAudio,
  });
  const {
    isRecording,
    recordingDuration,
    isLoadingVideo,
    loadingProgress,
    mousePositions,
    setMousePositions,
    audioFilePath,
    videoFilePath,
    videoFilePathOwnerUrl,
    error,
    setError,
    startNewRecording,
    handleStopRecording,
  } = recording;

  // Sync mouse positions to ref before paint so useVideoPlayback always reads
  // the latest positions without causing stale-closure bugs in Concurrent Mode.
  useLayoutEffect(() => {
    mousePositionsRef.current = mousePositions;
  }, [mousePositions]);

  // Projects
  const handleProjectRawVideoPathChange = useCallback((path: string) => {
    setCurrentRawVideoPath(path);
    setLastRawSavedPath("");
  }, []);
  const getPreloadRefs = useCallback((slot: PreloadSlotKey) => {
    return slot === "previous"
      ? {
          videoRef: previousPreloadVideoRef,
          audioRef: previousPreloadAudioRef,
        }
      : {
          videoRef: nextPreloadVideoRef,
          audioRef: nextPreloadAudioRef,
        };
  }, []);
  const clearClipMediaCaches = useCallback(
    (options?: {
      preserveVideoUrl?: string | null;
      preserveAudioUrl?: string | null;
    }) => {
      const preservedVideoUrl = options?.preserveVideoUrl ?? null;
      const preservedAudioUrl = options?.preserveAudioUrl ?? null;

      for (const { videoUrl, audioUrl } of clipUrlCacheRef.current.values()) {
        if (videoUrl?.startsWith("blob:") && videoUrl !== preservedVideoUrl) {
          URL.revokeObjectURL(videoUrl);
        }
        if (audioUrl?.startsWith("blob:") && audioUrl !== preservedAudioUrl) {
          URL.revokeObjectURL(audioUrl);
        }
      }
      clipAssetCacheRef.current.clear();
      clipUrlCacheRef.current.clear();
      (["previous", "next"] as const).forEach((slot) => {
        preloadedSlotClipIdsRef.current[slot] = null;
        const { videoRef: preloadVideoRef, audioRef: preloadAudioRef } =
          getPreloadRefs(slot);
        if (preloadVideoRef.current) {
          preloadVideoRef.current.pause();
          preloadVideoRef.current.removeAttribute("src");
          preloadVideoRef.current.load();
        }
        if (preloadAudioRef.current) {
          preloadAudioRef.current.pause();
          preloadAudioRef.current.removeAttribute("src");
          preloadAudioRef.current.load();
        }
      });
    },
    [getPreloadRefs],
  );
  const loadClipAssets = useCallback(
    async (
      projectId: string,
      clipId: string,
      projectOverride?: Project | null,
      compositionOverride?: ProjectComposition | null,
    ): Promise<ClipMediaAssets | null> => {
      const project = projectOverride ?? currentProjectData;
      const nextComposition = compositionOverride ?? composition;
      if (!project || !nextComposition) return null;
      const cachedAssets = clipAssetCacheRef.current.get(clipId);
      if (cachedAssets) return cachedAssets;
      const clip = getCompositionClip(nextComposition, clipId);
      if (!clip) return null;
      const loadedAssets =
        clip.role === "root"
          ? {
              videoBlob: project.videoBlob ?? null,
              audioBlob: project.audioBlob ?? null,
              customBackground:
                project.backgroundConfig.customBackground ?? null,
            }
          : await projectManager.loadCompositionClipAssets(projectId, clip.id);
      clipAssetCacheRef.current.set(clipId, loadedAssets);
      return loadedAssets;
    },
    [composition, currentProjectData],
  );
  const getClipMediaUrls = useCallback(
    async (
      projectId: string,
      clipId: string,
      projectOverride?: Project | null,
      compositionOverride?: ProjectComposition | null,
    ) => {
      const nextComposition = compositionOverride ?? composition;
      const clip = getCompositionClip(nextComposition, clipId);
      if (!clip) return null;
      const cachedUrls = clipUrlCacheRef.current.get(clipId);
      if (cachedUrls) return cachedUrls;
      if (clip.rawVideoPath) {
        const videoUrl = await getMediaServerUrl(clip.rawVideoPath);
        const loadedAssets = await loadClipAssets(
          projectId,
          clipId,
          projectOverride,
          compositionOverride,
        );
        const nextUrls = {
          videoUrl,
          audioUrl: loadedAssets?.audioBlob ? videoUrl : null,
        };
        clipUrlCacheRef.current.set(clipId, nextUrls);
        return nextUrls;
      }
      const loadedAssets = await loadClipAssets(
        projectId,
        clipId,
        projectOverride,
        compositionOverride,
      );
      if (!loadedAssets?.videoBlob) return null;
      const nextUrls = {
        videoUrl: URL.createObjectURL(loadedAssets.videoBlob),
        audioUrl: loadedAssets.audioBlob
          ? URL.createObjectURL(loadedAssets.audioBlob)
          : null,
      };
      clipUrlCacheRef.current.set(clipId, nextUrls);
      return nextUrls;
    },
    [composition, loadClipAssets],
  );
  const drawPreloadedClipFrame = useCallback(
    (
      clipId: string,
      nextBackground: BackgroundConfig,
      nextComposition: ProjectComposition,
    ) => {
      const slot = (["previous", "next"] as const).find(
        (slotKey) => preloadedSlotClipIdsRef.current[slotKey] === clipId,
      );
      if (!slot || !canvasRef.current) return;
      const clip = getCompositionClip(nextComposition, clipId);
      const preloadVideoRef = getPreloadRefs(slot).videoRef;
      const preloadedVideo = preloadVideoRef.current;
      if (!clip || !preloadedVideo || preloadedVideo.readyState < 2) return;
      videoRenderer.drawFrame({
        video: preloadedVideo,
        canvas: canvasRef.current,
        tempCanvas: tempCanvasRef.current,
        segment: clip.segment,
        backgroundConfig: nextBackground,
        mousePositions: clip.mousePositions,
        currentTime: preloadedVideo.currentTime || clip.segment.trimStart,
      });
    },
    [canvasRef, getPreloadRefs, tempCanvasRef],
  );
  const primePreloadSlot = useCallback(
    async (
      slot: PreloadSlotKey,
      clipId: string | null,
      projectOverride?: Project | null,
      compositionOverride?: ProjectComposition | null,
    ) => {
      const project = projectOverride ?? currentProjectData;
      const nextComposition = compositionOverride ?? composition;
      const { videoRef: preloadVideoRef, audioRef: preloadAudioRef } =
        getPreloadRefs(slot);
      const preloadVideo = preloadVideoRef.current;
      if (!project || !nextComposition || !preloadVideo) return;
      if (!clipId) {
        preloadedSlotClipIdsRef.current[slot] = null;
        preloadVideo.pause();
        preloadVideo.removeAttribute("src");
        preloadVideo.load();
        if (preloadAudioRef.current) {
          preloadAudioRef.current.pause();
          preloadAudioRef.current.removeAttribute("src");
          preloadAudioRef.current.load();
        }
        return;
      }
      const clip = getCompositionClip(nextComposition, clipId);
      if (!clip) return;
      const clipUrls = await getClipMediaUrls(
        project.id,
        clipId,
        project,
        nextComposition,
      );
      if (!clipUrls) return;
      preloadedSlotClipIdsRef.current[slot] = clipId;
      if (preloadVideo.src !== clipUrls.videoUrl) {
        preloadVideo.src = clipUrls.videoUrl;
        preloadVideo.preload = "auto";
        preloadVideo.load();
      }
      if (preloadVideo.readyState < 2) {
        await new Promise<void>((resolve) =>
          preloadVideo.addEventListener("loadeddata", () => resolve(), {
            once: true,
          }),
        );
      }
      const startTime = clip.segment.trimStart;
      if (Math.abs(preloadVideo.currentTime - startTime) > 0.02) {
        await new Promise<void>((resolve) => {
          const onSeeked = () => resolve();
          preloadVideo.addEventListener("seeked", onSeeked, { once: true });
          preloadVideo.currentTime = startTime;
        });
      }
      if (!preloadAudioRef.current) return;
      if (!clipUrls.audioUrl) {
        preloadAudioRef.current.removeAttribute("src");
        preloadAudioRef.current.load();
        return;
      }
      if (preloadAudioRef.current.src !== clipUrls.audioUrl) {
        preloadAudioRef.current.src = clipUrls.audioUrl;
        preloadAudioRef.current.preload = "auto";
        preloadAudioRef.current.load();
      }
    },
    [composition, currentProjectData, getClipMediaUrls, getPreloadRefs],
  );
  const loadClipMediaIntoEditor = useCallback(
    async (
      projectId: string,
      clipId: string,
      projectOverride?: Project | null,
      compositionOverride?: ProjectComposition | null,
      options?: {
        preferPreloadedFrame?: boolean;
        requestId?: number;
        deferThumbnailsMs?: number;
      },
    ) => {
      const project = projectOverride ?? currentProjectData;
      const nextComposition = compositionOverride ?? composition;
      if (!project || !nextComposition) return;
      const clip = getCompositionClip(nextComposition, clipId);
      if (!clip) return;
      const clipPreviewDuration = Math.max(clip.duration || 0, clip.segment.trimEnd);
      const clipThumbnailPlaceholder = clip.thumbnail
        ? Array.from({ length: 6 }, () => clip.thumbnail!)
        : null;
      const requestId = options?.requestId ?? clipLoadRequestSeqRef.current + 1;
      clipLoadRequestSeqRef.current = requestId;
      const isLatestRequest = () => clipLoadRequestSeqRef.current === requestId;
      isSwitchingCompositionClipRef.current = true;
      try {
        const cachedVideoUrls = new Set(
          Array.from(clipUrlCacheRef.current.values()).map(
            (urls) => urls.videoUrl,
          ),
        );
        const cachedAudioUrls = new Set(
          Array.from(clipUrlCacheRef.current.values())
            .map((urls) => urls.audioUrl)
            .filter((url): url is string => Boolean(url)),
        );
        if (
          currentVideo?.startsWith("blob:") &&
          !cachedVideoUrls.has(currentVideo)
        ) {
          URL.revokeObjectURL(currentVideo);
        }
        if (
          currentAudio?.startsWith("blob:") &&
          !cachedAudioUrls.has(currentAudio)
        ) {
          URL.revokeObjectURL(currentAudio);
        }
        invalidateThumbnails();
        const loadedAssets = await loadClipAssets(
          projectId,
          clip.id,
          project,
          nextComposition,
        );
        if (!isLatestRequest()) return;
        if (!loadedAssets?.videoBlob && !clip.rawVideoPath) return;
        const clipUrls = await getClipMediaUrls(
          projectId,
          clip.id,
          project,
          nextComposition,
        );
        if (!isLatestRequest()) return;
        const nextBackground =
          getCompositionResolvedBackgroundConfig(nextComposition, clip.id) ??
          clip.backgroundConfig;
        const customBackground = loadedAssets?.customBackground ?? undefined;
        const resolvedBackground = {
          ...nextBackground,
          customBackground: nextBackground.customBackground ?? customBackground,
        };
        if (options?.preferPreloadedFrame) {
          drawPreloadedClipFrame(clip.id, resolvedBackground, nextComposition);
        }
        const videoObjectUrl = await videoControllerRef.current?.loadVideo(
          clipUrls
            ? {
                videoUrl: clipUrls.videoUrl,
                debugLabel: `clip-focus:${clip.id}`,
              }
            : {
                videoBlob: loadedAssets?.videoBlob ?? undefined,
                debugLabel: `clip-focus:${clip.id}`,
              },
        );
        if (!isLatestRequest()) return;
        if (videoObjectUrl) {
          setCurrentVideo(videoObjectUrl);
        }
        if (clipUrls?.audioUrl || loadedAssets?.audioBlob) {
          const audioObjectUrl = await videoControllerRef.current?.loadAudio(
            clipUrls?.audioUrl
              ? { audioUrl: clipUrls.audioUrl }
              : { audioBlob: loadedAssets?.audioBlob ?? undefined },
          );
          if (!isLatestRequest()) return;
          setCurrentAudio(audioObjectUrl || null);
        } else {
          setCurrentAudio(null);
        }
        if (!isLatestRequest()) return;
        setPreviewDuration(
          Math.max(videoControllerRef.current?.duration ?? 0, clipPreviewDuration),
        );
        setSegment(clip.segment);
        if (clipThumbnailPlaceholder) {
          setThumbnails(clipThumbnailPlaceholder);
        }
        setBackgroundConfig(resolvedBackground);
        setMousePositions(clip.mousePositions);
        setCurrentRecordingMode(clip.recordingMode ?? "withoutCursor");
        handleProjectRawVideoPathChange(clip.rawVideoPath ?? "");
        setLoadedClipId(clip.id);
        void generateThumbnailsForSource({
          videoUrl: clipUrls?.videoUrl ?? videoObjectUrl ?? null,
          filePath: clip.rawVideoPath,
          segment: clip.segment,
          deferMs: options?.deferThumbnailsMs ?? 120,
        });
      } finally {
        queueMicrotask(() => {
          if (isLatestRequest()) {
            isSwitchingCompositionClipRef.current = false;
          }
        });
      }
    },
    [
      composition,
      currentAudio,
      currentProjectData,
      currentVideo,
      drawPreloadedClipFrame,
      getClipMediaUrls,
      handleProjectRawVideoPathChange,
      loadClipAssets,
      setCurrentAudio,
      setCurrentVideo,
      setPreviewDuration,
      setThumbnails,
      generateThumbnailsForSource,
      setMousePositions,
      invalidateThumbnails,
      setSegment,
      videoControllerRef,
    ],
  );
  const projects = useProjects({
    videoControllerRef,
    setCurrentVideo,
    setCurrentAudio,
    setSegment,
    setBackgroundConfig,
    setMousePositions,
    setThumbnails,
    setCurrentRecordingMode,
    setCurrentRawVideoPath: handleProjectRawVideoPathChange,
    onProjectLoaded: (project) => {
      clearClipMediaCaches({
        preserveVideoUrl: currentVideo,
        preserveAudioUrl: currentAudio,
      });
      clipExportSourcePathCacheRef.current.clear();
      setCurrentProjectData(project);
      const nextComposition = ensureProjectComposition(project);
      setComposition(nextComposition);
      setLoadedClipId(null);
      if (spreadAnimationTimerRef.current) {
        clearTimeout(spreadAnimationTimerRef.current);
      }
      setSpreadFromClipId(null);
      const nextClipId =
        nextComposition.focusedClipId ?? nextComposition.selectedClipId;
      if (nextClipId) {
        void loadClipMediaIntoEditor(
          project.id,
          nextClipId,
          project,
          nextComposition,
        );
      }
    },
    currentVideo,
    currentAudio,
  });
  const resolveClipExportSourcePath = useCallback(
    async (clip: ProjectCompositionClip): Promise<string> => {
      const projectId = projects.currentProjectId;
      if (!projectId) {
        throw new Error("Project not loaded");
      }
      const cacheKey = `${projectId}:${clip.id}`;
      const cached = clipExportSourcePathCacheRef.current.get(cacheKey);
      if (cached) {
        return cached;
      }
      if (clip.rawVideoPath) {
        clipExportSourcePathCacheRef.current.set(cacheKey, clip.rawVideoPath);
        return clip.rawVideoPath;
      }
      const assets = await loadClipAssets(projectId, clip.id);
      if (!assets?.videoBlob) {
        throw new Error(`Clip "${clip.name}" is missing source media`);
      }
      const tempPath = await writeBlobToTempMediaFile(assets.videoBlob);
      clipExportSourcePathCacheRef.current.set(cacheKey, tempPath);
      return tempPath;
    },
    [loadClipAssets, projects.currentProjectId],
  );
  const hasSequenceChain = (composition?.clips.length ?? 0) > 1;
  const selectedClipId = hasSequenceChain
    ? (composition?.focusedClipId ?? composition?.selectedClipId ?? null)
    : null;
  const activeClipId = hasSequenceChain
    ? (loadedClipId ?? selectedClipId)
    : null;
  const compositionSyncClipId = composition
    ? hasSequenceChain
      ? activeClipId
      : "root"
    : null;
  const activeCompositionClip = useMemo(
    () => (hasSequenceChain ? getCompositionClip(composition, activeClipId) : null),
    [activeClipId, composition, hasSequenceChain],
  );
  const { previousClipId, nextClipId } = useMemo(
    () =>
      hasSequenceChain
        ? getCompositionAdjacentClipIds(composition, activeClipId)
        : { previousClipId: null, nextClipId: null },
    [activeClipId, composition, hasSequenceChain],
  );
  const currentTime = previewCurrentTime;
  const setCurrentTime = setPreviewCurrentTime;
  const duration = previewDuration;
  const isPlaying = isPreviewPlaying;

  useEffect(() => {
    if (!PLAYBACK_RESET_DEBUG) return;
    const previousTime = playbackResetPrevTimeRef.current;
    playbackResetPrevTimeRef.current = currentTime;
    if (!currentVideo || isRecording || isLoadingVideo) return;
    if (previousTime <= 0.5 || currentTime > 0.05 || previousTime - currentTime <= 0.5)
      return;
    const payload = {
      reason: "app-state-regressed-to-start",
      previousTime,
      currentTime,
      isPlaying,
      isVideoReady,
      currentProjectId: projects.currentProjectId,
      hasSequenceChain,
      loadedClipId,
      selectedClipId,
      switchingClip: isSwitchingCompositionClipRef.current,
    };
    const signature = JSON.stringify({
      previousTime: Number(previousTime.toFixed(3)),
      currentTime: Number(currentTime.toFixed(3)),
      isPlaying,
      currentProjectId: projects.currentProjectId,
      loadedClipId,
      selectedClipId,
    });
    const now = Date.now();
    if (
      playbackResetLastSignatureRef.current === signature &&
      now - playbackResetLastAtRef.current < 800
    ) {
      return;
    }
    playbackResetLastSignatureRef.current = signature;
    playbackResetLastAtRef.current = now;
    console.warn("[PlaybackReset]", payload);
  }, [
    currentTime,
    currentVideo,
    hasSequenceChain,
    isLoadingVideo,
    isPlaying,
    isRecording,
    isVideoReady,
    loadedClipId,
    projects.currentProjectId,
    selectedClipId,
  ]);

  // FPS of the most-recent recording (set on stop, cleared when a different project loads).
  const [lastCaptureFps, setLastCaptureFps] = useState<number | null>(null);

  // Export
  const exportHook = useExport({
    videoRef,
    canvasRef,
    tempCanvasRef,
    audioRef,
    segment,
    backgroundConfig,
    isRecording,
    mousePositions,
    audioFilePath,
    videoFilePath,
    videoFilePathOwnerUrl,
    rawVideoPath: currentRawVideoPath,
    savedRawVideoPath: lastRawSavedPath,
    currentVideo,
    lastCaptureFps,
    composition,
    currentProjectId: projects.currentProjectId,
    resolveClipExportSourcePath,
  });

  const handleExportSuccessPathChange = useCallback(
    async (newPath: string) => {
      exportHook.setLastExportedPath(newPath);
    },
    [exportHook],
  );

  // Zoom keyframes
  const zoomKeyframes = useZoomKeyframes({
    segment,
    setSegment,
    videoRef,
    currentTime,
    isVideoReady,
    renderFrame,
    activePanel,
    setActivePanel,
  });
  const {
    editingKeyframeId,
    setEditingKeyframeId,
    zoomFactor,
    setZoomFactor,
    handleAddKeyframe,
    handleDeleteKeyframe,
    throttledUpdateZoom,
  } = zoomKeyframes;

  // Text overlays
  const textOverlays = useTextOverlays({
    segment,
    setSegment,
    currentTime,
    duration,
    setActivePanel,
  });
  const {
    editingTextId,
    setEditingTextId,
    handleAddText,
    handleDeleteText,
    handleTextDragMove,
  } = textOverlays;
  const [editingKeystrokeSegmentId, setEditingKeystrokeSegmentId] = useState<
    string | null
  >(null);

  // Auto zoom
  const { handleAutoZoom } = useAutoZoom({
    segment,
    setSegment,
    videoRef,
    mousePositions,
    duration,
    currentProjectId: projects.currentProjectId,
    backgroundConfig,
    loadProjects: projects.loadProjects,
    setActivePanel,
  });

  // Cursor hiding
  const cursorHiding = useCursorHiding({
    segment,
    setSegment,
    mousePositions,
    currentTime,
    duration,
    videoRef,
    backgroundConfig,
  });
  const {
    editingPointerId,
    setEditingPointerId,
    handleSmartPointerHiding,
    handleAddPointerSegment,
    handleDeletePointerSegment,
  } = cursorHiding;
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

  const getAutoCanvasSelectionConfig = useCallback(() => {
    const crop = segment?.crop ?? { x: 0, y: 0, width: 1, height: 1 };
    const sourceWidth =
      videoRef.current?.videoWidth || canvasRef.current?.width || 0;
    const sourceHeight =
      videoRef.current?.videoHeight || canvasRef.current?.height || 0;
    const derivedWidth =
      sourceWidth > 0 ? Math.max(2, Math.round(sourceWidth * crop.width)) : undefined;
    const derivedHeight =
      sourceHeight > 0
        ? Math.max(2, Math.round(sourceHeight * crop.height))
        : undefined;
    return {
      canvasMode: "auto" as const,
      canvasWidth: derivedWidth,
      canvasHeight: derivedHeight,
      autoSourceClipId:
        activeClipId ??
        getCompositionAutoSourceClipId(composition) ??
        composition?.focusedClipId ??
        composition?.selectedClipId ??
        "root",
    };
  }, [
    activeClipId,
    canvasRef,
    composition,
    segment?.crop,
    videoRef,
  ]);

  useEffect(() => {
    segmentRef.current = segment;
  }, [segment]);

  useEffect(() => {
    return () => {
      if (spreadAnimationTimerRef.current) {
        clearTimeout(spreadAnimationTimerRef.current);
      }
      clearClipMediaCaches();
    };
  }, [clearClipMediaCaches]);

  useEffect(() => {
    if (
      !hasSequenceChain ||
      !projects.currentProjectId ||
      !currentProjectData ||
      !composition
    )
      return;
    void primePreloadSlot(
      "previous",
      previousClipId,
      currentProjectData,
      composition,
    );
    void primePreloadSlot("next", nextClipId, currentProjectData, composition);
  }, [
    composition,
    currentProjectData,
    hasSequenceChain,
    nextClipId,
    previousClipId,
    primePreloadSlot,
    projects.currentProjectId,
  ]);

  useEffect(() => {
    if (
      !composition ||
      !segment ||
      !compositionSyncClipId ||
      isSwitchingCompositionClipRef.current
    )
      return;
    setComposition((prev) => {
      if (!prev) return prev;
      const canvasConfig = extractCanvasConfig(backgroundConfig);
      let next = syncCompositionCanvasConfig(prev, canvasConfig);
      const currentClipBackground =
        getCompositionClip(next, compositionSyncClipId)?.backgroundConfig ??
        applyCanvasConfig(backgroundConfig, canvasConfig);
      next = updateCompositionClip(next, compositionSyncClipId, {
        segment,
        backgroundConfig:
          prev.mode === "separate"
            ? applyCanvasConfig(backgroundConfig, canvasConfig)
            : currentClipBackground,
        mousePositions,
        duration: Math.max(duration, segment.trimEnd),
        recordingMode: currentRecordingMode,
        rawVideoPath: currentRawVideoPath || undefined,
      });
      if (prev.mode === "unified") {
        next = {
          ...next,
          unifiedSourceClipId:
            prev.unifiedSourceClipId ?? compositionSyncClipId,
          globalPresentationConfig: applyCanvasConfig(
            backgroundConfig,
            canvasConfig,
          ),
          globalBackgroundConfig: applyCanvasConfig(
            backgroundConfig,
            canvasConfig,
          ),
        };
      }
      return next;
    });
  }, [
    backgroundConfig,
    composition?.focusedClipId,
    composition?.selectedClipId,
    composition?.mode,
    compositionSyncClipId,
    currentRawVideoPath,
    currentRecordingMode,
    duration,
    mousePositions,
    segment,
  ]);

  // Persist last-used background config so new projects inherit previous project settings.
  useEffect(() => {
    try {
      localStorage.setItem(
        LAST_BG_CONFIG_KEY,
        JSON.stringify(backgroundConfig),
      );
    } catch {
      // ignore persistence failures
    }
  }, [backgroundConfig]);

  const focusCompositionClip = useCallback(
    async (
      clipId: string,
      options?: { seekTime?: number; playAfterLoad?: boolean },
    ) => {
      if (!projects.currentProjectId || !composition) return;
      const requestId = clipLoadRequestSeqRef.current + 1;
      const nextComposition = withCompositionSelection(composition, clipId);
      const targetClip = getCompositionClip(nextComposition, clipId);
      if (!targetClip) return;
      setComposition(nextComposition);
      await loadClipMediaIntoEditor(
        projects.currentProjectId,
        clipId,
        currentProjectData,
        nextComposition,
        {
          preferPreloadedFrame: true,
          requestId,
        },
      );
      if (clipLoadRequestSeqRef.current !== requestId) return;
      const targetSeekTime =
        typeof options?.seekTime === "number"
          ? options.seekTime
          : targetClip.segment.trimStart;
      await new Promise<void>((resolve) =>
        requestAnimationFrame(() => resolve()),
      );
      if (clipLoadRequestSeqRef.current !== requestId) return;
      seek(targetSeekTime);
      if (options?.playAfterLoad) {
        videoControllerRef.current?.play();
      }
    },
    [
      composition,
      currentProjectData,
      loadClipMediaIntoEditor,
      projects.currentProjectId,
      seek,
      videoControllerRef,
    ],
  );

  const handleTogglePlayPause = useCallback(() => {
    if (
      hasSequenceChain &&
      !isPlaying &&
      composition &&
      activeCompositionClip &&
      currentTime >= activeCompositionClip.segment.trimEnd - 0.04
    ) {
      const targetClipId = nextClipId ?? composition.clips[0]?.id ?? null;
      if (targetClipId && targetClipId !== activeClipId) {
        const targetClip = getCompositionClip(composition, targetClipId);
        void focusCompositionClip(targetClipId, {
          seekTime: targetClip?.segment.trimStart,
          playAfterLoad: true,
        });
        return;
      }
      if (targetClipId && activeCompositionClip) {
        seek(activeCompositionClip.segment.trimStart);
        requestAnimationFrame(() => {
          videoControllerRef.current?.play();
        });
        return;
      }
    }

    togglePlayback();
  }, [
    activeClipId,
    activeCompositionClip,
    composition,
    currentTime,
    focusCompositionClip,
    hasSequenceChain,
    isPlaying,
    nextClipId,
    togglePlayback,
  ]);

  useEffect(() => {
    if (!hasSequenceChain || !activeCompositionClip || !nextClipId || !isPlaying)
      return;
    const activeEndTime = activeCompositionClip.segment.trimEnd;
    const remaining = activeEndTime - currentTime;
    if (remaining > 0.04) return;
    if (isSwitchingCompositionClipRef.current) return;
    const upcomingClip = getCompositionClip(composition, nextClipId);
    if (!upcomingClip) return;
    void focusCompositionClip(nextClipId, {
      seekTime: upcomingClip.segment.trimStart,
      playAfterLoad: true,
    });
  }, [
    activeCompositionClip,
    composition,
    currentTime,
    focusCompositionClip,
    hasSequenceChain,
    isPlaying,
    nextClipId,
  ]);

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
      localStorage.setItem("screen-record-capture-target-v1", captureTargetId);
    } catch {}
  }, [captureTargetId]);

  useEffect(() => {
    try {
      if (captureFps === null)
        localStorage.removeItem("screen-record-capture-fps-v1");
      else
        localStorage.setItem(
          "screen-record-capture-fps-v1",
          captureFps.toString(),
        );
    } catch {}
    captureFpsRef.current = captureFps;
  }, [captureFps]);

  useEffect(() => {
    try {
      const saved = localStorage.getItem("screen-record-capture-target-v1");
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
  const handleCancelCrop = useCallback(() => {
    setIsCropping(false);
    setActivePanel("background");
    setZoomFactor(1.0);
    setEditingKeyframeId(null);
  }, [setZoomFactor, setEditingKeyframeId]);

  const handleApplyCrop = useCallback(
    (crop: VideoSegment["crop"]) => {
      if (segment && crop) {
        setSegment({
          ...segment,
          crop,
        });
      }
      handleCancelCrop();
    },
    [segment, setSegment, handleCancelCrop],
  );

  const handleToggleCrop = useCallback(() => {
    if (isCropping) {
      handleCancelCrop();
    } else {
      setIsCropping(true);
      if (isPlaying) handleTogglePlayPause();
    }
  }, [
    isCropping,
    isPlaying,
    handleTogglePlayPause,
    handleCancelCrop,
  ]);

  // Track active preview drag listeners for cleanup on unmount.
  const previewDragCleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    return () => {
      previewDragCleanupRef.current?.();
    };
  }, []);

  const handlePreviewMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!currentVideo || isCropping || activePanel === "text") return;
      e.preventDefault();
      e.stopPropagation();
      if (isPlaying) handleTogglePlayPause();

      const startX = e.clientX;
      const startY = e.clientY;
      const lastState = videoRenderer.getLastCalculatedState();
      if (!lastState) return;

      const {
        positionX: startPosX,
        positionY: startPosY,
        zoomFactor: z,
      } = lastState;
      const rect = e.currentTarget.getBoundingClientRect();
      beginBatch();
      setIsPreviewDragging(true);
      let lockedAxis: "x" | "y" | null = null;

      const handleMouseMove = (me: MouseEvent) => {
        let dx = me.clientX - startX;
        let dy = me.clientY - startY;

        if (me.shiftKey) {
          if (!lockedAxis) {
            if (Math.abs(dx) > Math.abs(dy)) lockedAxis = "x";
            else lockedAxis = "y";
          }
          if (lockedAxis === "x") dy = 0;
          if (lockedAxis === "y") dx = 0;
        } else {
          lockedAxis = null;
        }

        handleAddKeyframe({
          zoomFactor: z,
          positionX: Math.max(0, Math.min(1, startPosX - dx / rect.width / z)),
          positionY: Math.max(0, Math.min(1, startPosY - dy / rect.height / z)),
        });
        setActivePanel("zoom");
      };

      const handleMouseUp = () => {
        window.removeEventListener("mousemove", handleMouseMove);
        window.removeEventListener("mouseup", handleMouseUp);
        previewDragCleanupRef.current = null;
        setIsPreviewDragging(false);
        commitBatch();
      };

      // Store cleanup so unmount can remove listeners if mouseup never fires.
      previewDragCleanupRef.current = () => {
        window.removeEventListener("mousemove", handleMouseMove);
        window.removeEventListener("mouseup", handleMouseUp);
      };

      window.addEventListener("mousemove", handleMouseMove);
      window.addEventListener("mouseup", handleMouseUp);
    },
    [
      currentVideo,
      isCropping,
      activePanel,
      isPlaying,
      handleTogglePlayPause,
      handleAddKeyframe,
      beginBatch,
      commitBatch,
    ],
  );

  const previewCursorClass = useMemo(() => {
    if (isKeystrokeResizeDragging || isKeystrokeResizeHandleHover)
      return "cursor-nwse-resize";
    if (isPreviewDragging) return "cursor-grabbing";
    if (currentVideo && !isCropping) return "cursor-grab";
    return "cursor-default";
  }, [
    isKeystrokeResizeDragging,
    isKeystrokeResizeHandleHover,
    isPreviewDragging,
    currentVideo,
    isCropping,
  ]);

  const hasAppliedCrop = useMemo(() => {
    const crop = segment?.crop;
    if (!crop) return false;
    return (
      Math.abs(crop.x) > 0.0005 ||
      Math.abs(crop.y) > 0.0005 ||
      Math.abs(crop.width - 1) > 0.0005 ||
      Math.abs(crop.height - 1) > 0.0005
    );
  }, [
    segment?.crop?.x,
    segment?.crop?.y,
    segment?.crop?.width,
    segment?.crop?.height,
  ]);

  const showPlaybackControls = Boolean(
    currentVideo &&
      !isLoadingVideo &&
      !projects.showProjectsDialog &&
      !isCropping,
  );
  const showPlaybackControlsGhost = Boolean(
    !currentVideo && !isLoadingVideo && !isCropping,
  );
  const showSequencePillGhost = Boolean(
    !composition && !isCropping && !currentVideo && !isLoadingVideo,
  );

  const handleBackgroundUpload = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
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

            const canvas = document.createElement("canvas");
            canvas.width = w;
            canvas.height = h;
            const ctx = canvas.getContext("2d");
            if (!ctx) throw new Error("Failed to get 2D canvas context");
            ctx.imageSmoothingEnabled = true;
            ctx.imageSmoothingQuality = "high";
            ctx.drawImage(img, 0, 0, w, h);

            // Convert to JPEG to reduce IPC payload size (backgrounds do not need alpha).
            const dataUrl = canvas.toDataURL("image/jpeg", 0.92);
            const imageUrl = await invoke<string>("save_uploaded_bg_data_url", {
              dataUrl,
            });
            await invoke("prewarm_custom_background", { url: imageUrl });
            setBackgroundConfig((prev) => ({
              ...prev,
              backgroundType: "custom",
              customBackground: imageUrl,
            }));
            setRecentUploads((prev) =>
              [imageUrl, ...prev.filter((v) => v !== imageUrl)].slice(0, 12),
            );
          } catch (err) {
            console.error(
              "[Background] Failed to persist uploaded image:",
              err,
            );
          } finally {
            URL.revokeObjectURL(img.src);
            setIsBackgroundUploadProcessing(false);
            inputEl.value = "";
          }
        };

        img.onerror = () => {
          URL.revokeObjectURL(img.src);
          setIsBackgroundUploadProcessing(false);
          inputEl.value = "";
        };

        img.src = URL.createObjectURL(file);
      }
    },
    [setBackgroundConfig, setRecentUploads],
  );

  const handleRemoveRecentUpload = useCallback((imageUrl: string) => {
    setRecentUploads((prev) => prev.filter((v) => v !== imageUrl));
    setBackgroundConfig((prev) => {
      if (
        prev.backgroundType === "custom" &&
        prev.customBackground === imageUrl
      ) {
        return {
          ...prev,
          backgroundType: DEFAULT_BUILT_IN_BACKGROUND_ID,
          customBackground: undefined,
        };
      }
      return prev;
    });
  }, []);

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
    }, [segment, currentTime, getKeystrokeTimelineDuration]);

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
  }, [keystrokeOverlayEditBounds]);

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

  // Persist crop preference so newly recorded/imported videos inherit the last crop.
  useEffect(() => {
    if (!segment) return;
    saveCropPref(segment.crop);
  }, [
    segment?.crop?.x,
    segment?.crop?.y,
    segment?.crop?.width,
    segment?.crop?.height,
    segment,
  ]);

  const persistCurrentProjectNow = useCallback(
    async (options?: {
      refreshList?: boolean;
      includeMedia?: boolean;
      compositionOverride?: ProjectComposition;
      skipLiveCompositionSync?: boolean;
    }) => {
      const compositionState = options?.compositionOverride ?? composition;
      const shouldSyncLiveComposition = !options?.skipLiveCompositionSync;
      if (
        !projects.currentProjectId ||
        !compositionState ||
        (shouldSyncLiveComposition && isSwitchingCompositionClipRef.current) ||
        (shouldSyncLiveComposition && !segment)
      ) {
        return;
      }
      const projectId = projects.currentProjectId;
      const saveSeq = ++projectSaveSeqRef.current;
      const includeMedia = options?.includeMedia !== false;
      const activeClipId = shouldSyncLiveComposition
        ? loadedClipId ??
          compositionState.focusedClipId ??
          compositionState.selectedClipId
        : compositionState.focusedClipId ?? compositionState.selectedClipId;
      const activeClip = activeClipId
        ? getCompositionClip(compositionState, activeClipId)
        : null;
      if (!activeClip) return;
      debugProject("persist:start", {
        saveSeq,
        projectId,
        refreshList: options?.refreshList ?? true,
        includeMedia,
        canvasMode: backgroundConfig.canvasMode,
        canvasWidth: backgroundConfig.canvasWidth,
        canvasHeight: backgroundConfig.canvasHeight,
      });
      try {
        const loadedAssets = await loadClipAssets(
          projectId,
          activeClip.id,
          currentProjectData,
          compositionState,
        );
        let videoBlob: Blob | undefined;
        let thumbnail: string | undefined;
        if (includeMedia && activeClip.role === "root") {
          // Use the currently rendered preview frame whenever possible so the
          // project card thumbnail matches exactly what the user just saw.
          const canvasSnapshot = (() => {
            try {
              return canvasRef.current?.toDataURL("image/jpeg", 0.8);
            } catch {
              return undefined;
            }
          })();

          videoBlob = loadedAssets?.videoBlob ?? currentProjectData?.videoBlob;
          if (!videoBlob && currentVideo && !currentRawVideoPath) {
            const response = await fetch(currentVideo);
            videoBlob = await response.blob();
          }
          thumbnail =
            canvasSnapshot ||
            generateThumbnail() ||
            activeClip.thumbnail;
        }
        const canvasConfig = extractCanvasConfig(backgroundConfig);
        let nextComposition = compositionState;
        if (shouldSyncLiveComposition) {
          nextComposition = syncCompositionCanvasConfig(
            nextComposition,
            canvasConfig,
          );
          nextComposition = updateCompositionClip(
            nextComposition,
            activeClip.id,
            {
              segment: segment!,
              backgroundConfig:
                nextComposition.mode === "separate"
                  ? applyCanvasConfig(backgroundConfig, canvasConfig)
                  : (getCompositionClip(nextComposition, activeClip.id)
                      ?.backgroundConfig ?? activeClip.backgroundConfig),
              mousePositions,
              duration: Math.max(duration, segment!.trimEnd),
              thumbnail:
                activeClip.role === "root"
                  ? (thumbnail ?? activeClip.thumbnail)
                  : activeClip.thumbnail,
              recordingMode: currentRecordingMode,
              rawVideoPath: currentRawVideoPath || undefined,
            },
          );
          if (nextComposition.mode === "unified") {
            nextComposition = {
              ...nextComposition,
              globalPresentationConfig: applyCanvasConfig(
                backgroundConfig,
                canvasConfig,
              ),
              globalBackgroundConfig: applyCanvasConfig(
                backgroundConfig,
                canvasConfig,
              ),
            };
          }
        }
        if (
          includeMedia &&
          activeClip.role === "snapshot" &&
          !currentRawVideoPath
        ) {
          let snapshotVideoBlob = loadedAssets?.videoBlob ?? undefined;
          if (!snapshotVideoBlob && currentVideo) {
            const response = await fetch(currentVideo);
            snapshotVideoBlob = await response.blob();
          }
          if (!snapshotVideoBlob) return;
          let snapshotAudioBlob = loadedAssets?.audioBlob ?? undefined;
          if (!snapshotAudioBlob && currentAudio) {
            const audioResponse = await fetch(currentAudio);
            snapshotAudioBlob = await audioResponse.blob();
          }
          await projectManager.saveCompositionClipAssets(
            projectId,
            activeClip.id,
            {
              videoBlob: snapshotVideoBlob,
              audioBlob: snapshotAudioBlob,
              customBackground: backgroundConfig.customBackground,
            },
          );
        }
        // Drop stale in-flight saves so older state never overwrites newer edits.
        if (saveSeq !== projectSaveSeqRef.current) {
          debugProject("persist:stale-before-write", {
            saveSeq,
            latestSeq: projectSaveSeqRef.current,
            projectId,
          });
          return;
        }
        const rootClip = getCompositionClip(nextComposition, "root");
        if (!rootClip) return;
        await projectManager.updateProject(projectId, {
          name:
            projects.projects.find((p) => p.id === projectId)?.name ||
            "Auto Saved",
          videoBlob,
          segment: rootClip.segment,
          backgroundConfig: rootClip.backgroundConfig,
          mousePositions: rootClip.mousePositions,
          thumbnail:
            activeClip.role === "root"
              ? thumbnail
              : currentProjectData?.thumbnail,
          duration: rootClip.duration,
          recordingMode: rootClip.recordingMode ?? currentRecordingMode,
          rawVideoPath: rootClip.rawVideoPath,
          composition: nextComposition,
        });
        setComposition(nextComposition);
        if (saveSeq !== projectSaveSeqRef.current) {
          debugProject("persist:stale-after-write", {
            saveSeq,
            latestSeq: projectSaveSeqRef.current,
            projectId,
          });
          return;
        }
        debugProject("persist:committed", {
          saveSeq,
          projectId,
          canvasMode: backgroundConfig.canvasMode,
          canvasWidth: backgroundConfig.canvasWidth,
          canvasHeight: backgroundConfig.canvasHeight,
        });
        if (options?.refreshList !== false) {
          await projects.loadProjects();
          debugProject("persist:projects-refreshed", { saveSeq, projectId });
        }
      } catch (error) {
        debugProject("persist:error", {
          saveSeq,
          projectId,
          error: String(error),
        });
      }
    },
    [
      projects.currentProjectId,
      projects.projects,
      projects.loadProjects,
      currentVideo,
      currentAudio,
      loadedClipId,
      currentProjectData,
      segment,
      composition,
      backgroundConfig,
      mousePositions,
      generateThumbnail,
      duration,
      debugProject,
      currentRecordingMode,
      currentRawVideoPath,
      loadClipAssets,
    ],
  );
  persistRef.current = persistCurrentProjectNow;

  const handleLoadProjectFromGrid = useCallback(
    async (projectId: string) => {
      // Always persist the currently open project before loading another one.
      debugProject("grid-load:start", {
        targetProjectId: projectId,
        currentProjectId: projects.currentProjectId,
      });
      if (projectId === projects.currentProjectId) {
        projects.setShowProjectsDialog(false);
        debugProject("grid-load:same-project-close", {
          targetProjectId: projectId,
        });
        return;
      }
      void persistCurrentProjectNow({
        refreshList: false,
        includeMedia: false,
      });
      setLastCaptureFps(null); // loading a different project — probe should determine its FPS
      await projects.handleLoadProject(projectId);
      debugProject("grid-load:done", { targetProjectId: projectId });
    },
    [persistCurrentProjectNow, projects, debugProject],
  );

  const requestCloseProjects = useCallback(() => {
    if (!projects.showProjectsDialog) return;
    window.dispatchEvent(new CustomEvent("sr-close-projects"));
  }, [projects.showProjectsDialog]);

  const handleToggleProjects = useCallback(async () => {
    if (projects.showProjectsDialog) {
      debugProject("projects-toggle:close");
      requestCloseProjects();
      return;
    }

    debugProject("projects-toggle:open:start", {
      currentProjectId: projects.currentProjectId,
      canvasMode: backgroundConfig.canvasMode,
      canvasWidth: backgroundConfig.canvasWidth,
      canvasHeight: backgroundConfig.canvasHeight,
    });
    // Persist in background to keep opening Projects instant.
    void persistCurrentProjectNow({ refreshList: true, includeMedia: false });

    if (canvasRef.current && currentVideo) {
      try {
        restoreImageRef.current = canvasRef.current.toDataURL(
          "image/jpeg",
          0.8,
        );
      } catch {
        restoreImageRef.current = null;
      }
    } else {
      restoreImageRef.current = null;
    }
    projects.setShowProjectsDialog(true);
    debugProject("projects-toggle:open:done", {
      currentProjectId: projects.currentProjectId,
    });
  }, [
    projects.showProjectsDialog,
    projects.currentProjectId,
    currentVideo,
    backgroundConfig.canvasMode,
    backgroundConfig.canvasWidth,
    backgroundConfig.canvasHeight,
    persistCurrentProjectNow,
    debugProject,
    projects,
    requestCloseProjects,
  ]);

  const handleOpenInsertProjectPicker = useCallback(
    (clipId: string | null, placement: "before" | "after") => {
      setSequenceTargetClipId(clipId);
      setProjectPickerMode(
        placement === "before" ? "insertBefore" : "insertAfter",
      );
      projects.setShowProjectsDialog(true);
    },
    [projects],
  );

  const handlePickProjectForSequence = useCallback(
    async (projectId: string) => {
      if (!projects.currentProjectId || !composition) return;
      const pickedProject = await projectManager.loadProject(projectId);
      if (!pickedProject) return;
      let snapshotRawVideoPath: string | undefined;
      if (pickedProject.rawVideoPath) {
        try {
          const saved = await invoke<{ savedPath: string }>(
            "save_composition_snapshot_copy",
            {
              sourcePath: pickedProject.rawVideoPath,
            },
          );
          snapshotRawVideoPath = saved?.savedPath || undefined;
        } catch (error) {
          console.error(
            "[Composition] Failed to create native snapshot copy:",
            error,
          );
        }
      }
      const snapshotClip = normalizeCompositionClipToCanvas(
        createCompositionSnapshotClip({
          ...pickedProject,
          rawVideoPath: snapshotRawVideoPath,
        }),
        composition.globalCanvasConfig ?? extractCanvasConfig(backgroundConfig),
      );
      if (!snapshotRawVideoPath && !pickedProject.videoBlob) {
        console.error(
          "[Composition] Insert failed: project has neither raw video path nor stored video blob",
        );
        return;
      }
      if (!snapshotRawVideoPath) {
        await projectManager.saveCompositionClipAssets(
          projects.currentProjectId,
          snapshotClip.id,
          {
            videoBlob: pickedProject.videoBlob,
            audioBlob: pickedProject.audioBlob,
            customBackground: pickedProject.backgroundConfig.customBackground,
          },
        );
      }
      const nextComposition = insertCompositionClip(
        composition,
        sequenceTargetClipId,
        projectPickerMode === "insertBefore" ? "before" : "after",
        snapshotClip,
      );
      setComposition(nextComposition);
      setProjectPickerMode(null);
      projects.setShowProjectsDialog(false);
      await loadClipMediaIntoEditor(
        projects.currentProjectId,
        snapshotClip.id,
        currentProjectData,
        nextComposition,
      );
      void persistCurrentProjectNow({
        refreshList: true,
        includeMedia: false,
        compositionOverride: nextComposition,
        skipLiveCompositionSync: true,
      });
    },
    [
      composition,
      currentProjectData,
      loadClipMediaIntoEditor,
      persistCurrentProjectNow,
      projectPickerMode,
      projects,
      sequenceTargetClipId,
    ],
  );

  const handleSelectSequenceClip = useCallback(
    async (clipId: string) => {
      const targetClip = getCompositionClip(composition, clipId);
      if (!targetClip) return;
      if (clipId === loadedClipId && !isSwitchingCompositionClipRef.current) {
        seek(targetClip.segment.trimStart);
        if (isPlaying) {
          videoControllerRef.current?.play();
        }
        return;
      }
      await focusCompositionClip(clipId, {
        seekTime: targetClip.segment.trimStart,
        playAfterLoad: isPlaying,
      });
    },
    [
      composition,
      focusCompositionClip,
      isPlaying,
      loadedClipId,
      seek,
      videoControllerRef,
    ],
  );

  const handleRemoveSequenceClip = useCallback(
    async (clipId: string) => {
      if (!projects.currentProjectId || !composition) return;
      const clip = getCompositionClip(composition, clipId);
      if (!clip || clip.role === "root" || composition.clips.length <= 1)
        return;
      const nextComposition = removeCompositionClip(composition, clipId);
      setComposition(nextComposition);
      await projectManager.deleteCompositionClipAssets(
        projects.currentProjectId,
        clipId,
      );
      if (
        clip.rawVideoPath &&
        isManagedCompositionSnapshotPath(clip.rawVideoPath)
      ) {
        try {
          await invoke("delete_file", { path: clip.rawVideoPath });
        } catch {
          // ignore cleanup failures for snapshot media copies
        }
      }
      clipAssetCacheRef.current.delete(clipId);
      const removedUrls = clipUrlCacheRef.current.get(clipId);
      if (removedUrls) {
        if (removedUrls.videoUrl.startsWith("blob:")) {
          URL.revokeObjectURL(removedUrls.videoUrl);
        }
        if (removedUrls.audioUrl?.startsWith("blob:")) {
          URL.revokeObjectURL(removedUrls.audioUrl);
        }
        clipUrlCacheRef.current.delete(clipId);
      }
      if (preloadedSlotClipIdsRef.current.previous === clipId) {
        preloadedSlotClipIdsRef.current.previous = null;
      }
      if (preloadedSlotClipIdsRef.current.next === clipId) {
        preloadedSlotClipIdsRef.current.next = null;
      }
      const nextClipId =
        nextComposition.focusedClipId ?? nextComposition.selectedClipId;
      if (nextClipId) {
        await loadClipMediaIntoEditor(
          projects.currentProjectId,
          nextClipId,
          currentProjectData,
          nextComposition,
        );
      }
      void persistCurrentProjectNow({
        refreshList: true,
        includeMedia: false,
        compositionOverride: nextComposition,
        skipLiveCompositionSync: true,
      });
    },
    [
      composition,
      currentProjectData,
      loadClipMediaIntoEditor,
      persistCurrentProjectNow,
      projects.currentProjectId,
    ],
  );

  const handleSequenceModeChange = useCallback(
    async (mode: ProjectCompositionMode) => {
      if (!composition || !projects.currentProjectId) return;
      const activeEditableClipId =
        composition.focusedClipId ?? composition.selectedClipId;
      if (!activeEditableClipId) return;
      const canvasConfig = extractCanvasConfig(backgroundConfig);
      let nextComposition = syncCompositionCanvasConfig(
        setCompositionMode(composition, mode),
        canvasConfig,
      );
      if (mode === "unified") {
        if (spreadAnimationTimerRef.current) {
          clearTimeout(spreadAnimationTimerRef.current);
        }
        setSpreadFromClipId(activeEditableClipId);
        spreadAnimationTimerRef.current = setTimeout(() => {
          setSpreadFromClipId(null);
        }, 900);
        nextComposition = {
          ...nextComposition,
          unifiedSourceClipId: activeEditableClipId,
          globalPresentationConfig: applyCanvasConfig(
            backgroundConfig,
            canvasConfig,
          ),
          globalBackgroundConfig: applyCanvasConfig(
            backgroundConfig,
            canvasConfig,
          ),
        };
      } else {
        if (spreadAnimationTimerRef.current) {
          clearTimeout(spreadAnimationTimerRef.current);
        }
        setSpreadFromClipId(null);
      }
      setComposition(nextComposition);
      const targetClipId =
        nextComposition.focusedClipId ?? nextComposition.selectedClipId;
      if (targetClipId) {
        await loadClipMediaIntoEditor(
          projects.currentProjectId,
          targetClipId,
          currentProjectData,
          nextComposition,
        );
      }
    },
    [
      backgroundConfig,
      composition,
      currentProjectData,
      loadClipMediaIntoEditor,
      projects.currentProjectId,
    ],
  );

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

  const handleSelectMonitorCapture = useCallback(
    (monitorId: string, fps: number | null) => {
      setCaptureSource("monitor");
      setCaptureFps(fps);
      captureFpsRef.current = fps;
      setCaptureTargetId(monitorId);
    },
    [],
  );

  const handleSelectWindowCapture = useCallback((fps: number | null) => {
    setCaptureSource("window");
    setCaptureFps(fps);
    captureFpsRef.current = fps;
    setCaptureTargetId("0");
  }, []);

  const finalizeStartRecording = useCallback(
    async (targetId: string, targetType: "monitor" | "window") => {
      await startNewRecording(
        targetId,
        selectedRecordingMode,
        targetType,
        captureFpsRef.current ?? undefined,
      );
    },
    [startNewRecording, selectedRecordingMode],
  );

  const handleSelectWindowForRecording = useCallback(
    async (windowId: string, _captureMethod: "game" | "window") => {
      setShowWindowSelect(false);
      setCaptureTargetId(windowId);
      if (!pendingWindowRecordingRef.current) return;
      pendingWindowRecordingRef.current = false;
      try {
        await finalizeStartRecording(windowId, "window");
      } catch (err) {
        setError(err as string);
      }
    },
    [finalizeStartRecording, setError],
  );

  const handleStartRecording = useCallback(async () => {
    if (isRecording || pendingWindowRecordingRef.current) return;
    try {
      if (projects.currentProjectId && currentVideo && segment) {
        await persistCurrentProjectNow({
          refreshList: false,
          includeMedia: false,
        });
      }
      setCurrentRecordingMode(selectedRecordingMode);
      if (!currentVideo) {
        setCurrentRawVideoPath("");
        setLastRawSavedPath("");
      }
      setRawButtonSavedFlash(false);

      let finalTargetId = captureTargetId;
      if (
        captureSource === "monitor" &&
        (!finalTargetId || finalTargetId === "0")
      ) {
        const monitorList =
          monitors.length > 0 ? monitors : await getMonitors();
        const primary = monitorList.find((m) => m.is_primary) ?? monitorList[0];
        finalTargetId = primary?.id ?? "0";
      }

      if (captureSource === "window") {
        pendingWindowRecordingRef.current = true;
        try {
          await invoke("show_window_selector");
        } catch (err) {
          pendingWindowRecordingRef.current = false;
          throw err;
        }
        return;
      }

      await finalizeStartRecording(finalTargetId, captureSource);
    } catch (err) {
      setError(err as string);
    }
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
    setError,
  ]);

  // Listen for window selections dispatched by the native overlay via IPC.
  useEffect(() => {
    const handler = (event: Event) => {
      const { windowId } = (event as CustomEvent<{ windowId: string }>).detail;
      handleSelectWindowForRecording(windowId, "window");
    };
    window.addEventListener("external-window-selected", handler);
    return () =>
      window.removeEventListener("external-window-selected", handler);
  }, [handleSelectWindowForRecording]);

  useEffect(() => {
    const handler = () => {
      pendingWindowRecordingRef.current = false;
    };
    window.addEventListener("external-window-selection-cancelled", handler);
    return () =>
      window.removeEventListener(
        "external-window-selection-cancelled",
        handler,
      );
  }, []);

  const onStopRecording = useCallback(async () => {
    setShowRawVideoDialog(false);
    exportHook.setShowExportSuccessDialog(false);
    const result = await handleStopRecording();
    if (result) {
      setComposition(null);
      setLoadedClipId(null);
      setCurrentProjectData(null);
      projects.setCurrentProjectId(null);
      requestCloseProjects();
      const {
        mouseData,
        initialSegment,
        videoUrl,
        recordingMode,
        rawVideoPath,
        capturedFps,
      } = result;
      setLastCaptureFps(capturedFps);
      setCurrentRecordingMode(recordingMode);
      setCurrentRawVideoPath(rawVideoPath || "");
      setLastRawSavedPath("");

      let autoSavedPath = "";
      if (rawAutoCopyEnabled && rawVideoPath && rawSaveDir) {
        try {
          setIsRawActionBusy(true);
          const saved = await invoke<{ savedPath: string }>(
            "save_raw_video_copy",
            {
              sourcePath: rawVideoPath,
              targetDir: rawSaveDir,
            },
          );
          autoSavedPath = saved?.savedPath || "";
          if (autoSavedPath) {
            setLastRawSavedPath(autoSavedPath);
            await invoke("copy_video_file_to_clipboard", {
              filePath: autoSavedPath,
            });
            flashRawSavedButton();
          }
        } catch (e) {
          console.error("[RawVideo] Auto-copy after recording failed:", e);
        } finally {
          setIsRawActionBusy(false);
        }
      }

      let videoBlob: Blob | undefined;
      if (!rawVideoPath) {
        const response = await fetch(videoUrl);
        videoBlob = await response.blob();
      }
      const thumbnail = generateThumbnail();
      const project = await projectManager.saveProject({
        name: `Recording ${new Date().toLocaleString()}`,
        videoBlob,
        segment: initialSegment,
        backgroundConfig,
        mousePositions: mouseData,
        thumbnail: thumbnail || undefined,
        duration: initialSegment.trimEnd,
        recordingMode,
        rawVideoPath: rawVideoPath || undefined,
      });
      projects.setCurrentProjectId(project.id);
      setCurrentProjectData(project);
      setComposition(ensureProjectComposition(project));
      setLoadedClipId("root");
      await projects.loadProjects();
    }
  }, [
    handleStopRecording,
    backgroundConfig,
    generateThumbnail,
    projects,
    rawAutoCopyEnabled,
    rawSaveDir,
    flashRawSavedButton,
    setShowRawVideoDialog,
    exportHook,
    requestCloseProjects,
    setComposition,
    setCurrentProjectData,
  ]);

  // Effects
  useEffect(() => {
    const handleToggle = () => {
      if (showHotkeyDialog) return;
      if (isRecording) onStopRecording();
      else handleStartRecording();
    };
    window.addEventListener("toggle-recording", handleToggle);
    return () => window.removeEventListener("toggle-recording", handleToggle);
  }, [isRecording, showHotkeyDialog, onStopRecording, handleStartRecording]);

  // Keyboard shortcuts
  useAppShortcuts({
    togglePlayPause: handleTogglePlayPause,
    currentTime,
    duration,
    seek,
    isCropping,
    isModalOpen: showRawVideoDialog || exportHook.showExportSuccessDialog,
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

      const newZoom = Math.max(
        1.0,
        Math.min(
          12.0,
          lastState.zoomFactor - e.deltaY * 0.002 * lastState.zoomFactor,
        ),
      );
      handleAddKeyframe({
        zoomFactor: newZoom,
        positionX: lastState.positionX,
        positionY: lastState.positionY,
      });
      setActivePanel("zoom");
    };

    container.addEventListener("wheel", handleWheel, { passive: false });
    return () => container.removeEventListener("wheel", handleWheel);
  }, [currentVideo, isCropping, handleAddKeyframe, beginBatch, commitBatch]);

  // Initialize segment
  useEffect(() => {
    if (duration > 0 && !segment) {
      const initialSegment: VideoSegment = {
        trimStart: 0,
        trimEnd: duration,
        trimSegments: [
          {
            id: crypto.randomUUID(),
            startTime: 0,
            endTime: duration,
          },
        ],
        zoomKeyframes: [],
        textSegments: [],
        speedPoints: [
          { time: 0, speed: 1 },
          { time: duration, speed: 1 },
        ],
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
        if (
          videoRef.current &&
          canvasRef.current &&
          videoRef.current.readyState >= 2
        ) {
          videoRenderer.drawFrame({
            video: videoRef.current,
            canvas: canvasRef.current,
            tempCanvas: tempCanvasRef.current,
            segment: initialSegment,
            backgroundConfig,
            mousePositions,
            currentTime: 0,
          });
        }
      }, 0);
    }
  }, [
    duration,
    segment,
    backgroundConfig,
    mousePositions,
    setSegment,
    videoRef,
    canvasRef,
    tempCanvasRef,
  ]);

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
  }, [
    segment,
    backgroundConfig,
    mousePositions,
    projects.currentProjectId,
    currentVideo,
  ]);

  // Text & keystroke overlay drag listeners
  useKeystrokeDrag({
    segment,
    setSegment,
    canvasRef,
    segmentRef,
    isDraggingKeystrokeOverlayRef,
    isResizingKeystrokeOverlayRef,
    keystrokeOverlayDragStartRef,
    currentTime,
    getKeystrokeTimelineDuration,
    setIsPreviewDragging,
    setIsKeystrokeResizeDragging,
    setIsKeystrokeResizeHandleHover,
    setIsKeystrokeOverlaySelected,
    setEditingTextId,
    setActivePanel,
    handleTextDragMove,
    beginBatch,
    commitBatch,
  });

  return (
    <SettingsContext.Provider value={settings}>
      <div className="app-container min-h-screen bg-[var(--surface)]">
        <ResizeBorders />
        <Header
          isRecording={isRecording}
          recordingDuration={recordingDuration}
          currentVideo={currentVideo}
          isProcessing={exportHook.isProcessing}
          hotkeys={hotkeys}
          onRemoveHotkey={handleRemoveHotkey}
          onOpenHotkeyDialog={openHotkeyDialog}
          recordingMode={selectedRecordingMode}
          onRecordingModeChange={setSelectedRecordingMode}
          rawButtonLabel={
            rawButtonSavedFlash ? t.rawVideoSavedButton : t.saveRawVideo
          }
          rawButtonPulse={currentRecordingMode === "withCursor"}
          rawButtonDisabled={!currentRawVideoPath && !lastRawSavedPath}
          onOpenRawVideoDialog={handleOpenRawVideoDialog}
          onExport={exportHook.handleExport}
          onOpenProjects={handleToggleProjects}
          onOpenCursorLab={() => {
            window.location.hash = "cursor-lab";
          }}
          hideExport={isOverlayMode}
          hideRawVideo={projects.showProjectsDialog}
          captureSource={captureSource}
          captureFps={captureFps}
          monitors={monitors}
          onSelectMonitorCapture={handleSelectMonitorCapture}
          onSelectWindowCapture={handleSelectWindowCapture}
        />

        <main
          className="app-main flex flex-col px-3 py-3 overflow-hidden"
          style={{ height: "calc(100vh - 44px)" }}
        >
          {error && (
            <p className="error-message text-[var(--tertiary-color)] mb-2 flex-shrink-0">
              {error}
            </p>
          )}

          <div className="content-layout flex gap-4 flex-1 min-h-0 pb-1">
            <div className="preview-and-controls flex-1 flex flex-col min-w-0 gap-3 relative">
              {/* Video Preview */}
              <div className="video-preview-container ui-surface flex-1 min-h-0 overflow-hidden flex items-center justify-center">
                <div className="preview-inner relative w-full h-full flex justify-center items-center">
                  <div
                    ref={previewContainerRef}
                    className={`preview-canvas relative flex items-center justify-center ${previewCursorClass} group w-full h-full`}
                    onMouseDown={handlePreviewMouseDown}
                  >
                    <canvas
                      ref={canvasRef}
                      className="preview-canvas-element absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 max-w-full max-h-full"
                    />
                    <canvas ref={tempCanvasRef} className="hidden" />
                    <video
                      ref={videoRef}
                      className="hidden"
                      crossOrigin="anonymous"
                      playsInline
                      preload="auto"
                    />
                    <audio ref={audioRef} className="hidden" />
                    <video
                      ref={previousPreloadVideoRef}
                      className="hidden"
                      crossOrigin="anonymous"
                      playsInline
                      preload="auto"
                      muted
                    />
                    <audio ref={previousPreloadAudioRef} className="hidden" />
                    <video
                      ref={nextPreloadVideoRef}
                      className="hidden"
                      crossOrigin="anonymous"
                      playsInline
                      preload="auto"
                      muted
                    />
                    <audio ref={nextPreloadAudioRef} className="hidden" />
                    {keystrokeOverlayEditFrame &&
                      (isKeystrokeOverlaySelected ||
                        isDraggingKeystrokeOverlayRef.current ||
                        isResizingKeystrokeOverlayRef.current) && (
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
                      <Placeholder
                        isLoadingVideo={isLoadingVideo}
                        loadingProgress={loadingProgress}
                        isRecording={isRecording}
                        recordingDuration={recordingDuration}
                      />
                    )}

                    {!isCropping &&
                      currentVideo &&
                      backgroundConfig.canvasMode === "custom" &&
                      backgroundConfig.canvasWidth &&
                      backgroundConfig.canvasHeight && (
                        <CanvasResizeOverlay
                          previewContainerRef={
                            previewContainerRef as React.RefObject<HTMLDivElement>
                          }
                          backgroundConfig={backgroundConfig}
                          setBackgroundConfig={setBackgroundConfig}
                          beginBatch={beginBatch}
                          commitBatch={commitBatch}
                          onDragStateChange={setIsCanvasResizeDragging}
                        />
                      )}

                    <SeekIndicator
                      dir={seekIndicatorDir}
                      showKey={seekIndicatorKey}
                    />
                  </div>
                </div>
              </div>

              <div
                className={`playback-controls-row flex-shrink-0 flex justify-center pb-1 min-h-[56px] transition-opacity duration-200 ${showPlaybackControls || showPlaybackControlsGhost ? "opacity-100" : "opacity-0 pointer-events-none"}`}
              >
                {showPlaybackControls && (
                  <PlaybackControls
                    isPlaying={isPlaying}
                    isProcessing={exportHook.isProcessing}
                    isVideoReady={isVideoReady}
                    isCropping={isCropping}
                    hasAppliedCrop={hasAppliedCrop}
                    currentTime={currentTime}
                    duration={duration}
                    wallClockCurrentTime={wallClockCurrentTime}
                    wallClockDuration={wallClockDuration}
                    onTogglePlayPause={handleTogglePlayPause}
                    onToggleCrop={handleToggleCrop}
                    canvasModeToggle={
                      <div className="playback-canvas-mode-toggle ui-segmented">
                        {(["auto", "custom"] as const).map((mode) => {
                          const isActive =
                            (backgroundConfig.canvasMode ?? "auto") === mode;
                          return (
                            <button
                              key={mode}
                              type="button"
                              aria-pressed={isActive}
                              data-active={isActive ? "true" : "false"}
                              onClick={() => {
                                if (mode === "custom") {
                                  setBackgroundConfig((prev) => {
                                    const w =
                                      prev.canvasWidth ??
                                      canvasRef.current?.width ??
                                      1920;
                                    const h =
                                      prev.canvasHeight ??
                                      canvasRef.current?.height ??
                                      1080;
                                    return {
                                      ...prev,
                                      canvasMode: "custom",
                                      canvasWidth: w,
                                      canvasHeight: h,
                                      autoCanvasSourceId: null,
                                    };
                                  });
                                } else {
                                  const autoCanvasConfig =
                                    getAutoCanvasSelectionConfig();
                                  setBackgroundConfig((prev) => ({
                                    ...prev,
                                    canvasMode: "auto",
                                    canvasWidth: autoCanvasConfig.canvasWidth,
                                    canvasHeight: autoCanvasConfig.canvasHeight,
                                    autoCanvasSourceId:
                                      autoCanvasConfig.autoSourceClipId,
                                  }));
                                }
                              }}
                              className={`playback-canvas-mode-btn playback-canvas-mode-btn-${mode} ui-segmented-button ${isActive ? "playback-canvas-mode-btn-active ui-segmented-button-active" : "playback-canvas-mode-btn-inactive"} px-2 py-1 text-[10px] font-semibold ${
                                isActive
                                  ? ""
                                  : "text-[var(--overlay-panel-fg)]/70 hover:text-[var(--overlay-panel-fg)]"
                              }`}
                            >
                              {mode === "auto" ? t.canvasAuto : t.canvasCustom}
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
                          className={`playback-keystroke-toggle-btn ui-action-button h-7 text-[11px] transition-colors ${
                            !segment
                              ? "text-[var(--overlay-panel-fg)]/40 cursor-not-allowed"
                              : (segment.keystrokeMode ?? "off") === "off"
                                ? "ui-chip-button bg-transparent text-[var(--overlay-panel-fg)]/85 hover:text-[var(--overlay-panel-fg)]"
                                : ""
                          }`}
                          data-tone="success"
                          data-active={
                            !segment || (segment.keystrokeMode ?? "off") === "off"
                              ? "false"
                              : "true"
                          }
                        >
                          <Keyboard className="playback-keystroke-toggle-icon w-3.5 h-3.5 mr-1.5" />
                          <span className="playback-keystroke-toggle-label">
                            {(segment?.keystrokeMode ?? "off") === "keyboard"
                              ? t.keystrokeModeKeyboard
                              : (segment?.keystrokeMode ?? "off") ===
                                  "keyboardMouse"
                                ? t.keystrokeModeKeyboardMouse
                                : t.keystrokeModeOff}
                          </span>
                        </Button>
                        <div className="playback-keystroke-delay-popover absolute left-1/2 -translate-x-1/2 bottom-[calc(100%+4px)] w-[308px] px-2.5 py-2 rounded-lg border pointer-events-none opacity-0 translate-y-1 transition-all duration-150 group-hover/playback-keystroke:opacity-100 group-hover/playback-keystroke:translate-y-0 group-hover/playback-keystroke:pointer-events-auto group-focus-within/playback-keystroke:opacity-100 group-focus-within/playback-keystroke:translate-y-0 group-focus-within/playback-keystroke:pointer-events-auto">
                          <div className="playback-keystroke-delay-row flex items-center gap-3">
                            <div className="playback-keystroke-delay-slider-shell flex-1 rounded-full px-1 py-[3px]">
                              <input
                                type="range"
                                min="-1"
                                max="1"
                                step="0.01"
                                disabled={!segment}
                                value={
                                  segment?.keystrokeDelaySec ??
                                  DEFAULT_KEYSTROKE_DELAY_SEC
                                }
                                style={sv(
                                  segment?.keystrokeDelaySec ??
                                    DEFAULT_KEYSTROKE_DELAY_SEC,
                                  -1,
                                  1,
                                )}
                                onChange={(e) =>
                                  handleKeystrokeDelayChange(
                                    Number(e.target.value),
                                  )
                                }
                                className="playback-keystroke-delay-slider block w-full"
                              />
                            </div>
                            <span className="playback-keystroke-delay-value text-[10px] tabular-nums text-[var(--overlay-panel-fg)]/86 w-12 text-right">
                              {(
                                segment?.keystrokeDelaySec ??
                                DEFAULT_KEYSTROKE_DELAY_SEC
                              ).toFixed(2)}
                              s
                            </span>
                          </div>
                          <div className="playback-keystroke-language-row flex items-center gap-3 mt-2">
                            <span className="playback-keystroke-language-label text-[10px] text-[var(--overlay-panel-fg)]/60 shrink-0">
                              {t.keystrokeLanguageLabel}
                            </span>
                            <div className="playback-keystroke-language-toggle ui-segmented ml-auto flex-nowrap rounded-md overflow-hidden">
                              {(
                                ["en", "ko", "vi", "es", "ja", "zh"] as const
                              ).map((lang) => (
                                <button
                                  key={lang}
                                  className={`playback-keystroke-language-btn ui-segmented-button px-2 py-0.5 text-[10px] uppercase ${(segment?.keystrokeLanguage ?? "en") === lang ? "ui-segmented-button-active" : "text-[var(--overlay-panel-fg)]/70"}`}
                                  onClick={() => {
                                    if (!segment) return;
                                    saveKeystrokeLanguage(lang);
                                    setSegment({
                                      ...segment,
                                      keystrokeLanguage: lang,
                                    });
                                  }}
                                  disabled={!segment}
                                >
                                  {lang}
                                </button>
                              ))}
                            </div>
                          </div>
                        </div>
                      </div>
                    }
                    autoZoomButton={
                      <Button
                        onClick={handleAutoZoom}
                        disabled={
                          exportHook.isProcessing ||
                          !currentVideo ||
                          (!mousePositions.length &&
                            !segment?.smoothMotionPath?.length)
                        }
                        className={`auto-zoom-button ui-action-button flex items-center px-2.5 py-1 h-7 text-xs font-medium transition-colors whitespace-nowrap rounded-lg ${
                          !currentVideo ||
                          exportHook.isProcessing ||
                          (!mousePositions.length &&
                            !segment?.smoothMotionPath?.length)
                            ? "ui-toolbar-button text-[var(--on-surface)]/35 cursor-not-allowed"
                            : segment?.smoothMotionPath?.length
                              ? ""
                              : "ui-chip-button text-[var(--on-surface)]"
                        }`}
                        data-tone="primary"
                        data-active={segment?.smoothMotionPath?.length ? "true" : "false"}
                      >
                        <Wand2 className="w-3 h-3 mr-1" />
                        {t.autoZoom}
                      </Button>
                    }
                    smartPointerButton={
                      <Button
                        onClick={handleSmartPointerHiding}
                        disabled={
                          exportHook.isProcessing ||
                          !currentVideo ||
                          (() => {
                            const segs = segment?.cursorVisibilitySegments;
                            const isActive =
                              !!segs?.length &&
                              !(
                                segs.length === 1 &&
                                Math.abs(segs[0].startTime - 0) < 0.01 &&
                                Math.abs(segs[0].endTime - duration) < 0.01
                              );
                            return !mousePositions.length && !isActive;
                          })()
                        }
                        className={`smart-pointer-button ui-action-button flex items-center px-2.5 py-1 h-7 text-xs font-medium transition-colors whitespace-nowrap rounded-lg ${(() => {
                          const segs = segment?.cursorVisibilitySegments;
                          const isActive =
                            !!segs?.length &&
                            !(
                              segs.length === 1 &&
                              Math.abs(segs[0].startTime - 0) < 0.01 &&
                              Math.abs(segs[0].endTime - duration) < 0.01
                            );
                          if (
                            !currentVideo ||
                            exportHook.isProcessing ||
                            (!mousePositions.length && !isActive)
                          )
                            return "ui-toolbar-button text-[var(--on-surface)]/35 cursor-not-allowed";
                          return isActive
                            ? ""
                            : "ui-chip-button text-[var(--on-surface)]";
                        })()}`}
                        data-tone="warning"
                        data-active={(() => {
                          const segs = segment?.cursorVisibilitySegments;
                          const isActive =
                            !!segs?.length &&
                            !(
                              segs.length === 1 &&
                              Math.abs(segs[0].startTime - 0) < 0.01 &&
                              Math.abs(segs[0].endTime - duration) < 0.01
                            );
                          return isActive ? "true" : "false";
                        })()}
                      >
                        <MousePointer2 className="w-3 h-3 mr-1" />
                        {t.smartPointer}
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
                          onChange={(e) =>
                            setBackgroundConfig((prev) => ({
                              ...prev,
                              volume: Number(e.target.value),
                            }))
                          }
                          className="playback-volume-slider w-20"
                        />
                      </div>
                    }
                  />
                )}
                {showPlaybackControlsGhost && (
                  <div
                    className="editor-empty-playback-chrome ui-empty-state flex h-[44px] items-center gap-2 rounded-2xl px-3.5 py-2.5 opacity-65"
                    aria-hidden="true"
                  >
                    <div className="editor-empty-playback-pill h-7 w-[76px] rounded-xl bg-[var(--ui-surface-2)]" />
                    <div className="editor-empty-playback-divider h-5 w-px bg-[var(--ui-border)]" />
                    <div className="editor-empty-playback-icon h-8 w-8 rounded-lg bg-[var(--ui-surface-2)]" />
                    <div className="editor-empty-playback-time h-4 w-[88px] rounded-full bg-[var(--ui-surface-2)]" />
                    <div className="editor-empty-playback-divider h-5 w-px bg-[var(--ui-border)]" />
                    <div className="editor-empty-playback-pill h-7 w-[94px] rounded-xl bg-[var(--ui-surface-2)]" />
                    <div className="editor-empty-playback-pill h-7 w-[92px] rounded-xl bg-[var(--ui-surface-2)]" />
                    <div className="editor-empty-playback-pill h-7 w-[108px] rounded-xl bg-[var(--ui-surface-2)]" />
                  </div>
                )}
              </div>

              {!isCropping && composition && (
                <SequencePillChain
                  composition={composition}
                  activeClipId={activeClipId}
                  spreadFromClipId={spreadFromClipId}
                  onSelectClip={(clipId) => {
                    void handleSelectSequenceClip(clipId);
                  }}
                  onInsertClip={handleOpenInsertProjectPicker}
                  onRemoveClip={(clipId) => {
                    void handleRemoveSequenceClip(clipId);
                  }}
                  onModeChange={(mode) => {
                    void handleSequenceModeChange(mode);
                  }}
                />
              )}
              {showSequencePillGhost && (
                <div
                  className="sequence-focus-breadcrumb sequence-focus-breadcrumb-empty flex items-center justify-center gap-3 px-1 -mt-1 text-[11px] text-[var(--on-surface-variant)]"
                  aria-hidden="true"
                >
                  <div className="sequence-pill-chain sequence-pill-chain-empty flex min-w-0 flex-1 items-center justify-center overflow-x-auto py-2">
                    <div className="flex min-w-max items-center gap-1.5 opacity-65">
                      <div className="sequence-pill-add-btn sequence-pill-add-btn-empty ui-empty-state flex h-7 w-7 items-center justify-center rounded-full" />
                      <div className="sequence-pill sequence-pill-empty ui-empty-state h-8 w-[134px] rounded-full" />
                      <div className="sequence-pill-gap sequence-pill-gap-empty ui-empty-state h-7 w-7 rounded-full" />
                      <div className="sequence-pill sequence-pill-empty ui-empty-state h-8 w-[108px] rounded-full" />
                      <div className="sequence-pill-add-btn sequence-pill-add-btn-empty ui-empty-state flex h-7 w-7 items-center justify-center rounded-full" />
                    </div>
                  </div>
                </div>
              )}
            </div>

            {/* Side Panel */}
            <div className="side-panel-container w-[24rem] flex-shrink-0 min-h-0 relative overflow-visible">
              <SidePanel
                activePanel={activePanel}
                setActivePanel={setActivePanel}
                segment={segment}
                editingKeyframeId={editingKeyframeId}
                zoomFactor={zoomFactor}
                setZoomFactor={setZoomFactor}
                onDeleteKeyframe={handleDeleteKeyframe}
                onUpdateZoom={throttledUpdateZoom}
                backgroundConfig={backgroundConfig}
                setBackgroundConfig={setBackgroundConfig}
                recentUploads={recentUploads}
                onRemoveRecentUpload={handleRemoveRecentUpload}
                onBackgroundUpload={handleBackgroundUpload}
                isBackgroundUploadProcessing={isBackgroundUploadProcessing}
                editingTextId={editingTextId}
                onUpdateSegment={setSegment}
                beginBatch={beginBatch}
                commitBatch={commitBatch}
              />
              {isOverlayMode && (
                <div className="panel-block-overlay absolute inset-0 bg-[var(--surface)] z-50 rounded-xl" />
              )}
            </div>
          </div>

          {/* Timeline */}
          <div
            className={`timeline-container mt-3 flex-shrink-0 relative ${isOverlayMode ? "overflow-hidden" : ""}`}
          >
            <TimelineArea
              duration={duration}
              currentTime={currentTime}
              segment={segment}
              thumbnails={thumbnails}
              timelineRef={timelineRef}
              videoRef={videoRef}
              editingKeyframeId={editingKeyframeId}
              editingTextId={editingTextId}
              editingKeystrokeSegmentId={editingKeystrokeSegmentId}
              setCurrentTime={setCurrentTime}
              setEditingKeyframeId={setEditingKeyframeId}
              setEditingTextId={setEditingTextId}
              setEditingKeystrokeSegmentId={setEditingKeystrokeSegmentId}
              setEditingPointerId={setEditingPointerId}
              setActivePanel={setActivePanel}
              setSegment={setSegment}
              onSeek={seek}
              onSeekEnd={flushSeek}
              onAddText={handleAddText}
              onAddKeystrokeSegment={handleAddKeystrokeSegment}
              onAddPointerSegment={handleAddPointerSegment}
              isPlaying={isPlaying}
              beginBatch={beginBatch}
              commitBatch={commitBatch}
            />
            {isOverlayMode && (
              <div className="timeline-block-overlay absolute inset-0 bg-[var(--surface)] z-50" />
            )}
          </div>
        </main>

        {/* Absolute Projects View covering full screen below header */}
        {projects.showProjectsDialog && (
          <div className="absolute inset-0 top-[44px] z-[90]">
            <ProjectsView
              projects={projects.projects}
              onLoadProject={handleLoadProjectFromGrid}
              onProjectsChange={projects.loadProjects}
              onClose={() => {
                setProjectPickerMode(null);
                projects.setShowProjectsDialog(false);
              }}
              currentProjectId={projects.currentProjectId}
              restoreImage={restoreImageRef.current}
              pickerMode={projectPickerMode ?? "load"}
              onPickProject={handlePickProjectForSequence}
            />
          </div>
        )}

        {isCropping && currentVideo && (
          <div className="crop-workspace-overlay absolute inset-0 top-[44px] z-[120]">
            <CropWorkspace
              show={isCropping}
              videoSrc={currentVideo}
              initialCrop={segment?.crop}
              initialTime={currentTime}
              onCancel={handleCancelCrop}
              onApply={handleApplyCrop}
            />
          </div>
        )}

        {/* Dialogs */}
        <ProcessingOverlay
          show={exportHook.isProcessing}
          exportProgress={0}
          onCancel={exportHook.cancelExport}
        />
        <WindowSelectDialog
          show={showWindowSelect}
          onClose={() => setShowWindowSelect(false)}
          windows={windows}
          onSelectWindow={handleSelectWindowForRecording}
        />
        {currentVideo && !isVideoReady && !projects.showProjectsDialog && (
          <div className="video-loading-overlay absolute inset-0 flex items-center justify-center bg-black/62">
            <div className="loading-message text-[var(--on-surface)]">
              {t.preparingVideoOverlay}
            </div>
          </div>
        )}
        <ExportDialog
          show={exportHook.showExportDialog}
          onClose={() => exportHook.setShowExportDialog(false)}
          onExport={exportHook.startExport}
          exportOptions={exportHook.exportOptions}
          setExportOptions={exportHook.setExportOptions}
          segment={exportHook.dialogSegment}
          videoRef={videoRef}
          backgroundConfig={exportHook.dialogBackgroundConfig}
          hasAudio={exportHook.hasAudio}
          sourceVideoFps={exportHook.sourceVideoFps}
          trimmedDurationSec={exportHook.dialogTrimmedDurationSec}
          clipCount={exportHook.dialogClipCount}
          autoCopyEnabled={exportHook.exportAutoCopyEnabled}
          onToggleAutoCopy={exportHook.setExportAutoCopyEnabled}
        />
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
          artifacts={exportHook.lastExportArtifacts}
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
