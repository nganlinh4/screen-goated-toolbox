/**
 * Rendering and thumbnail generation helpers for VideoController.
 */

import { videoRenderer } from "./videoRenderer";
import { clampToTrimSegments } from "./trimSegments";
import type { RenderOptions } from "./videoControllerTypes";

// ---------------------------------------------------------------------------
// Render context building
// ---------------------------------------------------------------------------

export interface RenderHost {
  video: HTMLVideoElement;
  webcamVideo?: HTMLVideoElement;
  canvas: HTMLCanvasElement;
  tempCanvas: HTMLCanvasElement;
  renderOptions?: RenderOptions;
  getEffectiveDuration(fallback: number): number;
}

function getAdjustedTime(host: RenderHost, time: number): number {
  if (!host.renderOptions?.segment) return time;
  return clampToTrimSegments(
    time,
    host.renderOptions.segment,
    host.getEffectiveDuration(time),
  );
}

/**
 * Draw one frame using the current renderOptions (called when paused,
 * after seek, or on option changes).
 */
export function renderFrame(host: RenderHost): void {
  if (!host.renderOptions) return;
  const renderContext = {
    video: host.video,
    webcamVideo: host.webcamVideo,
    canvas: host.canvas,
    tempCanvas: host.tempCanvas,
    segment: host.renderOptions.segment,
    backgroundConfig: host.renderOptions.backgroundConfig,
    webcamConfig: host.renderOptions.webcamConfig,
    mousePositions: host.renderOptions.mousePositions,
    currentTime: getAdjustedTime(host, host.video.currentTime),
    interactiveBackgroundPreview: host.renderOptions.interactiveBackgroundPreview,
  };
  if (host.video.readyState >= 2) {
    const effectiveDuration = host.getEffectiveDuration(
      renderContext.video.currentTime,
    );
    if (
      renderContext.video.paused &&
      effectiveDuration > 0 &&
      renderContext.video.currentTime >= effectiveDuration
    ) return;
    videoRenderer.updateRenderContext(renderContext);
    videoRenderer.drawFrame(renderContext);
  }
}

// ---------------------------------------------------------------------------
// Thumbnail generation
// ---------------------------------------------------------------------------

export async function generateThumbnail(
  host: RenderHost,
  options: RenderOptions,
  setGenerating: (v: boolean) => void,
  afterRestore: () => void,
): Promise<string | undefined> {
  if (host.video.readyState < 2) return undefined;
  setGenerating(true);
  const savedTime = host.video.currentTime;
  try {
    host.video.currentTime = options.segment.trimStart;
    await new Promise<void>((r) => {
      const timeout = setTimeout(() => r(), 600);
      host.video.addEventListener(
        "seeked",
        () => { clearTimeout(timeout); r(); },
        { once: true },
      );
    });
    const thumbCanvas = document.createElement("canvas");
    thumbCanvas.width = host.canvas.width;
    thumbCanvas.height = host.canvas.height;
    const thumbTemp = document.createElement("canvas");
    videoRenderer.drawFrame({
      video: host.video,
      webcamVideo: host.webcamVideo,
      canvas: thumbCanvas,
      tempCanvas: thumbTemp,
      segment: options.segment,
      backgroundConfig: options.backgroundConfig,
      webcamConfig: options.webcamConfig,
      mousePositions: options.mousePositions,
      currentTime: options.segment.trimStart,
      interactiveBackgroundPreview: false,
    });
    return thumbCanvas.toDataURL("image/jpeg", 0.7);
  } catch {
    return undefined;
  } finally {
    host.video.currentTime = savedTime;
    await new Promise<void>((r) => {
      const timeout = setTimeout(() => r(), 600);
      host.video.addEventListener(
        "seeked",
        () => { clearTimeout(timeout); r(); },
        { once: true },
      );
    }).catch(() => {});
    setGenerating(false);
    afterRestore();
  }
}

// ---------------------------------------------------------------------------
// Render immediate (bypass React state)
// ---------------------------------------------------------------------------

export function renderImmediate(
  host: RenderHost,
  options: RenderOptions,
  applyAudioVolumes: () => void,
): void {
  if (host.video.readyState < 2) return;
  host.renderOptions = options;
  applyAudioVolumes();
  const ctx = {
    video: host.video,
    webcamVideo: host.webcamVideo,
    canvas: host.canvas,
    tempCanvas: host.tempCanvas,
    segment: options.segment,
    backgroundConfig: options.backgroundConfig,
    webcamConfig: options.webcamConfig,
    mousePositions: options.mousePositions,
    currentTime: options.segment.trimStart,
    interactiveBackgroundPreview: options.interactiveBackgroundPreview,
  };
  videoRenderer.updateRenderContext(ctx);
  videoRenderer.drawFrame(ctx);
}
