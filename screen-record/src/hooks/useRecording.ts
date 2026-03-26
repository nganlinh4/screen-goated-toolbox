import { useState, useRef, useEffect } from "react";
import { invoke } from "@/lib/ipc";
import { videoRenderer } from "@/lib/videoRenderer";
import { createVideoController } from "@/lib/videoController";
import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import { autoZoomGenerator } from "@/lib/autoZoom";
import {
  BackgroundConfig,
  VideoSegment,
  MousePosition,
  CursorVisibilitySegment,
  RawInputEvent,
  RecordingMode,
} from "@/types/video";
import {
  generateCursorVisibility,
} from "@/lib/cursorHiding";
import { buildKeystrokeEvents } from "@/lib/keystrokeProcessor";
import {
  filterKeystrokeEventsByMode,
  generateKeystrokeVisibilitySegments,
} from "@/lib/keystrokeVisibility";
import { normalizeMousePositionsToVideoSpace } from "@/lib/dynamicCapture";
import {
  buildFlatDeviceAudioPoints,
} from "@/lib/deviceAudio";
import {
  buildFlatMicAudioPoints,
} from "@/lib/micAudio";
import {
  buildFullWebcamVisibilitySegments,
} from "@/lib/webcamVisibility";
import {
  sanitizeRecordingAudioSelection,
  type RecordingAudioSelection,
} from "@/types/recordingAudio";
import { stabilizeMousePositionsForTimeline } from "./videoStateHelpers";
import {
  getSavedKeystrokeDelaySec,
  getSavedKeystrokeLanguage,
  getSavedKeystrokeModePref,
  getSavedKeystrokeOverlayPref,
  getSavedCropPref,
  getSavedAutoZoomPref,
  getSavedAutoZoomConfig,
  getSavedSmartPointerPref,
  normalizeTrackDelaySec,
} from "./videoStatePreferences";

// ============================================================================
// useRecording
// ============================================================================
interface UseRecordingProps {
  videoControllerRef: React.MutableRefObject<
    ReturnType<typeof createVideoController> | undefined
  >;
  videoRef: React.RefObject<HTMLVideoElement | null>;
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  tempCanvasRef: React.RefObject<HTMLCanvasElement>;
  backgroundConfig: BackgroundConfig;
  setSegment: (segment: VideoSegment | null) => void;
  setCurrentVideo: (url: string | null) => void;
  setCurrentAudio: (url: string | null) => void;
  setCurrentMicAudio: (url: string | null) => void;
  setCurrentWebcamVideo: (url: string | null) => void;
  setIsVideoReady: (ready: boolean) => void;
  setThumbnails: (thumbnails: string[]) => void;
  invalidateThumbnails: () => void;
  setDuration: (duration: number) => void;
  setCurrentTime: (time: number) => void;
  generateThumbnailsForSource: (options?: {
    videoUrl?: string | null;
    filePath?: string;
    segment?: VideoSegment | null;
    deferMs?: number;
    thumbnailCount?: number;
  }) => Promise<void>;
  generateThumbnail: () => string | undefined;
  renderFrame: () => void;
  currentVideo: string | null;
  currentAudio: string | null;
  currentMicAudio: string | null;
  currentWebcamVideo: string | null;
}

export function useRecording(props: UseRecordingProps) {
  const [isRecording, setIsRecording] = useState(false);
  const [activeRecordingMode, setActiveRecordingMode] =
    useState<RecordingMode>("withoutCursor");
  const [recordingDuration, setRecordingDuration] = useState(0);
  const [isLoadingVideo, setIsLoadingVideo] = useState(false);
  const [loadingProgress, setLoadingProgress] = useState(0);
  const [mousePositions, setMousePositions] = useState<MousePosition[]>([]);
  const [audioFilePath, setAudioFilePath] = useState("");
  const [micAudioFilePath, setMicAudioFilePath] = useState("");
  const [webcamVideoFilePath, setWebcamVideoFilePath] = useState("");
  const [videoFilePath, setVideoFilePath] = useState("");
  const [videoFilePathOwnerUrl, setVideoFilePathOwnerUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const activeRecordingAudioSelectionRef = useRef<RecordingAudioSelection>(
    sanitizeRecordingAudioSelection({
      deviceEnabled: true,
      micEnabled: false,
      deviceMode: "all",
      selectedDeviceApp: null,
    }),
  );

  const startNewRecording = async (
    targetId: string,
    recordingMode: RecordingMode,
    targetType: "monitor" | "window" = "monitor",
    targetFps?: number,
    recordingAudioSelection?: RecordingAudioSelection,
  ) => {
    try {
      if (props.currentVideo) {
        // User is editing a video — don't touch the preview at all. The canvas,
        // segment, playback state, and video URL all stay intact so editing can
        // continue uninterrupted. Old URLs are revoked in handleStopRecording
        // once the new video is ready to replace them.
      } else {
        setAudioFilePath("");
        setMicAudioFilePath("");
        setWebcamVideoFilePath("");
        setVideoFilePath("");
        setVideoFilePathOwnerUrl("");
        setMousePositions([]);
        // No existing video — safe to clear everything for a clean slate.
        props.setIsVideoReady(false);
        props.setCurrentTime(0);
        props.setDuration(0);
        props.setSegment(null);
        props.setThumbnails([]);
        if (props.videoRef.current) {
          props.videoRef.current.pause();
          props.videoRef.current.removeAttribute("src");
          props.videoRef.current.load();
          props.videoRef.current.currentTime = 0;
        }
        const canvas = props.canvasRef.current;
        if (canvas) {
          const ctx = canvas.getContext("2d");
          if (ctx) ctx.clearRect(0, 0, canvas.width, canvas.height);
        }
      }

      const audioSelection = sanitizeRecordingAudioSelection(
        recordingAudioSelection ?? {
          deviceEnabled: true,
          micEnabled: false,
          deviceMode: "all",
          selectedDeviceApp: null,
        },
      );
      activeRecordingAudioSelectionRef.current = audioSelection;

      await invoke("start_recording", {
        targetId,
        targetType,
        includeCursor: recordingMode === "withCursor",
        fps: targetFps ?? null,
        deviceAudioEnabled: audioSelection.deviceEnabled,
        deviceAudioMode: audioSelection.deviceMode,
        deviceAudioAppPid: audioSelection.selectedDeviceApp?.pid ?? null,
        micEnabled: audioSelection.micEnabled,
        webcamEnabled: true,
      });
      setActiveRecordingMode(recordingMode);
      setIsRecording(true);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleStopRecording = async (): Promise<{
    mouseData: MousePosition[];
    initialSegment: VideoSegment;
    videoUrl: string;
    webcamVideoUrl: string;
    recordingMode: RecordingMode;
    rawVideoPath: string;
    rawMicAudioPath: string;
    rawWebcamVideoPath: string;
    capturedFps: number | null;
  } | null> => {
    if (!isRecording) return null;

    let objectUrl: string | undefined;
    let audioObjectUrl: string | undefined;
    let micAudioObjectUrl: string | undefined;
    let webcamVideoObjectUrl: string | undefined;

    try {
      setIsRecording(false);
      setIsLoadingVideo(true);
      props.setIsVideoReady(false);
      props.invalidateThumbnails();
      props.setSegment(null);
      props.setCurrentTime(0);
      props.setDuration(0);
      setLoadingProgress(0);

      const result = await invoke<{
        videoUrl: string;
        deviceAudioUrl: string;
        micAudioUrl: string;
        webcamVideoUrl: string;
        micAudioOffsetSec: number;
        webcamVideoOffsetSec: number;
        mouseData: any[];
        deviceAudioPath: string;
        micAudioPath: string;
        webcamVideoPath: string;
        videoFilePath: string;
        inputEvents: RawInputEvent[];
        capturedFps: number | null;
      }>("stop_recording");
      const capturedFps =
        typeof result.capturedFps === "number" && result.capturedFps > 0
          ? result.capturedFps
          : null;
      setAudioFilePath(result.deviceAudioPath || "");
      setMicAudioFilePath(result.micAudioPath || "");
      setWebcamVideoFilePath(result.webcamVideoPath || "");
      setVideoFilePath(result.videoFilePath || "");

      const mouseData: MousePosition[] = result.mouseData.map((p) => ({
        x: p.x,
        y: p.y,
        timestamp: p.timestamp,
        isClicked: p.isClicked !== undefined ? p.isClicked : p.is_clicked,
        cursor_type: p.cursor_type || "default",
        captureWidth: p.captureWidth ?? p.capture_width,
        captureHeight: p.captureHeight ?? p.capture_height,
      }));

      objectUrl = await props.videoControllerRef.current?.loadVideo({
        videoUrl: result.videoUrl,
        onLoadingProgress: setLoadingProgress,
        debugLabel: "recording-stop",
      });

      if (objectUrl) {
        const videoDuration = props.videoRef.current?.duration || 0;
        const maxMouseTimestamp = result.mouseData.reduce((max, entry) => {
          const ts = typeof entry?.timestamp === "number" ? entry.timestamp : 0;
          return Math.max(max, ts);
        }, 0);
        const maxInputTimestamp = (result.inputEvents || []).reduce(
          (max: number, entry: any) => {
            const ts =
              typeof entry?.timestamp === "number" ? entry.timestamp : 0;
            return Math.max(max, ts);
          },
          0,
        );
        const timelineDuration =
          videoDuration > 0
            ? videoDuration
            : Math.max(maxMouseTimestamp, maxInputTimestamp);
        const stabilizedMouseData = stabilizeMousePositionsForTimeline(
          mouseData,
          timelineDuration,
        );
        setMousePositions(stabilizedMouseData);
        const micAudioAvailable =
          activeRecordingAudioSelectionRef.current.micEnabled &&
          Boolean(result.micAudioUrl || result.micAudioPath);
        const webcamAvailable = Boolean(
          result.webcamVideoUrl || result.webcamVideoPath,
        );
        const micDefaultVolume =
          micAudioAvailable &&
          !activeRecordingAudioSelectionRef.current.deviceEnabled
            ? 1
            : 0;
        const baseSegment: VideoSegment = {
          trimStart: 0,
          trimEnd: timelineDuration,
          trimSegments: [
            {
              id: crypto.randomUUID(),
              startTime: 0,
              endTime: timelineDuration,
            },
          ],
          zoomKeyframes: [],
          textSegments: [],
          speedPoints: [
            { time: 0, speed: 1 },
            { time: timelineDuration, speed: 1 },
          ],
          deviceAudioPoints: buildFlatDeviceAudioPoints(timelineDuration),
          micAudioPoints: buildFlatMicAudioPoints(
            timelineDuration,
            micDefaultVolume,
          ),
          micAudioOffsetSec: normalizeTrackDelaySec(result.micAudioOffsetSec),
          webcamVisibilitySegments: webcamAvailable
            ? buildFullWebcamVisibilitySegments(timelineDuration)
            : [],
          deviceAudioAvailable:
            activeRecordingAudioSelectionRef.current.deviceEnabled,
          micAudioAvailable,
          webcamOffsetSec: normalizeTrackDelaySec(result.webcamVideoOffsetSec),
          webcamAvailable,
        };

        const keystrokeEvents = buildKeystrokeEvents(
          result.inputEvents || [],
          timelineDuration,
        );
        const segmentWithKeystrokes: VideoSegment = {
          ...baseSegment,
          keystrokeEvents,
        };

        const vidW = props.videoRef.current?.videoWidth || 0;
        const vidH = props.videoRef.current?.videoHeight || 0;
        const normalizedMouseData =
          vidW > 0 && vidH > 0
            ? normalizeMousePositionsToVideoSpace(
                stabilizedMouseData,
                vidW,
                vidH,
              )
            : stabilizedMouseData;
        const normalizedPointerSegments = generateCursorVisibility(
          segmentWithKeystrokes,
          normalizedMouseData,
          timelineDuration,
          vidW,
          vidH,
          props.backgroundConfig,
        );
        const savedAutoZoomConfig = getSavedAutoZoomConfig();
        const initialAutoPath =
          vidW > 0 && vidH > 0 && normalizedMouseData.length > 0
            ? autoZoomGenerator.generateMotionPath(
                baseSegment,
                normalizedMouseData,
                vidW,
                vidH,
                savedAutoZoomConfig,
              )
            : [];

        const savedKeystrokeDelay = getSavedKeystrokeDelaySec();
        const keyboardVisibilitySegments = generateKeystrokeVisibilitySegments(
          filterKeystrokeEventsByMode(keystrokeEvents, "keyboard"),
          timelineDuration,
          { mode: "keyboard", delaySec: savedKeystrokeDelay },
        );
        const keyboardMouseVisibilitySegments =
          generateKeystrokeVisibilitySegments(
            filterKeystrokeEventsByMode(keystrokeEvents, "keyboardMouse"),
            timelineDuration,
            { mode: "keyboardMouse", delaySec: savedKeystrokeDelay },
          );

        const wantAutoZoom = getSavedAutoZoomPref();
        const wantSmartPointer = getSavedSmartPointerPref();
        const defaultPointerSeg: CursorVisibilitySegment[] = [
          { id: crypto.randomUUID(), startTime: 0, endTime: timelineDuration },
        ];

        const initialSegment: VideoSegment = {
          ...baseSegment,
          crop: getSavedCropPref(),
          cursorVisibilitySegments: wantSmartPointer
            ? normalizedPointerSegments
            : defaultPointerSeg,
          smoothMotionPath: wantAutoZoom ? initialAutoPath : [],
          zoomInfluencePoints:
            wantAutoZoom && initialAutoPath.length > 0
              ? [
                  { time: 0, value: 1.0 },
                  { time: timelineDuration, value: 1.0 },
                ]
              : [],
          keystrokeMode: getSavedKeystrokeModePref(),
          keystrokeDelaySec: savedKeystrokeDelay,
          keystrokeLanguage: getSavedKeystrokeLanguage(),
          keystrokeEvents,
          keyboardVisibilitySegments,
          keyboardMouseVisibilitySegments,
          keystrokeOverlay: getSavedKeystrokeOverlayPref(),
          useCustomCursor: activeRecordingMode !== "withCursor",
        };
        if (
          props.videoRef.current &&
          props.canvasRef.current &&
          props.videoRef.current.readyState >= 2
        ) {
          videoRenderer.drawFrame({
            video: props.videoRef.current,
            canvas: props.canvasRef.current,
            tempCanvas: props.tempCanvasRef.current!,
            segment: initialSegment,
            backgroundConfig: cloneBackgroundConfig(props.backgroundConfig),
            mousePositions: stabilizedMouseData,
            currentTime: 0,
          });
        }
        const placeholder = props.generateThumbnail();
        const placeholderStrip = placeholder
          ? Array.from({ length: 6 }, () => placeholder)
          : null;

        // Revoke the old video/audio URLs only once the new preview state is ready.
        if (props.currentVideo && props.currentVideo !== objectUrl)
          URL.revokeObjectURL(props.currentVideo);
        if (props.currentAudio) URL.revokeObjectURL(props.currentAudio);
        if (props.currentMicAudio) URL.revokeObjectURL(props.currentMicAudio);
        if (props.currentWebcamVideo) {
          URL.revokeObjectURL(props.currentWebcamVideo);
        }

        if (result.deviceAudioUrl) {
          audioObjectUrl = await props.videoControllerRef.current?.loadDeviceAudio({
            audioUrl: result.deviceAudioUrl,
          });
        }
        if (result.micAudioUrl) {
          micAudioObjectUrl = await props.videoControllerRef.current?.loadMicAudio(
            {
              audioUrl: result.micAudioUrl,
            },
          );
        }
        if (result.webcamVideoUrl) {
          webcamVideoObjectUrl =
            await props.videoControllerRef.current?.loadWebcamVideo({
              videoUrl: result.webcamVideoUrl,
            });
        }

        props.setCurrentVideo(objectUrl);
        setVideoFilePathOwnerUrl(objectUrl);
        props.setCurrentAudio(audioObjectUrl || null);
        props.setCurrentMicAudio(micAudioObjectUrl || null);
        props.setCurrentWebcamVideo(webcamVideoObjectUrl || null);
        props.setDuration(timelineDuration);
        props.setCurrentTime(initialSegment.trimStart);
        props.setSegment(initialSegment);
        if (placeholderStrip) {
          props.setThumbnails(placeholderStrip);
        }
        props.setIsVideoReady(true);
        // Restore the SR window so the user can review the new recording.
        invoke("restore_window").catch(() => {});
        void props.generateThumbnailsForSource({
          videoUrl: objectUrl,
          filePath: result.videoFilePath || undefined,
          segment: initialSegment,
          deferMs: 180,
        });

        return {
          mouseData: stabilizedMouseData,
          initialSegment,
          videoUrl: objectUrl,
          webcamVideoUrl: webcamVideoObjectUrl || "",
          recordingMode: activeRecordingMode,
          rawVideoPath: result.videoFilePath || "",
          rawMicAudioPath: result.micAudioPath || "",
          rawWebcamVideoPath: result.webcamVideoPath || "",
          capturedFps,
        };
      }
      return null;
    } catch (err) {
      if (objectUrl) URL.revokeObjectURL(objectUrl);
      if (audioObjectUrl) URL.revokeObjectURL(audioObjectUrl);
      if (micAudioObjectUrl) URL.revokeObjectURL(micAudioObjectUrl);
      if (webcamVideoObjectUrl) URL.revokeObjectURL(webcamVideoObjectUrl);
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
    return () => {
      if (interval) clearInterval(interval);
    };
  }, [isRecording]);

  return {
    isRecording,
    recordingDuration,
    isLoadingVideo,
    loadingProgress,
    mousePositions,
    setMousePositions,
    audioFilePath,
    micAudioFilePath,
    webcamVideoFilePath,
    videoFilePath,
    videoFilePathOwnerUrl,
    error,
    setError,
    startNewRecording,
    handleStopRecording,
  };
}
