import { cloneBackgroundConfig } from "@/lib/backgroundConfig";
import type {
  BackgroundConfig,
  MousePosition,
  VideoSegment,
  WebcamConfig,
} from "@/types/video";

export function getPlaybackRenderSegment(
  segment: VideoSegment,
  isCropping: boolean,
) {
  return isCropping
    ? {
        ...segment,
        crop: undefined,
        zoomBlocks: [],
      }
    : segment;
}

export function getPlaybackRenderBackground(
  backgroundConfig: BackgroundConfig,
  isCropping: boolean,
) {
  return isCropping
    ? {
        ...backgroundConfig,
        scale: 100,
        borderRadius: 0,
        shadow: 0,
        backgroundType: "solid" as const,
        customBackground: undefined,
        cropBottom: 0,
        canvasMode: "auto" as const,
      }
    : cloneBackgroundConfig(backgroundConfig);
}

export function buildPlaybackRenderOptions({
  segment,
  backgroundConfig,
  webcamConfig,
  mousePositions,
  isCropping,
  outputFrameRate,
  interactiveBackgroundPreview,
}: {
  segment: VideoSegment;
  backgroundConfig: BackgroundConfig;
  webcamConfig?: WebcamConfig;
  mousePositions: MousePosition[];
  isCropping: boolean;
  outputFrameRate: number;
  interactiveBackgroundPreview?: boolean;
}) {
  return {
    segment: getPlaybackRenderSegment(segment, isCropping),
    backgroundConfig: getPlaybackRenderBackground(backgroundConfig, isCropping),
    webcamConfig: webcamConfig ? { ...webcamConfig } : undefined,
    mousePositions,
    outputFrameRate,
    interactiveBackgroundPreview,
  };
}
