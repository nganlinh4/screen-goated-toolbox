import { useEffect, type RefObject } from "react";
import { VideoSegment, BackgroundConfig, MousePosition } from "@/types/video";
import { videoRenderer } from "@/lib/videoRenderer";
import { buildFlatDeviceAudioPoints } from "@/lib/deviceAudio";
import { buildFlatMicAudioPoints } from "@/lib/micAudio";
import { buildFullWebcamVisibilitySegments } from "@/lib/webcamVisibility";
import { getSavedCropPref, getSavedKeystrokeLanguage } from "@/hooks/useVideoState";
import {
  DEFAULT_KEYSTROKE_DELAY_SEC,
  getSavedKeystrokeModePref,
  getSavedKeystrokeOverlayPref,
} from "@/hooks/useKeystrokeOverlayEditor";

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
        speedPoints: [
          { time: 0, speed: 1 },
          { time: duration, speed: 1 },
        ],
        deviceAudioPoints: buildFlatDeviceAudioPoints(duration),
        micAudioPoints: buildFlatMicAudioPoints(duration),
        micAudioOffsetSec: 0,
        webcamVisibilitySegments: currentWebcamVideo
          ? buildFullWebcamVisibilitySegments(duration)
          : [],
        deviceAudioAvailable: true,
        micAudioAvailable: Boolean(currentMicAudio),
        webcamOffsetSec: 0,
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
