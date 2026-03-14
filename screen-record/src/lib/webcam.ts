import type {
  BakedWebcamFrame,
  VideoSegment,
  WebcamConfig,
  WebcamPosition,
} from "@/types/video";
import { getTrimSegments } from "@/lib/trimSegments";
import { getWebcamVisibility } from "@/lib/webcamVisibility";

const DEFAULT_WEBCAM_POSITION: WebcamPosition = "bottomRight";
const DEFAULT_WEBCAM_ASPECT = 16 / 9;
const DEFAULT_AUTO_ZOOM_RANGE = 0.8;
const DEFAULT_FPS = 60;

export const DEFAULT_WEBCAM_CONFIG: WebcamConfig = {
  visible: false,
  position: DEFAULT_WEBCAM_POSITION,
  mirror: false,
  roundnessPx: 28,
  maxSizePercent: 22,
  minSizePercent: 14,
  autoSizeDuringZoom: true,
  shadowPx: 28,
  insetPx: 28,
};

export interface WebcamLayoutRect {
  visible: boolean;
  x: number;
  y: number;
  width: number;
  height: number;
  roundnessPx: number;
  shadowPx: number;
  mirror: boolean;
}

export interface AnimatedWebcamLayoutRect extends WebcamLayoutRect {
  opacity: number;
}

export function cloneWebcamConfig(
  webcamConfig?: WebcamConfig | null,
): WebcamConfig {
  return {
    ...DEFAULT_WEBCAM_CONFIG,
    ...(webcamConfig ?? {}),
  };
}

export function resolveWebcamZoomInfluence(zoomFactor: number): number {
  if (!Number.isFinite(zoomFactor) || zoomFactor <= 1) return 0;
  return Math.max(
    0,
    Math.min(1, (zoomFactor - 1) / DEFAULT_AUTO_ZOOM_RANGE),
  );
}

export function resolveWebcamSizePercent(
  webcamConfig: WebcamConfig | null | undefined,
  zoomFactor: number,
): number {
  const config = cloneWebcamConfig(webcamConfig);
  if (!config.autoSizeDuringZoom) {
    return config.maxSizePercent;
  }

  const influence = resolveWebcamZoomInfluence(zoomFactor);
  return (
    config.maxSizePercent +
    (config.minSizePercent - config.maxSizePercent) * influence
  );
}

export function resolveWebcamLayoutRect(
  webcamConfig: WebcamConfig | null | undefined,
  canvasWidth: number,
  canvasHeight: number,
  webcamAspectRatio: number | null | undefined,
  zoomFactor: number,
  webcamAvailable: boolean = true,
): WebcamLayoutRect {
  const config = cloneWebcamConfig(webcamConfig);
  const aspectRatio =
    webcamAspectRatio && webcamAspectRatio > 0
      ? webcamAspectRatio
      : DEFAULT_WEBCAM_ASPECT;

  if (!webcamAvailable || !config.visible || canvasWidth <= 0 || canvasHeight <= 0) {
    return {
      visible: false,
      x: 0,
      y: 0,
      width: 0,
      height: 0,
      roundnessPx: config.roundnessPx,
      shadowPx: config.shadowPx,
      mirror: config.mirror,
    };
  }

  const sizePercent = resolveWebcamSizePercent(config, zoomFactor);
  let width = (canvasWidth * sizePercent) / 100;
  let height = width / aspectRatio;
  const maxHeight = canvasHeight * 0.42;
  if (height > maxHeight) {
    height = maxHeight;
    width = height * aspectRatio;
  }

  const inset = Math.max(12, config.insetPx);
  const x =
    config.position === "bottomLeft" || config.position === "topLeft"
      ? inset
      : canvasWidth - width - inset;
  const y =
    config.position === "topLeft" || config.position === "topRight"
      ? inset
      : canvasHeight - height - inset;

  return {
    visible: true,
    x,
    y,
    width,
    height,
    roundnessPx: Math.max(0, config.roundnessPx),
    shadowPx: Math.max(0, config.shadowPx),
    mirror: config.mirror,
  };
}

export function applyWebcamVisibilityToLayout(
  layout: WebcamLayoutRect,
  visibility: { opacity: number; scale: number },
): AnimatedWebcamLayoutRect {
  const opacity = Math.max(0, Math.min(1, visibility.opacity));
  const scale = Math.max(0, visibility.scale);

  if (!layout.visible || opacity <= 0.001 || scale <= 0.001) {
    return {
      ...layout,
      visible: false,
      opacity: 0,
      width: 0,
      height: 0,
    };
  }

  const scaledWidth = layout.width * scale;
  const scaledHeight = layout.height * scale;
  const deltaX = (layout.width - scaledWidth) * 0.5;
  const deltaY = (layout.height - scaledHeight) * 0.5;

  return {
    ...layout,
    visible: true,
    opacity,
    x: layout.x + deltaX,
    y: layout.y + deltaY,
    width: scaledWidth,
    height: scaledHeight,
  };
}

export function buildBakedWebcamFrames(
  segment: VideoSegment,
  webcamConfig: WebcamConfig | null | undefined,
  canvasWidth: number,
  canvasHeight: number,
  webcamAspectRatio: number | null | undefined,
  zoomSampler: (time: number) => number,
  fps: number = DEFAULT_FPS,
): BakedWebcamFrame[] {
  const duration = Math.max(
    segment.trimEnd,
    ...(segment.trimSegments ?? []).map((trimSegment) => trimSegment.endTime),
  );
  const trimSegments = getTrimSegments(segment, duration);
  if (trimSegments.length === 0) {
    return [];
  }

  const startTime = trimSegments[0].startTime;
  const endTime = trimSegments[trimSegments.length - 1].endTime;
  const step = 1 / Math.max(1, fps);
  const frames: BakedWebcamFrame[] = [];

  for (let time = startTime; time <= endTime + 0.00001; time += step) {
    const layout = resolveWebcamLayoutRect(
      webcamConfig,
      canvasWidth,
      canvasHeight,
      webcamAspectRatio,
      zoomSampler(time),
      segment.webcamAvailable !== false,
    );
    const visibility = getWebcamVisibility(
      time,
      segment.webcamVisibilitySegments,
    );
    const animatedLayout = applyWebcamVisibilityToLayout(layout, visibility);

    frames.push({
      time,
      visible: animatedLayout.visible,
      opacity: Number(animatedLayout.opacity.toFixed(4)),
      x: Number(animatedLayout.x.toFixed(3)),
      y: Number(animatedLayout.y.toFixed(3)),
      width: Number(animatedLayout.width.toFixed(3)),
      height: Number(animatedLayout.height.toFixed(3)),
      roundnessPx: Number(animatedLayout.roundnessPx.toFixed(3)),
      shadowPx: Number(animatedLayout.shadowPx.toFixed(3)),
      mirror: animatedLayout.mirror,
    });
  }

  return frames;
}
