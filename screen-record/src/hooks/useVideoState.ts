import { useState, useRef, useEffect, useCallback } from 'react';
import { invoke } from '@/lib/ipc';
import { videoRenderer } from '@/lib/videoRenderer';
import { createVideoController } from '@/lib/videoController';
import { projectManager } from '@/lib/projectManager';
import { thumbnailGenerator } from '@/lib/thumbnailGenerator';
import {
  videoExporter
} from '@/lib/videoExporter';
import { autoZoomGenerator } from '@/lib/autoZoom';
import { BackgroundConfig, VideoSegment, ZoomKeyframe, MousePosition, ExportOptions, Project, TextSegment, CursorVisibilitySegment, RawInputEvent, RecordingMode, CropRect } from '@/types/video';
import { clampVisibilitySegmentsToDuration, generateCursorVisibility, mergePointerSegments } from '@/lib/cursorHiding';
import { normalizeSegmentTrimData } from '@/lib/trimSegments';
import { getKeyframeRange } from '@/utils/helpers';
import { useThrottle } from './useAppHooks';
import { buildKeystrokeEvents } from '@/lib/keystrokeProcessor';
import {
  ensureKeystrokeVisibilitySegments,
  filterKeystrokeEventsByMode,
  generateKeystrokeVisibilitySegments,
  rebuildKeystrokeVisibilitySegmentsForMode
} from '@/lib/keystrokeVisibility';
import { normalizeMousePositionsToVideoSpace } from '@/lib/dynamicCapture';

const DEFAULT_KEYSTROKE_DELAY_SEC = 0;
const KEYSTROKE_DELAY_KEY = 'screen-record-keystroke-delay-v1';
const KEYSTROKE_LANGUAGE_KEY = 'screen-record-keystroke-language-v1';
const KEYSTROKE_MODE_PREF_KEY = 'screen-record-keystroke-mode-pref-v1';
const KEYSTROKE_OVERLAY_PREF_KEY = 'screen-record-keystroke-overlay-pref-v1';
const AUTO_ZOOM_PREF_KEY = 'screen-record-auto-zoom-pref-v1';
const SMART_POINTER_PREF_KEY = 'screen-record-smart-pointer-pref-v1';
const EXPORT_FPS_PREF_KEY = 'screen-record-export-fps-pref-v1';
const CROP_PREF_KEY = 'screen-record-crop-pref-v1';
const DEFAULT_EXPORT_FPS = 60;
const MIN_EXPORT_FPS = 1;
const MAX_EXPORT_FPS = 240;
const MIN_CROP_SIZE = 0.05;

function getSavedKeystrokeDelaySec(): number {
  try {
    const raw = localStorage.getItem(KEYSTROKE_DELAY_KEY);
    if (raw === null) return DEFAULT_KEYSTROKE_DELAY_SEC;
    const n = Number(raw);
    if (!Number.isFinite(n)) return DEFAULT_KEYSTROKE_DELAY_SEC;
    return Math.max(-1, Math.min(1, n));
  } catch {
    return DEFAULT_KEYSTROKE_DELAY_SEC;
  }
}

const VALID_KEYSTROKE_LANGUAGES = ['en', 'ko', 'vi', 'es', 'ja', 'zh'] as const;
type KeystrokeLanguage = typeof VALID_KEYSTROKE_LANGUAGES[number];

export function getSavedKeystrokeLanguage(): KeystrokeLanguage {
  try {
    const raw = localStorage.getItem(KEYSTROKE_LANGUAGE_KEY);
    if (raw && (VALID_KEYSTROKE_LANGUAGES as readonly string[]).includes(raw)) {
      return raw as KeystrokeLanguage;
    }
  } catch { /* ignore */ }
  return 'en';
}

export function saveKeystrokeLanguage(lang: KeystrokeLanguage): void {
  try { localStorage.setItem(KEYSTROKE_LANGUAGE_KEY, lang); } catch { /* ignore */ }
}

function getSavedKeystrokeModePref(): 'off' | 'keyboard' | 'keyboardMouse' {
  try {
    const raw = localStorage.getItem(KEYSTROKE_MODE_PREF_KEY);
    if (raw === 'keyboard' || raw === 'keyboardMouse' || raw === 'off') return raw;
  } catch { /* ignore */ }
  return 'off';
}

function getSavedKeystrokeOverlayPref(): { x: number; y: number; scale: number } {
  try {
    const raw = localStorage.getItem(KEYSTROKE_OVERLAY_PREF_KEY);
    if (raw) {
      const p = JSON.parse(raw) as Partial<{ x: number; y: number; scale: number }>;
      if (typeof p === 'object' && p !== null) {
        return {
          x: typeof p.x === 'number' ? p.x : 50,
          y: typeof p.y === 'number' ? p.y : 100,
          scale: typeof p.scale === 'number' ? p.scale : 1,
        };
      }
    }
  } catch { /* ignore */ }
  return { x: 50, y: 100, scale: 1 };
}

function normalizeCropRect(crop: Partial<CropRect> | null | undefined): CropRect | undefined {
  if (!crop) return undefined;
  const rawX = typeof crop.x === 'number' && Number.isFinite(crop.x) ? crop.x : 0;
  const rawY = typeof crop.y === 'number' && Number.isFinite(crop.y) ? crop.y : 0;
  const rawWidth = typeof crop.width === 'number' && Number.isFinite(crop.width) ? crop.width : 1;
  const rawHeight = typeof crop.height === 'number' && Number.isFinite(crop.height) ? crop.height : 1;

  const width = Math.max(MIN_CROP_SIZE, Math.min(1, rawWidth));
  const height = Math.max(MIN_CROP_SIZE, Math.min(1, rawHeight));
  const x = Math.max(0, Math.min(1 - width, rawX));
  const y = Math.max(0, Math.min(1 - height, rawY));

  if (width >= 0.999 && height >= 0.999 && x <= 0.001 && y <= 0.001) {
    return undefined;
  }
  return { x, y, width, height };
}

export function getSavedCropPref(): CropRect | undefined {
  try {
    const raw = localStorage.getItem(CROP_PREF_KEY);
    if (!raw) return undefined;
    const parsed = JSON.parse(raw) as Partial<CropRect>;
    return normalizeCropRect(parsed);
  } catch {
    return undefined;
  }
}

export function saveCropPref(crop: CropRect | undefined): void {
  try {
    const normalized = normalizeCropRect(crop);
    if (!normalized) {
      localStorage.removeItem(CROP_PREF_KEY);
      return;
    }
    localStorage.setItem(CROP_PREF_KEY, JSON.stringify(normalized));
  } catch {
    // ignore persistence failures
  }
}

function getSavedAutoZoomPref(): boolean {
  try {
    const raw = localStorage.getItem(AUTO_ZOOM_PREF_KEY);
    if (raw !== null) return raw === '1';
  } catch { /* ignore */ }
  return true; // default ON for first-time users
}

function saveAutoZoomPref(enabled: boolean): void {
  try { localStorage.setItem(AUTO_ZOOM_PREF_KEY, enabled ? '1' : '0'); } catch { /* ignore */ }
}

function getSavedSmartPointerPref(): boolean {
  try {
    const raw = localStorage.getItem(SMART_POINTER_PREF_KEY);
    if (raw !== null) return raw === '1';
  } catch { /* ignore */ }
  return true; // default ON for first-time users
}

function saveSmartPointerPref(enabled: boolean): void {
  try { localStorage.setItem(SMART_POINTER_PREF_KEY, enabled ? '1' : '0'); } catch { /* ignore */ }
}

function getSavedExportFpsPref(): number {
  try {
    const raw = localStorage.getItem(EXPORT_FPS_PREF_KEY);
    if (raw === null) return DEFAULT_EXPORT_FPS;
    const parsed = Number(raw);
    if (!Number.isFinite(parsed)) return DEFAULT_EXPORT_FPS;
    const rounded = Math.round(parsed);
    if (rounded < MIN_EXPORT_FPS || rounded > MAX_EXPORT_FPS) {
      return DEFAULT_EXPORT_FPS;
    }
    return rounded;
  } catch {
    return DEFAULT_EXPORT_FPS;
  }
}

// ============================================================================
// useVideoPlayback
// ============================================================================
interface UseVideoPlaybackProps {
  segment: VideoSegment | null;
  backgroundConfig: BackgroundConfig;
  mousePositionsRef: { current: MousePosition[] };
  isCropping: boolean;
}

export function useVideoPlayback({
  segment,
  backgroundConfig,
  mousePositionsRef,
  isCropping
}: UseVideoPlaybackProps) {
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [isVideoReady, setIsVideoReady] = useState(false);
  const [thumbnails, setThumbnails] = useState<string[]>([]);
  const [currentVideo, setCurrentVideo] = useState<string | null>(null);
  const [currentAudio, setCurrentAudio] = useState<string | null>(null);

  const videoRef = useRef<HTMLVideoElement | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const tempCanvasRef = useRef<HTMLCanvasElement>(document.createElement('canvas'));
  const videoControllerRef = useRef<ReturnType<typeof createVideoController>>();

  // Initialize controller
  useEffect(() => {
    if (!videoRef.current || !canvasRef.current) return;

    videoControllerRef.current = createVideoController({
      videoRef: videoRef.current,
      audioRef: audioRef.current || undefined,
      canvasRef: canvasRef.current,
      tempCanvasRef: tempCanvasRef.current,
      onTimeUpdate: setCurrentTime,
      onPlayingChange: setIsPlaying,
      onVideoReady: setIsVideoReady,
      onDurationChange: setDuration,
      onError: console.error,
      onMetadataLoaded: (metadata) => {
        // Segment update handled in App.tsx via useUndoRedo
        console.log('[useVideoPlayback] Metadata loaded:', metadata.duration);
      }
    });

    return () => { videoControllerRef.current?.destroy(); };
  }, []);

  const renderFrame = useCallback(() => {
    if (!segment || !videoRef.current || !canvasRef.current) return;
    if (!videoRef.current.paused) return;

    const renderSegment = isCropping ? {
      ...segment,
      crop: undefined,
      zoomKeyframes: segment.zoomKeyframes.map(k => ({ ...k, zoomFactor: 1.0, positionX: 0.5, positionY: 0.5 }))
    } : segment;

    const renderBackground = isCropping ? {
      ...backgroundConfig, scale: 100, borderRadius: 0, shadow: 0,
      backgroundType: 'solid' as const, customBackground: undefined, cropBottom: 0, canvasMode: 'auto' as const
    } : backgroundConfig;

    videoRenderer.drawFrame({
      video: videoRef.current, canvas: canvasRef.current, tempCanvas: tempCanvasRef.current,
      segment: renderSegment, backgroundConfig: renderBackground, mousePositions: mousePositionsRef.current,
      currentTime: videoRef.current.currentTime
    });
  }, [segment, backgroundConfig, isCropping]);

  const togglePlayPause = useCallback(() => {
    videoControllerRef.current?.togglePlayPause();
  }, []);

  const seek = useCallback((time: number) => {
    videoControllerRef.current?.seek(time);
  }, []);

  const flushSeek = useCallback(() => {
    videoControllerRef.current?.flushPendingSeek();
  }, []);

  const generateThumbnail = useCallback((): string | undefined => {
    if (!canvasRef.current) return undefined;
    try { return canvasRef.current.toDataURL('image/jpeg', 0.5); } catch { return undefined; }
  }, []);

  const generateThumbnails = useCallback(async (filePathOverride?: string) => {
    if (!currentVideo || !segment) return;
    const newThumbnails = await thumbnailGenerator.generateThumbnails(currentVideo, 20, {
      trimStart: segment.trimStart,
      trimEnd: segment.trimEnd,
      filePath: filePathOverride
    });
    setThumbnails(newThumbnails);
  }, [currentVideo, segment]);

  // Volume sync
  useEffect(() => {
    if (videoControllerRef.current && backgroundConfig.volume !== undefined) {
      videoControllerRef.current.setVolume(backgroundConfig.volume);
    }
  }, [backgroundConfig.volume]);

  // Render options sync — apply isCropping overrides so the controller always
  // renders the correct view (e.g. after seeked events, thumbnail generation).
  useEffect(() => {
    if (!segment || !videoControllerRef.current) return;

    const renderSegment = isCropping ? {
      ...segment, crop: undefined,
      zoomKeyframes: segment.zoomKeyframes.map(k => ({ ...k, zoomFactor: 1.0, positionX: 0.5, positionY: 0.5 }))
    } : segment;

    const renderBackground = isCropping ? {
      ...backgroundConfig, scale: 100, borderRadius: 0, shadow: 0,
      backgroundType: 'solid' as const, customBackground: undefined, cropBottom: 0, canvasMode: 'auto' as const
    } : backgroundConfig;

    videoControllerRef.current.updateRenderOptions({
      segment: renderSegment, backgroundConfig: renderBackground, mousePositions: mousePositionsRef.current
    });
  }, [segment, backgroundConfig, isCropping]);

  // Render context sync — update the running animation loop's context when
  // segment/backgroundConfig/isCropping change, WITHOUT restarting the loop.
  // VideoController.handlePlay owns startAnimation; the loop self-exits on pause.
  // This eliminates the stop→start thrashing that caused audio play/pause AbortErrors.
  useEffect(() => {
    const video = videoRef.current;
    if (!video || !segment) return;

    const loopSegment = isCropping ? {
      ...segment, crop: undefined,
      zoomKeyframes: segment.zoomKeyframes.map(k => ({ ...k, zoomFactor: 1.0, positionX: 0.5, positionY: 0.5 }))
    } : segment;

    const loopBackground = isCropping ? {
      ...backgroundConfig, scale: 100, borderRadius: 0, shadow: 0,
      backgroundType: 'solid' as const, customBackground: undefined, cropBottom: 0, canvasMode: 'auto' as const
    } : backgroundConfig;

    // Update context for the animation loop (picked up on next RAF tick)
    videoRenderer.updateRenderContext({
      video, canvas: canvasRef.current!, tempCanvas: tempCanvasRef.current,
      segment: loopSegment, backgroundConfig: loopBackground,
      mousePositions: mousePositionsRef.current,
      currentTime: video.currentTime
    });

    if (video.paused) {
      renderFrame();
    }
  }, [segment, backgroundConfig, isCropping]);

  // Generate thumbnails when ready
  useEffect(() => {
    if (isVideoReady && duration > 0 && thumbnails.length === 0) generateThumbnails();
  }, [isVideoReady, duration, thumbnails.length, generateThumbnails]);

  // Cleanup URLs
  useEffect(() => {
    return () => { if (currentVideo?.startsWith('blob:')) URL.revokeObjectURL(currentVideo); };
  }, [currentVideo]);

  useEffect(() => {
    return () => { if (currentAudio?.startsWith('blob:')) URL.revokeObjectURL(currentAudio); };
  }, [currentAudio]);

  return {
    currentTime, setCurrentTime, duration, setDuration, isPlaying, isVideoReady, setIsVideoReady,
    thumbnails, setThumbnails, currentVideo, setCurrentVideo, currentAudio, setCurrentAudio,
    videoRef, audioRef, canvasRef, tempCanvasRef, videoControllerRef,
    renderFrame, togglePlayPause, seek, flushSeek, generateThumbnail, generateThumbnails
  };
}

// ============================================================================
// useRecording
// ============================================================================
interface UseRecordingProps {
  videoControllerRef: React.MutableRefObject<ReturnType<typeof createVideoController> | undefined>;
  videoRef: React.RefObject<HTMLVideoElement | null>;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  tempCanvasRef: React.RefObject<HTMLCanvasElement>;
  backgroundConfig: BackgroundConfig;
  setSegment: (segment: VideoSegment | null) => void;
  setCurrentVideo: (url: string | null) => void;
  setCurrentAudio: (url: string | null) => void;
  setIsVideoReady: (ready: boolean) => void;
  setThumbnails: (thumbnails: string[]) => void;
  setDuration: (duration: number) => void;
  setCurrentTime: (time: number) => void;
  generateThumbnails: (filePathOverride?: string) => void;
  generateThumbnail: () => string | undefined;
  renderFrame: () => void;
  currentVideo: string | null;
  currentAudio: string | null;
}

export function useRecording(props: UseRecordingProps) {
  const [isRecording, setIsRecording] = useState(false);
  const [activeRecordingMode, setActiveRecordingMode] = useState<RecordingMode>('withoutCursor');
  const [recordingDuration, setRecordingDuration] = useState(0);
  const [isLoadingVideo, setIsLoadingVideo] = useState(false);
  const [loadingProgress, setLoadingProgress] = useState(0);
  const [mousePositions, setMousePositions] = useState<MousePosition[]>([]);
  const [audioFilePath, setAudioFilePath] = useState("");
  const [videoFilePath, setVideoFilePath] = useState("");
  const [videoFilePathOwnerUrl, setVideoFilePathOwnerUrl] = useState("");
  const [error, setError] = useState<string | null>(null);

  const startNewRecording = async (
    targetId: string,
    recordingMode: RecordingMode,
    targetType: 'monitor' | 'window' = 'monitor',
    targetFps?: number
  ) => {
    try {
      setAudioFilePath("");
      setVideoFilePath("");
      setVideoFilePathOwnerUrl("");
      setMousePositions([]);

      if (props.currentVideo) {
        // User is editing a video — don't touch the preview at all. The canvas,
        // segment, playback state, and video URL all stay intact so editing can
        // continue uninterrupted. Old URLs are revoked in handleStopRecording
        // once the new video is ready to replace them.
      } else {
        // No existing video — safe to clear everything for a clean slate.
        props.setIsVideoReady(false);
        props.setCurrentTime(0);
        props.setDuration(0);
        props.setSegment(null);
        props.setThumbnails([]);
        if (props.videoRef.current) {
          props.videoRef.current.pause();
          props.videoRef.current.removeAttribute('src');
          props.videoRef.current.load();
          props.videoRef.current.currentTime = 0;
        }
        const canvas = props.canvasRef.current;
        if (canvas) {
          const ctx = canvas.getContext('2d');
          if (ctx) ctx.clearRect(0, 0, canvas.width, canvas.height);
        }
      }

      await invoke("start_recording", {
        targetId,
        targetType,
        includeCursor: recordingMode === 'withCursor',
        fps: targetFps ?? null
      });
      setActiveRecordingMode(recordingMode);
      setIsRecording(true);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleStopRecording = async (): Promise<{ mouseData: MousePosition[], initialSegment: VideoSegment, videoUrl: string, recordingMode: RecordingMode, rawVideoPath: string, capturedFps: number | null } | null> => {
    if (!isRecording) return null;

    let objectUrl: string | undefined;
    let audioObjectUrl: string | undefined;

    try {
      setIsRecording(false);
      setIsLoadingVideo(true);
      props.setIsVideoReady(false);
      props.setSegment(null);
      props.setCurrentTime(0);
      props.setDuration(0);
      setLoadingProgress(0);
      props.setThumbnails([]);

      const [videoUrl, audioUrl, rawMouseData, audioPath, videoPath, rawInputEvents, rawCapturedFps] =
        await invoke<[string, string, any[], string, string, RawInputEvent[], number | null]>("stop_recording");
      const capturedFps = typeof rawCapturedFps === 'number' && rawCapturedFps > 0 ? rawCapturedFps : null;
      setAudioFilePath(audioPath);
      setVideoFilePath(videoPath || "");

      const mouseData: MousePosition[] = rawMouseData.map(p => ({
        x: p.x, y: p.y, timestamp: p.timestamp,
        isClicked: p.isClicked !== undefined ? p.isClicked : p.is_clicked,
        cursor_type: p.cursor_type || 'default',
        captureWidth: p.captureWidth ?? p.capture_width,
        captureHeight: p.captureHeight ?? p.capture_height,
      }));
      setMousePositions(mouseData);

      objectUrl = await props.videoControllerRef.current?.loadVideo({
        videoUrl, onLoadingProgress: setLoadingProgress
      });

      if (objectUrl) {
        // Revoke the old video/audio URLs that were preserved during recording.
        if (props.currentVideo && props.currentVideo !== objectUrl) URL.revokeObjectURL(props.currentVideo);
        if (props.currentAudio) URL.revokeObjectURL(props.currentAudio);

        props.setCurrentVideo(objectUrl);
        setVideoFilePathOwnerUrl(objectUrl);

        if (audioUrl && audioUrl !== videoUrl) {
          audioObjectUrl = await props.videoControllerRef.current?.loadAudio({ audioUrl });
          if (audioObjectUrl) props.setCurrentAudio(audioObjectUrl);
        } else {
          props.setCurrentAudio(null);
        }

        props.setIsVideoReady(true);
        // Restore the SR window so the user can review the new recording.
        invoke('restore_window').catch(() => {});
        props.generateThumbnails(videoPath || undefined);

        const videoDuration = props.videoRef.current?.duration || 0;
        const maxMouseTimestamp = rawMouseData.reduce((max, entry) => {
          const ts = typeof entry?.timestamp === 'number' ? entry.timestamp : 0;
          return Math.max(max, ts);
        }, 0);
        const maxInputTimestamp = (rawInputEvents || []).reduce((max: number, entry: any) => {
          const ts = typeof entry?.timestamp === 'number' ? entry.timestamp : 0;
          return Math.max(max, ts);
        }, 0);
        const timelineDuration = videoDuration > 0
          ? videoDuration
          : Math.max(maxMouseTimestamp, maxInputTimestamp);
        const baseSegment: VideoSegment = {
          trimStart: 0,
          trimEnd: timelineDuration,
          trimSegments: [{
            id: crypto.randomUUID(),
            startTime: 0,
            endTime: timelineDuration,
          }],
          zoomKeyframes: [],
          textSegments: [],
          speedPoints: [{ time: 0, speed: 1 }, { time: timelineDuration, speed: 1 }],
        };

        const keystrokeEvents = buildKeystrokeEvents(rawInputEvents || [], timelineDuration);
        const segmentWithKeystrokes: VideoSegment = { ...baseSegment, keystrokeEvents };

        const vidW = props.videoRef.current?.videoWidth || 0;
        const vidH = props.videoRef.current?.videoHeight || 0;
        const normalizedMouseData = (vidW > 0 && vidH > 0)
          ? normalizeMousePositionsToVideoSpace(mouseData, vidW, vidH)
          : mouseData;
        const normalizedPointerSegments = generateCursorVisibility(
          segmentWithKeystrokes,
          normalizedMouseData,
          timelineDuration
        );
        const initialAutoPath = (vidW > 0 && vidH > 0 && normalizedMouseData.length > 0)
          ? autoZoomGenerator.generateMotionPath(baseSegment, normalizedMouseData, vidW, vidH)
          : [];

        const savedKeystrokeDelay = getSavedKeystrokeDelaySec();
        const keyboardVisibilitySegments = generateKeystrokeVisibilitySegments(
          filterKeystrokeEventsByMode(keystrokeEvents, 'keyboard'),
          timelineDuration,
          { mode: 'keyboard', delaySec: savedKeystrokeDelay }
        );
        const keyboardMouseVisibilitySegments = generateKeystrokeVisibilitySegments(
          filterKeystrokeEventsByMode(keystrokeEvents, 'keyboardMouse'),
          timelineDuration,
          { mode: 'keyboardMouse', delaySec: savedKeystrokeDelay }
        );

        const wantAutoZoom = getSavedAutoZoomPref();
        const wantSmartPointer = getSavedSmartPointerPref();
        const defaultPointerSeg: CursorVisibilitySegment[] = [{ id: crypto.randomUUID(), startTime: 0, endTime: timelineDuration }];

        const initialSegment: VideoSegment = {
          ...baseSegment,
          crop: getSavedCropPref(),
          cursorVisibilitySegments: wantSmartPointer ? normalizedPointerSegments : defaultPointerSeg,
          smoothMotionPath: wantAutoZoom ? initialAutoPath : [],
          zoomInfluencePoints: wantAutoZoom && initialAutoPath.length > 0
            ? [{ time: 0, value: 1.0 }, { time: timelineDuration, value: 1.0 }]
            : [],
          keystrokeMode: getSavedKeystrokeModePref(),
          keystrokeDelaySec: savedKeystrokeDelay,
          keystrokeLanguage: getSavedKeystrokeLanguage(),
          keystrokeEvents,
          keyboardVisibilitySegments,
          keyboardMouseVisibilitySegments,
          keystrokeOverlay: getSavedKeystrokeOverlayPref(),
          useCustomCursor: activeRecordingMode !== 'withCursor',
        };
        props.setSegment(initialSegment);

        if (props.videoRef.current && props.canvasRef.current && props.videoRef.current.readyState >= 2) {
          videoRenderer.drawFrame({
            video: props.videoRef.current, canvas: props.canvasRef.current,
            tempCanvas: props.tempCanvasRef.current!, segment: initialSegment,
            backgroundConfig: props.backgroundConfig, mousePositions: mouseData, currentTime: 0
          });
        }

        return {
          mouseData,
          initialSegment,
          videoUrl: objectUrl,
          recordingMode: activeRecordingMode,
          rawVideoPath: videoPath || "",
          capturedFps
        };
      }
      return null;
    } catch (err) {
      if (objectUrl) URL.revokeObjectURL(objectUrl);
      if (audioObjectUrl) URL.revokeObjectURL(audioObjectUrl);
      setError(err instanceof Error ? err.message : String(err));
      return null;
    } finally {
      setIsLoadingVideo(false);
      setLoadingProgress(0);
    }
  };

  // Recording duration timer
  useEffect(() => {
    let interval: number;
    if (isRecording) {
      const startTime = Date.now();
      interval = window.setInterval(() => {
        setRecordingDuration(Math.floor((Date.now() - startTime) / 1000));
      }, 1000);
    } else {
      setRecordingDuration(0);
    }
    return () => { if (interval) clearInterval(interval); };
  }, [isRecording]);

  return {
    isRecording, recordingDuration, isLoadingVideo, loadingProgress,
    mousePositions, setMousePositions, audioFilePath, videoFilePath, videoFilePathOwnerUrl, error, setError,
    startNewRecording, handleStopRecording
  };
}

// ============================================================================
// useProjects
// ============================================================================
interface UseProjectsProps {
  videoControllerRef: React.MutableRefObject<ReturnType<typeof createVideoController> | undefined>;
  setCurrentVideo: (url: string | null) => void;
  setCurrentAudio: (url: string | null) => void;
  setSegment: (segment: VideoSegment | null) => void;
  setBackgroundConfig: React.Dispatch<React.SetStateAction<BackgroundConfig>>;
  setMousePositions: (positions: MousePosition[]) => void;
  setThumbnails: (thumbnails: string[]) => void;
  setCurrentRecordingMode?: (mode: RecordingMode) => void;
  setCurrentRawVideoPath?: (path: string) => void;
  currentVideo: string | null;
  currentAudio: string | null;
}

export function useProjects(props: UseProjectsProps) {
  const [projects, setProjects] = useState<Omit<Project, 'videoBlob'>[]>([]);
  const [showProjectsDialog, setShowProjectsDialog] = useState(false);
  const [currentProjectId, setCurrentProjectId] = useState<string | null>(null);
  const logProjectLoad = (event: string, data?: Record<string, unknown>) => {
    const ts = new Date().toISOString();
    console.log(`[ProjectLoad][${ts}] ${event}`, data || {});
  };

  const loadProjects = useCallback(async () => {
    const projects = await projectManager.getProjects();
    setProjects(projects);
  }, []);

  const handleLoadProject = useCallback(async (projectId: string) => {
    logProjectLoad('load:start', { projectId });
    const project = await projectManager.loadProject(projectId);
    if (!project) {
      logProjectLoad('load:missing', { projectId });
      return;
    }
    logProjectLoad('load:fetched', {
      projectId,
      canvasMode: project.backgroundConfig?.canvasMode,
      canvasWidth: project.backgroundConfig?.canvasWidth,
      canvasHeight: project.backgroundConfig?.canvasHeight
    });

    if (props.currentVideo) URL.revokeObjectURL(props.currentVideo);
    if (props.currentAudio) URL.revokeObjectURL(props.currentAudio);

    props.setThumbnails([]);
    props.setCurrentAudio(null);

    // Restore rawVideoPath for old projects that only have a blob.
    // Writes the blob to disk via the media server POST endpoint (binary, no JSON overhead).
    let rawVideoPath = project.rawVideoPath ?? '';
    if (!rawVideoPath && project.videoBlob && project.videoBlob.size > 0) {
      try {
        const port = await invoke<number>('get_media_server_port');
        if (port) {
          const resp = await fetch(`http://localhost:${port}/write-temp`, {
            method: 'POST',
            body: project.videoBlob,
          });
          if (resp.ok) {
            const { path } = await resp.json() as { path: string };
            if (path) {
              rawVideoPath = path;
              // Persist so this migration only happens once.
              await projectManager.updateProject(projectId, { ...project, rawVideoPath: path });
            }
          }
        }
      } catch (e) {
        console.error('[ProjectLoad] Failed to restore rawVideoPath:', e);
      }
    }

    const videoObjectUrl = await props.videoControllerRef.current?.loadVideo({ videoBlob: project.videoBlob });
    if (videoObjectUrl) {
      props.setCurrentVideo(videoObjectUrl);
      if (rawVideoPath) {
        thumbnailGenerator.generateThumbnails(videoObjectUrl, 20, {
          trimStart: project.segment.trimStart,
          trimEnd: project.segment.trimEnd,
          filePath: rawVideoPath
        }).then(props.setThumbnails).catch(() => {});
      }
    }

    if (project.audioBlob) {
      const audioObjectUrl = await props.videoControllerRef.current?.loadAudio({ audioBlob: project.audioBlob });
      if (audioObjectUrl) props.setCurrentAudio(audioObjectUrl);
    }

    const videoDuration = props.videoControllerRef.current?.duration || 0;
    let correctedSegment = { ...project.segment };
    const hasExplicitPointerSegments = Array.isArray(correctedSegment.cursorVisibilitySegments);
    if (correctedSegment.trimEnd === 0 || correctedSegment.trimEnd > videoDuration) {
      correctedSegment.trimEnd = videoDuration;
    }
    correctedSegment = normalizeSegmentTrimData(correctedSegment, videoDuration);
    if (typeof correctedSegment.useCustomCursor !== 'boolean') {
      correctedSegment.useCustomCursor = project.recordingMode === 'withCursor' ? false : true;
    }
    correctedSegment.crop = normalizeCropRect(correctedSegment.crop);
    correctedSegment.cursorVisibilitySegments = clampVisibilitySegmentsToDuration(
      correctedSegment.cursorVisibilitySegments,
      videoDuration
    );
    correctedSegment.keyboardVisibilitySegments = clampVisibilitySegmentsToDuration(
      correctedSegment.keyboardVisibilitySegments,
      videoDuration
    );
    correctedSegment.keyboardMouseVisibilitySegments = clampVisibilitySegmentsToDuration(
      correctedSegment.keyboardMouseVisibilitySegments,
      videoDuration
    );
    // Materialize pointer segments for backward-compat (old projects have undefined)
    if (!hasExplicitPointerSegments) {
      correctedSegment.cursorVisibilitySegments = [{
        id: crypto.randomUUID(),
        startTime: 0,
        endTime: videoDuration,
      }];
    }
    if (!correctedSegment.speedPoints || correctedSegment.speedPoints.length === 0) {
      correctedSegment.speedPoints = [{ time: 0, speed: 1 }, { time: videoDuration, speed: 1 }];
    }
    if (!correctedSegment.keystrokeMode) {
      correctedSegment.keystrokeMode = 'off';
    }
    if (!Array.isArray(correctedSegment.keystrokeEvents)) {
      correctedSegment.keystrokeEvents = [];
    }
    if (typeof correctedSegment.keystrokeDelaySec !== 'number' || Number.isNaN(correctedSegment.keystrokeDelaySec)) {
      correctedSegment.keystrokeDelaySec = DEFAULT_KEYSTROKE_DELAY_SEC;
    } else {
      correctedSegment.keystrokeDelaySec = Math.max(-1, Math.min(1, correctedSegment.keystrokeDelaySec));
    }
    const overlay = correctedSegment.keystrokeOverlay;
    correctedSegment.keystrokeOverlay = {
      x: typeof overlay?.x === 'number' ? Math.max(0, Math.min(100, overlay.x)) : 50,
      y: typeof overlay?.y === 'number' ? Math.max(0, Math.min(100, overlay.y)) : 100,
      scale: typeof overlay?.scale === 'number' && Number.isFinite(overlay.scale)
        ? Math.max(0.45, Math.min(2.4, overlay.scale))
        : 1,
    };
    correctedSegment = ensureKeystrokeVisibilitySegments(correctedSegment, videoDuration);
    const loadedMode = correctedSegment.keystrokeMode ?? 'off';
    if (loadedMode === 'keyboard' || loadedMode === 'keyboardMouse') {
      const modeEvents = filterKeystrokeEventsByMode(correctedSegment.keystrokeEvents ?? [], loadedMode);
      const modeSegments = loadedMode === 'keyboard'
        ? (correctedSegment.keyboardVisibilitySegments ?? [])
        : (correctedSegment.keyboardMouseVisibilitySegments ?? []);
      if (modeSegments.length === 0 && modeEvents.length > 0) {
        correctedSegment = rebuildKeystrokeVisibilitySegmentsForMode(
          correctedSegment,
          loadedMode,
          videoDuration
        );
      }
    }

    // Draw the first frame on the canvas immediately (before React state updates)
    // so the canvas has content when the projects overlay fades out.
    props.videoControllerRef.current?.renderImmediate({
      segment: correctedSegment,
      backgroundConfig: project.backgroundConfig,
      mousePositions: project.mousePositions
    });

    props.setSegment(correctedSegment);
    props.setBackgroundConfig(project.backgroundConfig);
    props.setMousePositions(project.mousePositions);
    props.setCurrentRecordingMode?.(project.recordingMode ?? 'withoutCursor');
    props.setCurrentRawVideoPath?.(rawVideoPath);
    logProjectLoad('load:applied', {
      projectId,
      canvasMode: project.backgroundConfig?.canvasMode,
      canvasWidth: project.backgroundConfig?.canvasWidth,
      canvasHeight: project.backgroundConfig?.canvasHeight
    });

    if (props.videoControllerRef.current && project.backgroundConfig.volume !== undefined) {
      props.videoControllerRef.current.setVolume(project.backgroundConfig.volume);
    }

    setShowProjectsDialog(false);
    setCurrentProjectId(projectId);

    // Ensure keyboard focus returns to the document after the Projects overlay
    // animates out (clone removal can leave focus in limbo → spacebar ignored).
    requestAnimationFrame(() => document.body.focus());
  }, [props]);

  useEffect(() => { loadProjects(); }, [loadProjects]);

  return {
    projects, showProjectsDialog, setShowProjectsDialog,
    currentProjectId, setCurrentProjectId, loadProjects, handleLoadProject
  };
}

// ============================================================================
// useExport
// ============================================================================
interface UseExportProps {
  videoRef: React.RefObject<HTMLVideoElement | null>;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  tempCanvasRef: React.RefObject<HTMLCanvasElement>;
  audioRef: React.RefObject<HTMLAudioElement | null>;
  segment: VideoSegment | null;
  backgroundConfig: BackgroundConfig;
  mousePositions: MousePosition[];
  audioFilePath: string;
  videoFilePath: string;
  videoFilePathOwnerUrl: string;
  rawVideoPath: string;
  savedRawVideoPath: string;
  currentVideo: string | null;
  /** Actual FPS the most-recent recording was encoded at (from backend). Overrides probe. */
  lastCaptureFps: number | null;
}

interface NativeVideoMetadataProbe {
  width: number;
  height: number;
  fps: number;
  fpsNum: number;
  fpsDen: number;
}

export function useExport(props: UseExportProps) {
  const [isProcessing, setIsProcessing] = useState(false);
  const [exportProgress, setExportProgress] = useState(0);
  const [showExportDialog, setShowExportDialog] = useState(false);
  const [showExportSuccessDialog, setShowExportSuccessDialog] = useState(false);
  const [lastExportedPath, setLastExportedPath] = useState('');
  const [sourceVideoFps, setSourceVideoFps] = useState<number | null>(null);
  const [exportAutoCopyEnabled, setExportAutoCopyEnabled] = useState(() => {
    try { return localStorage.getItem('screen-record-export-auto-copy-v1') === '1'; } catch { return false; }
  });
  const [exportOptions, setExportOptions] = useState<ExportOptions>(() => {
    return {
      width: 0,
      height: 0,
      // Preserve user-selected source-matched FPS (for example 50fps recordings),
      // instead of restricting to a fixed preset list.
      fps: getSavedExportFpsPref(),
      targetVideoBitrateKbps: 0,
      speed: 1,
      exportProfile: 'turbo_nv',
      preferNvTurbo: true,
      qualityGatePercent: 3,
      turboCodec: 'hevc',
      preRenderPolicy: 'aggressive',
      outputDir: '',
      format: 'mp4'
    };
  });
  const [hasCheckedExportCapabilities, setHasCheckedExportCapabilities] = useState(false);

  useEffect(() => {
    try { localStorage.setItem('screen-record-export-auto-copy-v1', exportAutoCopyEnabled ? '1' : '0'); } catch {}
  }, [exportAutoCopyEnabled]);

  const handleExport = useCallback(() => setShowExportDialog(true), []);

  const resolveSourceVideoPath = useCallback((): string => {
    const directRecordingPath = props.currentVideo === props.videoFilePathOwnerUrl
      ? props.videoFilePath
      : '';
    return (
      directRecordingPath ||
      props.rawVideoPath ||
      props.savedRawVideoPath ||
      ''
    ).trim();
  }, [
    props.currentVideo,
    props.videoFilePathOwnerUrl,
    props.videoFilePath,
    props.rawVideoPath,
    props.savedRawVideoPath
  ]);

  useEffect(() => {
    if (!showExportDialog) return;

    const sourceVideoPath = resolveSourceVideoPath();
    if (!sourceVideoPath) {
      setSourceVideoFps(null);
      return;
    }

    let cancelled = false;
    void invoke<Partial<NativeVideoMetadataProbe>>('probe_video_metadata', { path: sourceVideoPath })
      .then((metadata) => {
        if (cancelled) return;
        const probedFps = typeof metadata?.fps === 'number' && Number.isFinite(metadata.fps) && metadata.fps > 0
          ? metadata.fps
          : null;
        // Prefer the authoritative capture FPS from the backend over the container
        // metadata probe — WinRT encoder may write 60fps headers even for 100fps captures.
        setSourceVideoFps(props.lastCaptureFps ?? probedFps);
      })
      .catch((error) => {
        if (cancelled) return;
        console.warn('[Export] Source video metadata probe failed:', error);
        setSourceVideoFps(props.lastCaptureFps ?? null);
      });

    return () => {
      cancelled = true;
    };
  }, [showExportDialog, resolveSourceVideoPath, props.lastCaptureFps]);

  useEffect(() => {
    let cancelled = false;
    void videoExporter.getExportCapabilities()
      .then((caps) => {
        if (cancelled) return;
        setExportOptions((prev) => {
          if (!caps.nvencAvailable) {
            if (prev.exportProfile === 'turbo_nv' || prev.preferNvTurbo) {
              return {
                ...prev,
                exportProfile: 'max_speed',
                preferNvTurbo: false,
                turboCodec: 'h264'
              };
            }
            return prev;
          }
          if (caps.nvencAvailable && prev.exportProfile !== 'turbo_nv') {
            return {
              ...prev,
              exportProfile: 'turbo_nv',
              preferNvTurbo: true,
              turboCodec: caps.hevcNvencAvailable ? 'hevc' : 'h264'
            };
          }
          if (!caps.hevcNvencAvailable && prev.turboCodec === 'hevc') {
            return {
              ...prev,
              turboCodec: 'h264'
            };
          }
          return prev;
        });
        setHasCheckedExportCapabilities(true);
      })
      .catch((error) => {
        if (cancelled) return;
        console.warn('[Export] capability probe failed, using safe defaults:', error);
        setExportOptions((prev) => {
          if (prev.exportProfile !== 'turbo_nv') return prev;
          return {
            ...prev,
            exportProfile: 'max_speed',
            preferNvTurbo: false,
            turboCodec: 'h264'
          };
        });
        setHasCheckedExportCapabilities(true);
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (isProcessing || showExportDialog || !hasCheckedExportCapabilities) return;
    const videoEl = props.videoRef.current;
    const canvasEl = props.canvasRef.current;
    const segment = props.segment;
    if (!props.currentVideo || !segment || !videoEl || !canvasEl) return;

    const sourceVideoPath = resolveSourceVideoPath();
    let cancelled = false;
    const runPrime = () => {
      if (cancelled) return;
      void videoExporter.primeExportPreparation({
        width: exportOptions.width,
        height: exportOptions.height,
        fps: exportOptions.fps,
        targetVideoBitrateKbps: exportOptions.targetVideoBitrateKbps,
        speed: exportOptions.speed,
        exportProfile: exportOptions.exportProfile || 'turbo_nv',
        preferNvTurbo: exportOptions.preferNvTurbo ?? true,
        qualityGatePercent: exportOptions.qualityGatePercent ?? 3,
        turboCodec: exportOptions.turboCodec || 'hevc',
        preRenderPolicy: exportOptions.preRenderPolicy || 'idle_only',
        outputDir: exportOptions.outputDir || '',
        video: videoEl,
        canvas: canvasEl,
        tempCanvas: props.tempCanvasRef.current!,
        segment,
        backgroundConfig: props.backgroundConfig,
        mousePositions: props.mousePositions,
        audio: props.audioRef.current || undefined,
        audioFilePath: props.audioFilePath || sourceVideoPath,
        videoFilePath: sourceVideoPath
      }).catch(() => {
        // keep background prewarm silent
      });
    };

    const preRenderPolicy = exportOptions.preRenderPolicy || 'idle_only';
    if (preRenderPolicy === 'off') {
      return () => {
        cancelled = true;
      };
    }

    let idleId = 0;
    const idleApi = (window as Window & {
      requestIdleCallback?: (cb: () => void, options?: { timeout: number }) => number;
      cancelIdleCallback?: (id: number) => void;
    });
    if (preRenderPolicy === 'aggressive') {
      idleId = window.setTimeout(runPrime, 80);
    } else if (typeof idleApi.requestIdleCallback === 'function') {
      idleId = idleApi.requestIdleCallback(runPrime, { timeout: 1500 });
    } else {
      idleId = window.setTimeout(runPrime, 700);
    }

    return () => {
      cancelled = true;
      if (typeof idleApi.cancelIdleCallback === 'function') {
        idleApi.cancelIdleCallback(idleId);
      } else {
        window.clearTimeout(idleId);
      }
    };
  }, [
    isProcessing,
    showExportDialog,
    hasCheckedExportCapabilities,
    props.currentVideo,
    props.segment,
    props.videoRef,
    props.canvasRef,
    props.tempCanvasRef,
    props.backgroundConfig,
    props.mousePositions,
    props.audioRef,
    props.audioFilePath,
    resolveSourceVideoPath,
    exportOptions.width,
    exportOptions.height,
    exportOptions.fps,
    exportOptions.targetVideoBitrateKbps,
    exportOptions.speed,
    exportOptions.exportProfile,
    exportOptions.preferNvTurbo,
    exportOptions.qualityGatePercent,
    exportOptions.turboCodec,
    exportOptions.preRenderPolicy,
    exportOptions.outputDir
  ]);

  useEffect(() => {
    if (isProcessing || !showExportDialog || !hasCheckedExportCapabilities) return;
    const preRenderPolicy = exportOptions.preRenderPolicy || 'idle_only';
    if (preRenderPolicy === 'off') return;
    const videoEl = props.videoRef.current;
    const canvasEl = props.canvasRef.current;
    const segment = props.segment;
    if (!props.currentVideo || !segment || !videoEl || !canvasEl) return;

    const sourceVideoPath = resolveSourceVideoPath();
    const primeDelayMs = preRenderPolicy === 'aggressive' ? 32 : 220;
    const timer = window.setTimeout(() => {
      void videoExporter.primeExportPreparation({
        width: exportOptions.width,
        height: exportOptions.height,
        fps: exportOptions.fps,
        targetVideoBitrateKbps: exportOptions.targetVideoBitrateKbps,
        speed: exportOptions.speed,
        exportProfile: exportOptions.exportProfile || 'turbo_nv',
        preferNvTurbo: exportOptions.preferNvTurbo ?? true,
        qualityGatePercent: exportOptions.qualityGatePercent ?? 3,
        turboCodec: exportOptions.turboCodec || 'hevc',
        preRenderPolicy: exportOptions.preRenderPolicy || 'idle_only',
        outputDir: exportOptions.outputDir || '',
        video: videoEl,
        canvas: canvasEl,
        tempCanvas: props.tempCanvasRef.current!,
        segment,
        backgroundConfig: props.backgroundConfig,
        mousePositions: props.mousePositions,
        audio: props.audioRef.current || undefined,
        audioFilePath: props.audioFilePath || sourceVideoPath,
        videoFilePath: sourceVideoPath
      }).catch((error) => {
        console.error('[ExportPrep] Warm preparation failed:', error);
      });
    }, primeDelayMs);

    return () => {
      window.clearTimeout(timer);
    };
  }, [
    isProcessing,
    showExportDialog,
    hasCheckedExportCapabilities,
    exportOptions.width,
    exportOptions.height,
    exportOptions.fps,
    exportOptions.targetVideoBitrateKbps,
    exportOptions.speed,
    exportOptions.exportProfile,
    exportOptions.preferNvTurbo,
    exportOptions.qualityGatePercent,
    exportOptions.turboCodec,
    exportOptions.preRenderPolicy,
    exportOptions.outputDir,
    props.currentVideo,
    props.segment,
    props.videoRef,
    props.canvasRef,
    props.tempCanvasRef,
    props.backgroundConfig,
    props.mousePositions,
    props.audioRef,
    props.audioFilePath,
    resolveSourceVideoPath
  ]);

  const startExport = useCallback(async () => {
    if (!props.currentVideo || !props.segment || !props.videoRef.current || !props.canvasRef.current) return;
    const sourceVideoPath = resolveSourceVideoPath();

    try {
      setShowExportDialog(false);
      setIsProcessing(true);
      await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

      const res = await videoExporter.exportAndDownload({
        width: exportOptions.width, height: exportOptions.height, fps: exportOptions.fps, targetVideoBitrateKbps: exportOptions.targetVideoBitrateKbps, speed: exportOptions.speed,
        exportProfile: exportOptions.exportProfile || 'turbo_nv',
        preferNvTurbo: exportOptions.preferNvTurbo ?? true,
        qualityGatePercent: exportOptions.qualityGatePercent ?? 3,
        turboCodec: exportOptions.turboCodec || 'hevc',
        preRenderPolicy: exportOptions.preRenderPolicy || 'idle_only',
        outputDir: exportOptions.outputDir || '',
        format: exportOptions.format || 'mp4',
        video: props.videoRef.current, canvas: props.canvasRef.current, tempCanvas: props.tempCanvasRef.current!,
        segment: props.segment, backgroundConfig: props.backgroundConfig, mousePositions: props.mousePositions,
        audio: props.audioRef.current || undefined,
        audioFilePath: props.audioFilePath || sourceVideoPath,
        videoFilePath: sourceVideoPath,
        onProgress: setExportProgress
      });
      if (res?.status === 'success' && typeof res.path === 'string' && res.path) {
        setLastExportedPath(res.path);
        setShowExportSuccessDialog(true);
        if (exportAutoCopyEnabled) {
          invoke('copy_video_file_to_clipboard', { filePath: res.path }).catch(console.error);
        }
      }
    } catch (error) {
      console.error('[Export] Error:', error);
    } finally {
      setIsProcessing(false);
      setExportProgress(0);
    }
  }, [props, exportOptions, resolveSourceVideoPath, exportAutoCopyEnabled]);

  const cancelExport = useCallback(() => {
    videoExporter.cancel();
    setIsProcessing(false);
    setExportProgress(0);
  }, []);

  const hasAudio = Boolean(resolveSourceVideoPath());

  return {
    isProcessing, exportProgress, showExportDialog, setShowExportDialog,
    exportOptions, setExportOptions, handleExport, startExport, cancelExport, hasAudio,
    showExportSuccessDialog, setShowExportSuccessDialog, lastExportedPath, setLastExportedPath,
    exportAutoCopyEnabled, setExportAutoCopyEnabled, sourceVideoFps
  };
}

// ============================================================================
// useZoomKeyframes
// ============================================================================
interface UseZoomKeyframesProps {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null, addToHistory?: boolean) => void;
  videoRef: React.RefObject<HTMLVideoElement | null>;
  currentTime: number;
  isVideoReady: boolean;
  renderFrame: () => void;
  activePanel: string;
  setActivePanel: (panel: 'zoom' | 'background' | 'cursor' | 'text') => void;
}

export function useZoomKeyframes(props: UseZoomKeyframesProps) {
  const [editingKeyframeId, setEditingKeyframeId] = useState<number | null>(null);
  const [zoomFactor, setZoomFactor] = useState(1.5);
  // Stable ref so handleAddKeyframe always reads the latest timeline position
  // without needing currentTime in its dependency array (which changes 60fps).
  const currentTimeRef = useRef(props.currentTime);
  currentTimeRef.current = props.currentTime;

  const handleAddKeyframe = useCallback((override?: Partial<ZoomKeyframe>) => {
    if (!props.segment || !props.videoRef.current) return;

    // Use the React-state currentTime (what the user sees on the timeline),
    // NOT videoRef.current.currentTime which can silently diverge when
    // throttledUpdateZoom seeks the video element to an editing keyframe's time.
    const currentVideoTime = currentTimeRef.current;
    const nearbyIndex = props.segment.zoomKeyframes.findIndex(k => Math.abs(k.time - currentVideoTime) < 0.2);
    let updatedKeyframes: ZoomKeyframe[];

    if (nearbyIndex !== -1) {
      const existing = props.segment.zoomKeyframes[nearbyIndex];
      updatedKeyframes = [...props.segment.zoomKeyframes];
      updatedKeyframes[nearbyIndex] = {
        ...existing,
        zoomFactor: override?.zoomFactor ?? existing.zoomFactor,
        positionX: override?.positionX ?? existing.positionX,
        positionY: override?.positionY ?? existing.positionY,
      };
      setEditingKeyframeId(nearbyIndex);
    } else {
      const previousKeyframe = [...props.segment.zoomKeyframes]
        .sort((a, b) => b.time - a.time)
        .find(k => k.time < currentVideoTime);

      const newKeyframe: ZoomKeyframe = {
        time: currentVideoTime, duration: 2.0,
        zoomFactor: override?.zoomFactor ?? previousKeyframe?.zoomFactor ?? 1.5,
        positionX: override?.positionX ?? previousKeyframe?.positionX ?? 0.5,
        positionY: override?.positionY ?? previousKeyframe?.positionY ?? 0.5,
        easingType: 'easeInOut'
      };

      updatedKeyframes = [...props.segment.zoomKeyframes, newKeyframe].sort((a, b) => a.time - b.time);
      setEditingKeyframeId(updatedKeyframes.indexOf(newKeyframe));
    }

    props.setSegment({ ...props.segment, zoomKeyframes: updatedKeyframes });
    const finalFactor = override?.zoomFactor ?? updatedKeyframes[updatedKeyframes.length - 1]?.zoomFactor;
    if (finalFactor !== undefined) setZoomFactor(finalFactor);
  }, [props.segment, props.videoRef, props.setSegment]);

  const handleDeleteKeyframe = useCallback(() => {
    if (props.segment && editingKeyframeId !== null) {
      props.setSegment({
        ...props.segment,
        zoomKeyframes: props.segment.zoomKeyframes.filter((_, i) => i !== editingKeyframeId)
      });
      setEditingKeyframeId(null);
    }
  }, [props.segment, editingKeyframeId, props.setSegment]);

  const throttledUpdateZoom = useThrottle((updates: Partial<ZoomKeyframe>) => {
    if (!props.segment || editingKeyframeId === null) return;

    const updatedKeyframes = props.segment.zoomKeyframes.map((kf, i) =>
      i === editingKeyframeId ? { ...kf, ...updates } : kf
    );

    props.setSegment({ ...props.segment, zoomKeyframes: updatedKeyframes }, false);

    if (props.videoRef.current) {
      const kf = updatedKeyframes[editingKeyframeId];
      if (Math.abs(props.videoRef.current.currentTime - kf.time) > 0.1) {
        props.videoRef.current.currentTime = kf.time;
      }
    }

    requestAnimationFrame(() => props.renderFrame());
  }, 32);

  // Active keyframe tracking
  useEffect(() => {
    if (!props.segment || !props.isVideoReady) return;

    const sortedKeyframes = [...props.segment.zoomKeyframes].sort((a, b) => a.time - b.time);
    for (let i = 0; i < sortedKeyframes.length; i++) {
      const { rangeStart, rangeEnd } = getKeyframeRange(sortedKeyframes, i);
      if (props.currentTime >= rangeStart && props.currentTime <= rangeEnd) {
        if (editingKeyframeId !== i) {
          setEditingKeyframeId(i);
          setZoomFactor(sortedKeyframes[i].zoomFactor);
          if (props.activePanel !== "zoom") props.setActivePanel("zoom");
        }
        return;
      }
    }
    if (editingKeyframeId !== null) setEditingKeyframeId(null);
  }, [props.currentTime, props.segment, props.isVideoReady]);

  // Sync zoomFactor with editing keyframe
  useEffect(() => {
    if (props.segment && editingKeyframeId !== null) {
      const kf = props.segment.zoomKeyframes[editingKeyframeId];
      if (kf) setZoomFactor(kf.zoomFactor);
    }
  }, [editingKeyframeId, props.segment]);

  return {
    editingKeyframeId, setEditingKeyframeId, zoomFactor, setZoomFactor,
    handleAddKeyframe, handleDeleteKeyframe, throttledUpdateZoom
  };
}

// ============================================================================
// useTextOverlays
// ============================================================================
interface UseTextOverlaysProps {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null) => void;
  currentTime: number;
  duration: number;
  setActivePanel: (panel: 'zoom' | 'background' | 'cursor' | 'text') => void;
}

export function useTextOverlays(props: UseTextOverlaysProps) {
  const [editingTextId, setEditingTextId] = useState<string | null>(null);

  const handleAddText = useCallback((atTime?: number) => {
    if (!props.segment) return;
    const t0 = atTime ?? props.currentTime;
    const segDur = 3;
    const startTime = Math.max(0, t0 - segDur / 2);

    const newText: TextSegment = {
      id: crypto.randomUUID(),
      startTime,
      endTime: Math.min(startTime + segDur, props.duration),
      text: 'New Text',
      style: {
        fontSize: 116, color: '#ffffff', x: 50, y: 50,
        fontVariations: { wght: 693, wdth: 96, slnt: 0, ROND: 100 },
        textAlign: 'center', opacity: 1, letterSpacing: 1,
        background: { enabled: true, color: '#000000', opacity: 0.6, paddingX: 16, paddingY: 8, borderRadius: 32 }
      }
    };

    props.setSegment({ ...props.segment, textSegments: [...(props.segment.textSegments || []), newText] });
    setEditingTextId(newText.id);
    props.setActivePanel('text');
  }, [props.segment, props.currentTime, props.duration, props.setSegment, props.setActivePanel]);

  const handleTextDragMove = useCallback((id: string, x: number, y: number) => {
    if (!props.segment) return;
    props.setSegment({
      ...props.segment,
      textSegments: props.segment.textSegments.map(t => t.id === id ? { ...t, style: { ...t.style, x, y } } : t)
    });
  }, [props.segment, props.setSegment]);

  const handleDeleteText = useCallback(() => {
    if (!props.segment || !editingTextId) return;
    props.setSegment({
      ...props.segment,
      textSegments: props.segment.textSegments.filter(ts => ts.id !== editingTextId)
    });
    setEditingTextId(null);
  }, [props.segment, editingTextId, props.setSegment]);

  return { editingTextId, setEditingTextId, handleAddText, handleDeleteText, handleTextDragMove };
}

// ============================================================================
// useAutoZoom
// ============================================================================
interface UseAutoZoomProps {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null) => void;
  videoRef: React.RefObject<HTMLVideoElement | null>;
  mousePositions: MousePosition[];
  duration: number;
  currentProjectId: string | null;
  backgroundConfig: BackgroundConfig;
  loadProjects: () => Promise<void>;
  setActivePanel: (panel: 'zoom' | 'background' | 'cursor' | 'text') => void;
}

export function useAutoZoom(props: UseAutoZoomProps) {
  const handleAutoZoom = useCallback(() => {
    if (!props.segment) return;

    // Toggle: if auto zoom is already active, clear it
    const hasAutoPath = props.segment.smoothMotionPath && props.segment.smoothMotionPath.length > 0;
    if (hasAutoPath) {
      saveAutoZoomPref(false);
      const newSegment: VideoSegment = {
        ...props.segment,
        smoothMotionPath: [],
        zoomInfluencePoints: []
      };
      props.setSegment(newSegment);
      if (props.currentProjectId) {
        projectManager.updateProject(props.currentProjectId, {
          segment: newSegment, backgroundConfig: props.backgroundConfig, mousePositions: props.mousePositions
        }).then(() => props.loadProjects());
      }
      return;
    }

    if (!props.mousePositions.length || !props.videoRef.current) return;

    const vid = props.videoRef.current;
    const normalizedMousePositions = normalizeMousePositionsToVideoSpace(
      props.mousePositions,
      vid.videoWidth || 0,
      vid.videoHeight || 0
    );
    const motionPath = autoZoomGenerator.generateMotionPath(
      props.segment,
      normalizedMousePositions,
      vid.videoWidth,
      vid.videoHeight
    );

    saveAutoZoomPref(true);
    const newSegment: VideoSegment = {
      ...props.segment,
      smoothMotionPath: motionPath,
      zoomInfluencePoints: [{ time: 0, value: 1.0 }, { time: props.duration, value: 1.0 }]
    };

    props.setSegment(newSegment);

    if (props.currentProjectId) {
      projectManager.updateProject(props.currentProjectId, {
        segment: newSegment, backgroundConfig: props.backgroundConfig, mousePositions: props.mousePositions
      }).then(() => props.loadProjects());
    }

    props.setActivePanel('zoom');
  }, [props]);

  return { handleAutoZoom };
}

// ============================================================================
// useCursorHiding
// ============================================================================
interface UseCursorHidingProps {
  segment: VideoSegment | null;
  setSegment: (segment: VideoSegment | null) => void;
  mousePositions: MousePosition[];
  currentTime: number;
  duration: number;
  videoRef: React.RefObject<HTMLVideoElement | null>;
}

export function useCursorHiding(props: UseCursorHidingProps) {
  const [editingPointerId, setEditingPointerId] = useState<string | null>(null);

  const handleSmartPointerHiding = useCallback(() => {
    if (!props.segment) return;

    const segs = props.segment.cursorVisibilitySegments;
    // Check if current state is "default" (single full-duration segment) or empty
    const isDefault = !segs || segs.length === 0 || (
      segs.length === 1 &&
      Math.abs(segs[0].startTime - 0) < 0.01 &&
      Math.abs(segs[0].endTime - props.duration) < 0.01
    );

    if (!isDefault) {
      // Has customized/generated segments → reset to default (cursor visible everywhere)
      saveSmartPointerPref(false);
      props.setSegment({
        ...props.segment,
        cursorVisibilitySegments: [{
          id: crypto.randomUUID(),
          startTime: 0,
          endTime: props.duration,
        }],
      });
      setEditingPointerId(null);
      return;
    }

    // Default or empty → generate from mouse data
    saveSmartPointerPref(true);
    const vidW = props.videoRef.current?.videoWidth || 0;
    const vidH = props.videoRef.current?.videoHeight || 0;
    const normalizedMousePositions = normalizeMousePositionsToVideoSpace(
      props.mousePositions,
      vidW,
      vidH
    );
    const segments = generateCursorVisibility(props.segment, normalizedMousePositions, props.duration);
    props.setSegment({
      ...props.segment,
      cursorVisibilitySegments: clampVisibilitySegmentsToDuration(segments, props.duration),
    });
  }, [props.segment, props.mousePositions, props.setSegment, props.duration, props.videoRef]);

  const handleAddPointerSegment = useCallback((atTime?: number) => {
    if (!props.segment) return;
    const t0 = atTime ?? props.currentTime;
    const segDur = 2;
    const startTime = Math.max(0, t0 - segDur / 2);

    const newSeg: CursorVisibilitySegment = {
      id: crypto.randomUUID(),
      startTime,
      endTime: Math.min(startTime + segDur, props.duration),
    };

    const allSegs = [...(props.segment.cursorVisibilitySegments || []), newSeg];
    props.setSegment({
      ...props.segment,
      cursorVisibilitySegments: clampVisibilitySegmentsToDuration(mergePointerSegments(allSegs), props.duration),
    });
    setEditingPointerId(null);
  }, [props.segment, props.currentTime, props.duration, props.setSegment]);

  const handleDeletePointerSegment = useCallback(() => {
    if (!props.segment || !editingPointerId) return;
    const remaining = props.segment.cursorVisibilitySegments?.filter(s => s.id !== editingPointerId) ?? [];
    props.setSegment({
      ...props.segment,
      cursorVisibilitySegments: clampVisibilitySegmentsToDuration(remaining, props.duration),
    });
    setEditingPointerId(null);
  }, [props.segment, editingPointerId, props.setSegment]);

  return {
    editingPointerId,
    setEditingPointerId,
    handleSmartPointerHiding,
    handleAddPointerSegment,
    handleDeletePointerSegment,
  };
}
