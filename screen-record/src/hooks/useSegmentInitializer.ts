import { useEffect, type RefObject } from "react";
import { VideoSegment, BackgroundConfig, MousePosition } from "@/types/video";
import { videoRenderer } from "@/lib/videoRenderer";
import { buildFlatDeviceAudioPoints } from "@/lib/deviceAudio";
import { buildFlatMicAudioPoints } from "@/lib/micAudio";
import { createSubtitleTrackStateFromSegments } from "@/lib/subtitleTracks";
import { buildFullWebcamVisibilitySegments } from "@/lib/webcamVisibility";
import { getDefaultAudioVolume } from "@/lib/recordingDefaults";
import {
  getSavedCropPref,
  getSavedKeystrokeLanguage,
  getSavedCustomCursorPref,
  getSavedKeystrokeDelaySec,
  getSavedKeystrokeModePref,
  getSavedKeystrokeOverlayPref,
} from "@/hooks/videoStatePreferences";

export interface UseSegmentInitializerParams {
  duration: number;
  segment: VideoSegment | null;
  backgroundConfig: BackgroundConfig;
  mousePositions: MousePosition[];
  currentMicAudio: string | null;
  currentWebcamVideo: string | null;
  setSegment: (s: VideoSegment | null) => void;
  videoRef: RefObject<HTMLVideoElement | null>;
  canvasRef: RefObject<HTMLCanvasElement | null>;
  tempCanvasRef: RefObject<HTMLCanvasElement | null>;
}

export function useSegmentInitializer({
  duration,
  segment,
  backgroundConfig,
  mousePositions,
  currentMicAudio,
  currentWebcamVideo,
  setSegment,
  videoRef,
  canvasRef,
  tempCanvasRef,
}: UseSegmentInitializerParams) {
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
        ...createSubtitleTrackStateFromSegments([]),
        speedPoints: [
          { time: 0, speed: 1 },
          { time: duration, speed: 1 },
        ],
        deviceAudioPoints: buildFlatDeviceAudioPoints(duration, getDefaultAudioVolume("device", 1)),
        deviceAudioOffsetSec: 0,
        micAudioPoints: buildFlatMicAudioPoints(duration, getDefaultAudioVolume("mic", 0)),
        micAudioOffsetSec: 0,
        webcamVisibilitySegments: currentWebcamVideo
          ? buildFullWebcamVisibilitySegments(duration)
          : [],
        deviceAudioAvailable: true,
        micAudioAvailable: Boolean(currentMicAudio),
        webcamOffsetSec: 0,
        keystrokeMode: getSavedKeystrokeModePref(),
        keystrokeDelaySec: getSavedKeystrokeDelaySec(),
        keystrokeLanguage: getSavedKeystrokeLanguage(),
        keystrokeEvents: [],
        keyboardVisibilitySegments: [],
        keyboardMouseVisibilitySegments: [],
        keystrokeOverlay: getSavedKeystrokeOverlayPref(),
        crop: getSavedCropPref(),
        useCustomCursor: getSavedCustomCursorPref(),
      };
      setSegment(initialSegment);
      setTimeout(() => {
        if (
          videoRef.current &&
          canvasRef.current &&
          tempCanvasRef.current &&
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
    currentMicAudio,
    currentWebcamVideo,
  ]);
}
