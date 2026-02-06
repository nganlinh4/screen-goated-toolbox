import { useState, useRef, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { videoRenderer } from '@/lib/videoRenderer';
import { createVideoController } from '@/lib/videoController';
import { projectManager } from '@/lib/projectManager';
import { thumbnailGenerator } from '@/lib/thumbnailGenerator';
import { videoExporter } from '@/lib/videoExporter';
import { autoZoomGenerator } from '@/lib/autoZoom';
import { BackgroundConfig, VideoSegment, ZoomKeyframe, MousePosition, ExportOptions, Project, TextSegment } from '@/types/video';
import { getKeyframeRange } from '@/utils/helpers';
import { useThrottle } from './useAppHooks';

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
      backgroundType: 'solid' as const, customBackground: undefined, cropBottom: 0
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

  const generateThumbnail = useCallback((): string | undefined => {
    if (!canvasRef.current) return undefined;
    try { return canvasRef.current.toDataURL('image/jpeg', 0.5); } catch { return undefined; }
  }, []);

  const generateThumbnails = useCallback(async () => {
    if (!currentVideo || !segment) return;
    const newThumbnails = await thumbnailGenerator.generateThumbnails(currentVideo, 20, {
      trimStart: segment.trimStart, trimEnd: segment.trimEnd
    });
    setThumbnails(newThumbnails);
  }, [currentVideo, segment]);

  // Volume sync
  useEffect(() => {
    if (videoControllerRef.current && backgroundConfig.volume !== undefined) {
      videoControllerRef.current.setVolume(backgroundConfig.volume);
    }
  }, [backgroundConfig.volume]);

  // Render options sync
  useEffect(() => {
    if (!segment || !videoControllerRef.current) return;
    videoControllerRef.current.updateRenderOptions({ segment, backgroundConfig, mousePositions: mousePositionsRef.current });
  }, [segment, backgroundConfig]);

  // Animation effect
  useEffect(() => {
    const video = videoRef.current;
    if (!video || !segment) return;

    if (video.paused) {
      renderFrame();
    } else {
      const loopSegment = isCropping ? {
        ...segment, crop: undefined,
        zoomKeyframes: segment.zoomKeyframes.map(k => ({ ...k, zoomFactor: 1.0, positionX: 0.5, positionY: 0.5 }))
      } : segment;

      const loopBackground = isCropping ? {
        ...backgroundConfig, scale: 100, borderRadius: 0, shadow: 0,
        backgroundType: 'solid' as const, customBackground: undefined, cropBottom: 0
      } : backgroundConfig;

      videoRenderer.startAnimation({
        video, canvas: canvasRef.current!, tempCanvas: tempCanvasRef.current,
        segment: loopSegment, backgroundConfig: loopBackground, mousePositions: mousePositionsRef.current,
        currentTime: video.currentTime
      });
    }

    return () => { videoRenderer.stopAnimation(); };
  }, [segment, backgroundConfig, isCropping]);

  // Background config redraw
  useEffect(() => {
    if (videoRef.current && !videoRef.current.paused) return;
    requestAnimationFrame(() => renderFrame());
  }, [backgroundConfig, renderFrame]);

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
    renderFrame, togglePlayPause, seek, generateThumbnail, generateThumbnails
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
  generateThumbnails: () => void;
  generateThumbnail: () => string | undefined;
  renderFrame: () => void;
  currentVideo: string | null;
  currentAudio: string | null;
}

export function useRecording(props: UseRecordingProps) {
  const [isRecording, setIsRecording] = useState(false);
  const [recordingDuration, setRecordingDuration] = useState(0);
  const [isLoadingVideo, setIsLoadingVideo] = useState(false);
  const [loadingProgress, setLoadingProgress] = useState(0);
  const [mousePositions, setMousePositions] = useState<MousePosition[]>([]);
  const [audioFilePath, setAudioFilePath] = useState("");
  const [error, setError] = useState<string | null>(null);

  const startNewRecording = async (monitorId: string) => {
    try {
      setMousePositions([]);
      props.setIsVideoReady(false);
      props.setCurrentTime(0);
      props.setDuration(0);
      props.setSegment(null);
      props.setThumbnails([]);

      if (props.currentVideo) { URL.revokeObjectURL(props.currentVideo); props.setCurrentVideo(null); }
      if (props.currentAudio) { URL.revokeObjectURL(props.currentAudio); props.setCurrentAudio(null); }

      if (props.videoRef.current) {
        props.videoRef.current.pause();
        props.videoRef.current.src = "";
        props.videoRef.current.load();
        props.videoRef.current.currentTime = 0;
      }

      const canvas = props.canvasRef.current;
      if (canvas) {
        const ctx = canvas.getContext('2d');
        if (ctx) ctx.clearRect(0, 0, canvas.width, canvas.height);
      }

      await invoke("start_recording", { monitorId });
      setIsRecording(true);
      setError(null);
    } catch (err) {
      setError(err as string);
    }
  };

  const handleStopRecording = async (): Promise<{ mouseData: MousePosition[], initialSegment: VideoSegment, videoUrl: string } | null> => {
    if (!isRecording) return null;

    try {
      setIsRecording(false);
      setIsLoadingVideo(true);
      props.setIsVideoReady(false);
      setLoadingProgress(0);
      props.setThumbnails([]);

      const [videoUrl, audioUrl, rawMouseData, audioPath] = await invoke<[string, string, any[], string]>("stop_recording");
      setAudioFilePath(audioPath);

      const mouseData: MousePosition[] = rawMouseData.map(p => ({
        x: p.x, y: p.y, timestamp: p.timestamp,
        isClicked: p.isClicked !== undefined ? p.isClicked : p.is_clicked,
        cursor_type: p.cursor_type || 'default'
      }));
      setMousePositions(mouseData);

      const objectUrl = await props.videoControllerRef.current?.loadVideo({
        videoUrl, onLoadingProgress: setLoadingProgress
      });

      if (objectUrl) {
        props.setCurrentVideo(objectUrl);

        if (audioUrl) {
          const audioObjectUrl = await props.videoControllerRef.current?.loadAudio({ audioUrl });
          if (audioObjectUrl) props.setCurrentAudio(audioObjectUrl);
        }

        props.setIsVideoReady(true);
        props.generateThumbnails();

        const videoDuration = props.videoRef.current?.duration || 0;
        const initialSegment: VideoSegment = {
          trimStart: 0, trimEnd: videoDuration, zoomKeyframes: [], textSegments: []
        };
        props.setSegment(initialSegment);

        if (props.videoRef.current && props.canvasRef.current && props.videoRef.current.readyState >= 2) {
          videoRenderer.drawFrame({
            video: props.videoRef.current, canvas: props.canvasRef.current,
            tempCanvas: props.tempCanvasRef.current!, segment: initialSegment,
            backgroundConfig: props.backgroundConfig, mousePositions: mouseData, currentTime: 0
          });
        }

        return { mouseData, initialSegment, videoUrl: objectUrl };
      }
      return null;
    } catch (err) {
      setError(err as string);
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
    mousePositions, setMousePositions, audioFilePath, error, setError,
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
  currentVideo: string | null;
  currentAudio: string | null;
}

export function useProjects(props: UseProjectsProps) {
  const [projects, setProjects] = useState<Omit<Project, 'videoBlob'>[]>([]);
  const [showProjectsDialog, setShowProjectsDialog] = useState(false);
  const [currentProjectId, setCurrentProjectId] = useState<string | null>(null);

  const loadProjects = useCallback(async () => {
    const projects = await projectManager.getProjects();
    setProjects(projects);
  }, []);

  const handleLoadProject = useCallback(async (projectId: string) => {
    const project = await projectManager.loadProject(projectId);
    if (!project) return;

    if (props.currentVideo) URL.revokeObjectURL(props.currentVideo);
    if (props.currentAudio) URL.revokeObjectURL(props.currentAudio);

    props.setThumbnails([]);
    props.setCurrentAudio(null);

    const videoObjectUrl = await props.videoControllerRef.current?.loadVideo({ videoBlob: project.videoBlob });
    if (videoObjectUrl) props.setCurrentVideo(videoObjectUrl);

    if (project.audioBlob) {
      const audioObjectUrl = await props.videoControllerRef.current?.loadAudio({ audioBlob: project.audioBlob });
      if (audioObjectUrl) props.setCurrentAudio(audioObjectUrl);
    }

    const videoDuration = props.videoControllerRef.current?.duration || 0;
    const correctedSegment = { ...project.segment };
    if (correctedSegment.trimEnd === 0 || correctedSegment.trimEnd > videoDuration) {
      correctedSegment.trimEnd = videoDuration;
    }

    props.setSegment(correctedSegment);
    props.setBackgroundConfig(project.backgroundConfig);
    props.setMousePositions(project.mousePositions);

    if (props.videoControllerRef.current && project.backgroundConfig.volume !== undefined) {
      props.videoControllerRef.current.setVolume(project.backgroundConfig.volume);
    }

    setShowProjectsDialog(false);
    setCurrentProjectId(projectId);
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
  currentVideo: string | null;
}

export function useExport(props: UseExportProps) {
  const [isProcessing, setIsProcessing] = useState(false);
  const [exportProgress, setExportProgress] = useState(0);
  const [showExportDialog, setShowExportDialog] = useState(false);
  const [exportOptions, setExportOptions] = useState<ExportOptions>({
    quality: 'balanced', dimensions: '1080p', speed: 1
  });

  const handleExport = useCallback(() => setShowExportDialog(true), []);

  const startExport = useCallback(async () => {
    if (!props.currentVideo || !props.segment || !props.videoRef.current || !props.canvasRef.current) return;

    try {
      setShowExportDialog(false);
      setIsProcessing(true);

      await videoExporter.exportAndDownload({
        quality: exportOptions.quality, dimensions: exportOptions.dimensions, speed: exportOptions.speed,
        video: props.videoRef.current, canvas: props.canvasRef.current, tempCanvas: props.tempCanvasRef.current!,
        segment: props.segment, backgroundConfig: props.backgroundConfig, mousePositions: props.mousePositions,
        audio: props.audioRef.current || undefined, audioFilePath: props.audioFilePath,
        onProgress: setExportProgress
      });
    } catch (error) {
      console.error('[Export] Error:', error);
    } finally {
      setIsProcessing(false);
      setExportProgress(0);
    }
  }, [props, exportOptions]);

  return {
    isProcessing, exportProgress, showExportDialog, setShowExportDialog,
    exportOptions, setExportOptions, handleExport, startExport
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

  const handleAddKeyframe = useCallback((override?: Partial<ZoomKeyframe>) => {
    if (!props.segment || !props.videoRef.current) return;

    const currentVideoTime = props.videoRef.current.currentTime;
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

  const handleAddText = useCallback(() => {
    if (!props.segment) return;

    const newText: TextSegment = {
      id: crypto.randomUUID(),
      startTime: props.currentTime,
      endTime: Math.min(props.currentTime + 3, props.duration),
      text: 'New Text',
      style: { fontSize: 24, color: '#ffffff', x: 50, y: 50 }
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

  return { editingTextId, setEditingTextId, handleAddText, handleTextDragMove };
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
    const motionPath = autoZoomGenerator.generateMotionPath(
      props.segment, props.mousePositions, vid.videoWidth, vid.videoHeight
    );

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
