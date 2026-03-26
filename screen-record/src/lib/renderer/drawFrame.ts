import { MousePosition, VideoSegment, ZoomKeyframe } from '@/types/video';
import { getCursorVisibility } from '@/lib/cursorHiding';
import {
  getCursorMovementDelaySec,
} from './cursorDynamics';
import {
  CursorImageSet,
  CursorRenderState,
  resolveCursorRenderType,
  drawMouseCursor,
} from './cursorGraphics';
import {
  GradientCache,
  CustomBgCache,
  getBackgroundStyle,
  fillBuiltInBackground,
  parseBuiltInBackgroundToken,
} from './gradientGenerator';
import {
  KeystrokeState,
  drawActiveKeystrokeOverlays,
} from './keystrokeRenderer';
import {
  drawTextOverlay,
} from './overlayBaker';
import type { RenderContext, RenderOptions } from './index';
import {
  getContainedRect,
  getLogicalCropSize,
  sampleCaptureDimensionsAtTime,
} from '@/lib/dynamicCapture';
import { drawWebcamOverlay } from './drawFrameWebcam';
import {
  interpolateCursorPosition,
  logPreviewCursorDebug,
  updateSquishAnimation,
} from './drawFrameCursor';

// ---------------------------------------------------------------------------
// RendererState - all mutable state needed by drawFrame
// ---------------------------------------------------------------------------

export interface RendererState {
  // Cursor images
  cursorImages: CursorImageSet;
  cursorState: CursorRenderState;

  // Gradient caches
  gradientCache: GradientCache;
  customBgCache: CustomBgCache;

  // Keystroke state
  keystrokeState: KeystrokeState;

  // Squish animation
  currentSquishScale: number;
  squishTarget: number;
  squishAnimFrom: number;
  squishAnimProgress: number;
  squishAnimDuration: number;
  squishHasRoom: boolean;    // locked-in at animation start -- keeps easing consistent mid-animation
  lastHoldTime: number;
  lastActiveEventId: string | null;

  // Motion blur canvases (reused across frames)
  blurAccumCanvas: OffscreenCanvas | null;
  blurAccumCtx: OffscreenCanvasRenderingContext2D | null;
  blurSubCanvas: OffscreenCanvas | null;
  blurSubCtx: OffscreenCanvasRenderingContext2D | null;
  webcamFrameCanvas: OffscreenCanvas | null;
  webcamFrameCtx: OffscreenCanvasRenderingContext2D | null;
  webcamFrameReady: boolean;

  // Timing
  isDrawing: boolean;
  lastDrawTime: number;
  latestElapsed: number;

  // Cursor processing cache
  processedCursorPositions: MousePosition[] | null;
  lastMousePositionsRef: MousePosition[] | null;
  lastCursorProcessSignature: string;
  lastCursorNormalizationSignature: string;
  lastCursorPreviewDebugSignature: string;
  lastCursorPreviewDebugBucket: number;
  lastCursorPreviewDebugPoint: { x: number; y: number } | null;

  // Methods from VideoRenderer that drawFrame delegates to
  calculateCurrentZoomState: (
    currentTime: number,
    segment: VideoSegment,
    viewW: number,
    viewH: number,
    srcCropW?: number,
    srcCropH?: number,
    videoScale?: number,
  ) => ZoomKeyframe;
  requestRedraw: () => void;
}

// ---------------------------------------------------------------------------
// drawFrame - main rendering entry point
// ---------------------------------------------------------------------------

export async function drawFrame(
  context: RenderContext,
  options: RenderOptions,
  state: RendererState,
): Promise<void> {
  if (state.isDrawing) return;

  const {
    video,
    webcamVideo,
    canvas,
    tempCanvas,
    segment,
    backgroundConfig,
    webcamConfig,
    mousePositions,
  } = context;
  if (!video || !canvas || !segment) return;
  if (video.readyState < 2) return;
  if (video.seeking) return;

  const isExportMode = options.exportMode || false;
  const quality: ImageSmoothingQuality = 'high';

  const ctx = canvas.getContext('2d', {
    alpha: false,
    willReadFrequently: false
  });
  if (!ctx) return;

  state.isDrawing = true;
  ctx.imageSmoothingQuality = quality as ImageSmoothingQuality;

  const now = performance.now();
  state.latestElapsed = state.lastDrawTime === 0 ? 1000 / 60 : now - state.lastDrawTime;
  state.lastDrawTime = now;

  const vidW = video.videoWidth;
  const vidH = video.videoHeight;
  const webcamAspectRatio =
    webcamVideo && webcamVideo.videoWidth > 0 && webcamVideo.videoHeight > 0
      ? webcamVideo.videoWidth / webcamVideo.videoHeight
      : null;

  if (!vidW || !vidH) {
    state.isDrawing = false;
    return;
  }

  const crop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
  const srcX = vidW * crop.x;
  const srcY = vidH * crop.y;
  const srcW = vidW * crop.width;
  const srcH = vidH * crop.height;

  const hasLockedAutoCanvas =
    backgroundConfig.canvasMode === 'auto' &&
    !!backgroundConfig.autoCanvasSourceId &&
    !!backgroundConfig.canvasWidth &&
    !!backgroundConfig.canvasHeight;
  const useExplicitCanvas =
    (backgroundConfig.canvasMode === 'custom' || hasLockedAutoCanvas) &&
    backgroundConfig.canvasWidth &&
    backgroundConfig.canvasHeight;
  const canvasW = useExplicitCanvas ? backgroundConfig.canvasWidth! : Math.round(srcW);
  const canvasH = useExplicitCanvas ? backgroundConfig.canvasHeight! : Math.round(srcH);

  if (canvas.width !== canvasW || canvas.height !== canvasH) {
    canvas.width = canvasW;
    canvas.height = canvasH;
  }

  if (!isExportMode) {
    canvas.style.aspectRatio = `${canvasW} / ${canvasH}`;
  }

  try {
    const legacyCrop = (backgroundConfig.cropBottom || 0) / 100;
    const scale = backgroundConfig.scale / 100;
    const captureDims = sampleCaptureDimensionsAtTime(
      video.currentTime,
      mousePositions,
      vidW,
      vidH
    );
    const logicalCrop = getLogicalCropSize(
      captureDims.width,
      captureDims.height,
      crop,
      backgroundConfig.cropBottom || 0
    );
    const contained = getContainedRect(
      canvasW,
      canvasH,
      logicalCrop.width,
      logicalCrop.height,
      scale
    );
    const scaledWidth = contained.width;
    const scaledHeight = contained.height;
    const x = contained.left;
    const y = contained.top;

    const zoomState = state.calculateCurrentZoomState(video.currentTime, segment, canvas.width, canvas.height, srcW, srcH, scale);

    // Supersample to keep zoom crisp
    const zf = zoomState?.zoomFactor ?? 1;
    const bgScale = Math.max(0.01, backgroundConfig.scale / 100);
    let ss = 1;
    const fullQualitySs = zf > 1 ? Math.min(Math.ceil(zf / bgScale), 4) : 1;
    const isRealtimePreview = !isExportMode && !video.paused;

    if (!isRealtimePreview) {
      ss = fullQualitySs;
    } else {
      const requiredSs = zf / bgScale;
      if (requiredSs > 1.05) {
        ss = Math.min(requiredSs, 2.5);
        const maxTempWidth = 3840;
        if (canvasW * ss > maxTempWidth) {
          ss = Math.max(1, maxTempWidth / canvasW);
        }
      }
    }

    // --- Prepare tempCanvas (video + shadow + border radius) ---
    const tempW = Math.round(canvasW * ss);
    const tempH = Math.round(canvasH * ss);
    if (tempCanvas.width !== tempW || tempCanvas.height !== tempH) {
      tempCanvas.width = tempW;
      tempCanvas.height = tempH;
    }
    const tempCtx = tempCanvas.getContext('2d', { alpha: true, willReadFrequently: false });
    if (!tempCtx) return;

    tempCtx.clearRect(0, 0, tempW, tempH);
    tempCtx.save();
    tempCtx.imageSmoothingEnabled = true;
    tempCtx.imageSmoothingQuality = 'high';
    if (ss > 1) tempCtx.scale(ss, ss);

    const radius = backgroundConfig.borderRadius;
    const offset = 0.5;

    if (backgroundConfig.shadow) {
      tempCtx.save();
      tempCtx.shadowColor = 'rgba(0, 0, 0, 0.5)';
      tempCtx.shadowBlur = backgroundConfig.shadow * ss;
      tempCtx.shadowOffsetY = backgroundConfig.shadow * 0.5 * ss;

      tempCtx.beginPath();
      tempCtx.moveTo(x + radius + offset, y + offset);
      tempCtx.lineTo(x + scaledWidth - radius - offset, y + offset);
      tempCtx.quadraticCurveTo(x + scaledWidth - offset, y + offset, x + scaledWidth - offset, y + radius + offset);
      tempCtx.lineTo(x + scaledWidth - offset, y + scaledHeight - radius - offset);
      tempCtx.quadraticCurveTo(x + scaledWidth - offset, y + scaledHeight - offset, x + scaledWidth - radius - offset, y + scaledHeight - offset);
      tempCtx.lineTo(x + radius + offset, y + scaledHeight - offset);
      tempCtx.quadraticCurveTo(x + offset, y + scaledHeight - offset, x + offset, y + scaledHeight - radius - offset);
      tempCtx.lineTo(x + offset, y + radius + offset);
      tempCtx.quadraticCurveTo(x + offset, y + offset, x + radius + offset, y + offset);
      tempCtx.closePath();

      tempCtx.fillStyle = '#fff';
      tempCtx.fill();
      tempCtx.restore();
    }

    tempCtx.beginPath();
    tempCtx.moveTo(x + radius + offset, y + offset);
    tempCtx.lineTo(x + scaledWidth - radius - offset, y + offset);
    tempCtx.quadraticCurveTo(x + scaledWidth - offset, y + offset, x + scaledWidth - offset, y + radius + offset);
    tempCtx.lineTo(x + scaledWidth - offset, y + scaledHeight - radius - offset);
    tempCtx.quadraticCurveTo(x + scaledWidth - offset, y + scaledHeight - offset, x + scaledWidth - radius - offset, y + scaledHeight - offset);
    tempCtx.lineTo(x + radius + offset, y + scaledHeight - offset);
    tempCtx.quadraticCurveTo(x + offset, y + scaledHeight - offset, x + offset, y + scaledHeight - radius - offset);
    tempCtx.lineTo(x + offset, y + radius + offset);
    tempCtx.quadraticCurveTo(x + offset, y + offset, x + radius + offset, y + offset);
    tempCtx.closePath();

    tempCtx.clip();

    try {
      tempCtx.drawImage(
        video,
        srcX, srcY, srcW, srcH * (1 - legacyCrop),
        x, y, scaledWidth, scaledHeight
      );
    } catch (_e) {
    }

    tempCtx.strokeStyle = 'rgba(0, 0, 0, 0.1)';
    tempCtx.lineWidth = 1;
    tempCtx.stroke();
    tempCtx.restore();

    // --- Compute cursor state (squish, visibility) once per frame ---
    const cursorTime = video.currentTime + getCursorMovementDelaySec(backgroundConfig);
    const interpolatedPosition = interpolateCursorPosition(
      cursorTime,
      mousePositions,
      state,
      vidW,
      vidH,
      backgroundConfig
    );
    const cursorVis = getCursorVisibility(video.currentTime, segment.cursorVisibilitySegments);
    const shouldRenderCustomCursor = segment.useCustomCursor !== false;
    const showCursor = Boolean(shouldRenderCustomCursor && interpolatedPosition && cursorVis.opacity > 0.001);
    if (!isExportMode) {
      logPreviewCursorDebug(
        state,
        video.currentTime,
        cursorTime,
        interpolatedPosition,
        showCursor,
        cursorVis,
        segment
      );
    }

    if (showCursor) {
      updateSquishAnimation(state, video, segment, interpolatedPosition!);
    }

    const bgStyle = getBackgroundStyle(ctx, backgroundConfig.backgroundType, state.customBgCache, () => {
      state.requestRedraw();
    }, backgroundConfig.customBackground);
    const sizeRatio = Math.min(
      canvas.width / Math.max(1, logicalCrop.width),
      canvas.height / Math.max(1, logicalCrop.height)
    );

    // Helper: compute cursor screen position for a given cursor + zoom state
    const cursorScreenPos = (
      cur: { x: number; y: number },
      zs: ZoomKeyframe | null
    ) => {
      const relCX = (cur.x - srcX) / srcW;
      const relCY = (cur.y - srcY) / (srcH * (1 - legacyCrop));
      let cx = x + relCX * scaledWidth;
      let cy = y + relCY * scaledHeight;
      if (zs && zs.zoomFactor !== 1) {
        cx = cx * zs.zoomFactor + (canvasW - canvasW * zs.zoomFactor) * zs.positionX;
        cy = cy * zs.zoomFactor + (canvasH - canvasH * zs.zoomFactor) * zs.positionY;
      }
      return { x: cx, y: cy };
    };

    // Helper: draw one composited sub-frame (background + video + cursor)
    const drawSubFrame = (
      tCtx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
      subZoom: ZoomKeyframe | null,
      subCur: { x: number; y: number; isClicked: boolean; cursor_type: string; cursor_rotation?: number } | null,
    ) => {
      tCtx.save();
      if (subZoom && subZoom.zoomFactor !== 1) {
        const zW = canvasW * subZoom.zoomFactor;
        const zH = canvasH * subZoom.zoomFactor;
        tCtx.translate((canvasW - zW) * subZoom.positionX, (canvasH - zH) * subZoom.positionY);
        tCtx.scale(subZoom.zoomFactor, subZoom.zoomFactor);
      }
      if (typeof bgStyle === 'string') {
        const builtInBackgroundId = parseBuiltInBackgroundToken(bgStyle);
        if (builtInBackgroundId) {
          fillBuiltInBackground(
            state.gradientCache,
            tCtx,
            builtInBackgroundId,
            canvasW,
            canvasH,
            Boolean(context.interactiveBackgroundPreview)
          );
        } else {
          tCtx.fillStyle = bgStyle;
          tCtx.fillRect(0, 0, canvasW, canvasH);
        }
      } else {
        tCtx.fillStyle = bgStyle;
        tCtx.fillRect(0, 0, canvasW, canvasH);
      }
      tCtx.drawImage(tempCanvas, 0, 0, canvasW, canvasH);
      tCtx.restore();

      if (subCur && showCursor) {
        tCtx.save();
        tCtx.setTransform(1, 0, 0, 1, 0, 0);
        tCtx.globalAlpha = cursorVis.opacity;
        const sp = cursorScreenPos(subCur, subZoom);
        const cScale = (backgroundConfig.cursorScale || 2) * sizeRatio * (subZoom?.zoomFactor || 1) * cursorVis.scale;
        drawMouseCursor(
          tCtx as unknown as CanvasRenderingContext2D, sp.x, sp.y,
          interpolatedPosition!.isClicked,
          cScale,
          resolveCursorRenderType(subCur.cursor_type || 'default', backgroundConfig, Boolean(subCur.isClicked)),
          subCur.cursor_rotation || 0,
          state.cursorImages,
          state.cursorState,
          backgroundConfig
        );
        tCtx.restore();
      }
    };

    // --- Motion blur detection ---
    const blurZoomVal = backgroundConfig.motionBlurZoom ?? 10;
    const blurPanVal = backgroundConfig.motionBlurPan ?? 10;
    const blurCursorVal = backgroundConfig.motionBlurCursor ?? 25;
    const maxBlurVal = Math.max(blurZoomVal, blurPanVal, blurCursorVal) / 100.0;
    const anyBlurEnabled = maxBlurVal > 0.0001;

    const exportStep = 1 / 60;
    const zoomShutterSec = (blurZoomVal / 100.0) * exportStep;
    const panShutterSec = (blurPanVal / 100.0) * exportStep;
    const cursorShutterSec = (blurCursorVal / 100.0) * exportStep;
    const maxShutterSec = Math.max(zoomShutterSec, panShutterSec, cursorShutterSec);

    const targetSamples = anyBlurEnabled ? Math.max(2, Math.min(8, Math.ceil(maxBlurVal * 8.0))) : 1;
    const N = targetSamples;

    let cameraMoving = false;
    let cursorMoving = false;
    if (anyBlurEnabled && maxShutterSec > 0) {
      const halfShutter = maxShutterSec / 2;
      const t0 = video.currentTime - halfShutter;
      const t1 = video.currentTime + halfShutter;
      if (blurZoomVal > 0 || blurPanVal > 0) {
        const z0 = state.calculateCurrentZoomState(t0, segment, canvasW, canvasH, srcW, srcH, scale);
        const z1 = state.calculateCurrentZoomState(t1, segment, canvasW, canvasH, srcW, srcH, scale);
        if (z0 && z1) {
          if (blurZoomVal > 0 && Math.abs(z0.zoomFactor - z1.zoomFactor) > 0.002) cameraMoving = true;
          if (blurPanVal > 0 && (Math.abs(z0.positionX - z1.positionX) > 0.001 || Math.abs(z0.positionY - z1.positionY) > 0.001)) cameraMoving = true;
        }
      }
      if (blurCursorVal > 0 && shouldRenderCustomCursor && interpolatedPosition) {
        const delay = getCursorMovementDelaySec(backgroundConfig);
        const c0 = interpolateCursorPosition(t0 + delay, mousePositions, state, vidW, vidH, backgroundConfig);
        const c1 = interpolateCursorPosition(t1 + delay, mousePositions, state, vidW, vidH, backgroundConfig);
        if (c0 && c1 && Math.hypot(c1.x - c0.x, c1.y - c0.y) > 1.0) cursorMoving = true;
      }
    }

    ctx.save();

    if (cameraMoving && N > 1) {
      if (!state.blurAccumCanvas || state.blurAccumCanvas.width !== canvasW || state.blurAccumCanvas.height !== canvasH) {
        state.blurAccumCanvas = new OffscreenCanvas(canvasW, canvasH);
        state.blurAccumCtx = state.blurAccumCanvas.getContext('2d')!;
      }
      if (!state.blurSubCanvas || state.blurSubCanvas.width !== canvasW || state.blurSubCanvas.height !== canvasH) {
        state.blurSubCanvas = new OffscreenCanvas(canvasW, canvasH);
        state.blurSubCtx = state.blurSubCanvas.getContext('2d')!;
      }
      const aCtx = state.blurAccumCtx!;
      const sCtx = state.blurSubCtx!;
      aCtx.clearRect(0, 0, canvasW, canvasH);

      for (let i = 0; i < N; i++) {
        const f = N > 1 ? i / (N - 1) : 0.5;
        const cameraZoomSubT = video.currentTime - (zoomShutterSec / 2) + f * zoomShutterSec;
        const cameraPanSubT = video.currentTime - (panShutterSec / 2) + f * panShutterSec;
        const cursorSubT = video.currentTime + getCursorMovementDelaySec(backgroundConfig) - (cursorShutterSec / 2) + f * cursorShutterSec;

        const zState = state.calculateCurrentZoomState(cameraZoomSubT, segment, canvasW, canvasH, srcW, srcH, scale);
        const pState = state.calculateCurrentZoomState(cameraPanSubT, segment, canvasW, canvasH, srcW, srcH, scale);
        const subZoom: ZoomKeyframe | null = zState ? {
          ...zState,
          zoomFactor: blurZoomVal > 0 ? zState.zoomFactor : (zoomState?.zoomFactor ?? 1),
          positionX: blurPanVal > 0 && pState ? pState.positionX : (zoomState?.positionX ?? 0.5),
          positionY: blurPanVal > 0 && pState ? pState.positionY : (zoomState?.positionY ?? 0.5),
        } : zoomState;

        const subCur = cursorMoving
          ? interpolateCursorPosition(cursorSubT, mousePositions, state, vidW, vidH, backgroundConfig)
          : interpolatedPosition;

        sCtx.clearRect(0, 0, canvasW, canvasH);
        drawSubFrame(sCtx, subZoom, subCur);

        aCtx.save();
        aCtx.globalAlpha = 1 / (i + 1);
        aCtx.drawImage(state.blurSubCanvas!, 0, 0);
        aCtx.restore();
      }

      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.drawImage(state.blurAccumCanvas, 0, 0);

    } else if (cursorMoving && showCursor && N > 1) {
      // --- CURSOR-ONLY BLUR PATH: single video draw + multi-cursor ---
      drawSubFrame(ctx, zoomState, null);

      if (!state.blurAccumCanvas || state.blurAccumCanvas.width !== canvasW || state.blurAccumCanvas.height !== canvasH) {
        state.blurAccumCanvas = new OffscreenCanvas(canvasW, canvasH);
        state.blurAccumCtx = state.blurAccumCanvas.getContext('2d')!;
      }
      const aCtx = state.blurAccumCtx!;
      aCtx.clearRect(0, 0, canvasW, canvasH);

      for (let i = 0; i < N; i++) {
        const f = N > 1 ? i / (N - 1) : 0.5;
        const subCursorT = video.currentTime + getCursorMovementDelaySec(backgroundConfig) - (cursorShutterSec / 2) + f * cursorShutterSec;
        const subCur = interpolateCursorPosition(subCursorT, mousePositions, state, vidW, vidH, backgroundConfig);
        if (!subCur) continue;

        aCtx.save();
        aCtx.setTransform(1, 0, 0, 1, 0, 0);
        aCtx.globalCompositeOperation = 'lighter';
        aCtx.globalAlpha = cursorVis.opacity / N;
        const sp = cursorScreenPos(subCur, zoomState);
        const cScale = (backgroundConfig.cursorScale || 2) * sizeRatio * (zoomState?.zoomFactor || 1) * cursorVis.scale;
        drawMouseCursor(
          aCtx as unknown as CanvasRenderingContext2D, sp.x, sp.y,
          interpolatedPosition!.isClicked, cScale,
          resolveCursorRenderType(subCur.cursor_type || 'default', backgroundConfig, Boolean(subCur.isClicked)),
          subCur.cursor_rotation || 0,
          state.cursorImages,
          state.cursorState,
          backgroundConfig
        );
        aCtx.restore();
      }

      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.drawImage(state.blurAccumCanvas, 0, 0);

    } else {
      // --- NO BLUR PATH: single draw ---
      drawSubFrame(ctx, zoomState, interpolatedPosition);
    }

    drawWebcamOverlay(ctx, zoomState, state, video, webcamVideo, segment, webcamConfig, canvasW, canvasH, webcamAspectRatio);

    if (segment.textSegments) {
      const FADE_DURATION = 0.3;
      for (const textSegment of segment.textSegments) {
        if (video.currentTime >= textSegment.startTime && video.currentTime <= textSegment.endTime) {
          let fadeAlpha = 1.0;
          const elapsed = video.currentTime - textSegment.startTime;
          const remaining = textSegment.endTime - video.currentTime;
          if (elapsed < FADE_DURATION) fadeAlpha = elapsed / FADE_DURATION;
          if (remaining < FADE_DURATION) fadeAlpha = Math.min(fadeAlpha, remaining / FADE_DURATION);
          drawTextOverlay(ctx, textSegment, canvas.width, canvas.height, fadeAlpha);
        }
      }
      canvas.style.fontVariationSettings = 'normal';
    }

    const segmentDuration = Math.max(
      segment.trimEnd,
      ...(segment.trimSegments || []).map((trimSegment) => trimSegment.endTime),
      video.duration || segment.trimEnd || 0
    );
    drawActiveKeystrokeOverlays(
      state.keystrokeState,
      ctx,
      segment,
      video.currentTime,
      canvas.width,
      canvas.height,
      segmentDuration
    );

  } finally {
    state.isDrawing = false;
    ctx.restore();
  }
}
