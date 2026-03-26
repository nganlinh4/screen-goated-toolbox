import { VideoSegment, ZoomKeyframe } from '@/types/video';
import {
  applyWebcamVisibilityToLayout,
  resolveWebcamLayoutRect,
} from '@/lib/webcam';
import { getWebcamVisibility } from '@/lib/webcamVisibility';
import type { RendererState } from './drawFrame';
import type { WebcamConfig } from '@/types/video';

/**
 * Draw the webcam overlay on top of the composited frame.
 * Extracted from drawFrame to keep file sizes manageable.
 */
export function drawWebcamOverlay(
  targetCtx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  activeZoomState: ZoomKeyframe | null,
  state: RendererState,
  video: HTMLVideoElement,
  webcamVideo: HTMLVideoElement | null | undefined,
  segment: VideoSegment,
  webcamConfig: WebcamConfig | undefined,
  canvasW: number,
  canvasH: number,
  webcamAspectRatio: number | null,
): void {
  if (video.currentTime + 0.0001 < (segment.webcamOffsetSec ?? 0)) return;
  let webcamSource: CanvasImageSource | null = null;
  if (webcamVideo && webcamVideo.readyState >= 2) {
    const frameW = webcamVideo.videoWidth || 1;
    const frameH = webcamVideo.videoHeight || 1;
    if (
      !state.webcamFrameCanvas ||
      state.webcamFrameCanvas.width !== frameW ||
      state.webcamFrameCanvas.height !== frameH
    ) {
      state.webcamFrameCanvas = new OffscreenCanvas(frameW, frameH);
      state.webcamFrameCtx = state.webcamFrameCanvas.getContext('2d');
      state.webcamFrameReady = false;
    }
    if (state.webcamFrameCanvas && state.webcamFrameCtx) {
      state.webcamFrameCtx.clearRect(0, 0, frameW, frameH);
      state.webcamFrameCtx.drawImage(webcamVideo, 0, 0, frameW, frameH);
      state.webcamFrameReady = true;
      webcamSource = state.webcamFrameCanvas;
    } else {
      webcamSource = webcamVideo;
    }
  } else if (state.webcamFrameCanvas && state.webcamFrameReady) {
    webcamSource = state.webcamFrameCanvas;
  }
  if (!webcamSource) return;

  const layout = resolveWebcamLayoutRect(
    webcamConfig,
    canvasW,
    canvasH,
    webcamAspectRatio,
    activeZoomState?.zoomFactor ?? 1,
    segment.webcamAvailable !== false,
  );
  const animatedLayout = applyWebcamVisibilityToLayout(
    layout,
    getWebcamVisibility(video.currentTime, segment.webcamVisibilitySegments),
  );
  if (
    !animatedLayout.visible ||
    animatedLayout.opacity <= 0.001 ||
    animatedLayout.width <= 0 ||
    animatedLayout.height <= 0
  ) {
    return;
  }

  targetCtx.save();
  targetCtx.setTransform(1, 0, 0, 1, 0, 0);
  targetCtx.globalAlpha = animatedLayout.opacity;

  if (animatedLayout.shadowPx > 0) {
    targetCtx.shadowColor = 'rgba(8, 10, 20, 0.32)';
    targetCtx.shadowBlur = animatedLayout.shadowPx;
    targetCtx.shadowOffsetY = Math.max(2, animatedLayout.shadowPx * 0.18);
  }

  targetCtx.beginPath();
  targetCtx.roundRect(
    animatedLayout.x,
    animatedLayout.y,
    animatedLayout.width,
    animatedLayout.height,
    Math.max(0, animatedLayout.roundnessPx),
  );
  targetCtx.fillStyle = 'rgba(0, 0, 0, 0.14)';
  targetCtx.fill();
  targetCtx.clip();
  targetCtx.shadowColor = 'transparent';
  targetCtx.shadowBlur = 0;
  targetCtx.shadowOffsetY = 0;

  if (animatedLayout.mirror) {
    targetCtx.translate(animatedLayout.x + animatedLayout.width, animatedLayout.y);
    targetCtx.scale(-1, 1);
    targetCtx.drawImage(webcamSource, 0, 0, animatedLayout.width, animatedLayout.height);
  } else {
    targetCtx.drawImage(
      webcamSource,
      animatedLayout.x,
      animatedLayout.y,
      animatedLayout.width,
      animatedLayout.height,
    );
  }
  targetCtx.restore();
}
