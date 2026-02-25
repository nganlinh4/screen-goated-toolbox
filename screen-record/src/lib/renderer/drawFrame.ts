import { BackgroundConfig, MousePosition, VideoSegment, ZoomKeyframe } from '@/types/video';
import { getCursorVisibility } from '@/lib/cursorHiding';
import {
  getCursorMovementDelaySec,
  getCursorProcessingSignature,
  processCursorPositions,
  interpolateCursorPositionInternal,
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
  fillGradient4Background,
  fillGradient5Background,
  fillGradient6Background,
  fillGradient7Background,
  GRADIENT4_STYLE_TOKEN,
  GRADIENT5_STYLE_TOKEN,
  GRADIENT6_STYLE_TOKEN,
  GRADIENT7_STYLE_TOKEN,
} from './gradientGenerator';
import {
  KeystrokeState,
  drawActiveKeystrokeOverlays,
} from './keystrokeRenderer';
import {
  drawTextOverlay,
} from './overlayBaker';
import type { RenderContext, RenderOptions } from './index';

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
  lastHoldTime: number;

  // Motion blur canvases (reused across frames)
  blurAccumCanvas: OffscreenCanvas | null;
  blurAccumCtx: OffscreenCanvasRenderingContext2D | null;
  blurSubCanvas: OffscreenCanvas | null;
  blurSubCtx: OffscreenCanvasRenderingContext2D | null;

  // Timing
  isDrawing: boolean;
  lastDrawTime: number;
  latestElapsed: number;

  // Cursor processing cache
  processedCursorPositions: MousePosition[] | null;
  lastMousePositionsRef: MousePosition[] | null;
  lastCursorProcessSignature: string;

  // Methods from VideoRenderer that drawFrame delegates to
  calculateCurrentZoomState: (
    currentTime: number,
    segment: VideoSegment,
    viewW: number,
    viewH: number,
    srcCropW?: number,
    srcCropH?: number,
  ) => ZoomKeyframe;
  requestRedraw: () => void;
}

// Constants
const CLICK_FUSE_THRESHOLD = 0.15;
const SQUISH_SPEED = 0.015;
const RELEASE_SPEED = 0.01;

// ---------------------------------------------------------------------------
// interpolateCursorPosition - cached wrapper around cursorDynamics
// ---------------------------------------------------------------------------

function interpolateCursorPosition(
  currentTime: number,
  mousePositions: MousePosition[],
  state: RendererState,
  backgroundConfig?: BackgroundConfig | null,
): { x: number; y: number; isClicked: boolean; cursor_type: string; cursor_rotation?: number } | null {
  const processSignature = getCursorProcessingSignature(backgroundConfig);

  if (state.lastMousePositionsRef !== mousePositions || state.lastCursorProcessSignature !== processSignature) {
    state.processedCursorPositions = null;
    state.lastMousePositionsRef = mousePositions;
    state.lastCursorProcessSignature = processSignature;
  }

  if (!state.processedCursorPositions && mousePositions.length > 0) {
    state.processedCursorPositions = processCursorPositions(mousePositions, backgroundConfig);
  }

  const dataToUse = state.processedCursorPositions || mousePositions;
  return interpolateCursorPositionInternal(currentTime, dataToUse);
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

  const { video, canvas, tempCanvas, segment, backgroundConfig, mousePositions } = context;
  if (!video || !canvas || !segment) return;
  if (video.readyState < 2) return;
  if (video.seeking) return;

  const isExportMode = options.exportMode || false;
  const quality = options.highQuality || isExportMode ? 'high' : 'medium';

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

  if (!vidW || !vidH) {
    state.isDrawing = false;
    return;
  }

  const crop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
  const srcX = vidW * crop.x;
  const srcY = vidH * crop.y;
  const srcW = vidW * crop.width;
  const srcH = vidH * crop.height;

  const useCustomCanvas = backgroundConfig.canvasMode === 'custom' && backgroundConfig.canvasWidth && backgroundConfig.canvasHeight;
  const canvasW = useCustomCanvas ? backgroundConfig.canvasWidth! : Math.round(srcW);
  const canvasH = useCustomCanvas ? backgroundConfig.canvasHeight! : Math.round(srcH);

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

    const effectiveSrcH = srcH * (1 - legacyCrop);
    const srcAspect = srcW / effectiveSrcH;
    const canvasAspect = canvasW / canvasH;
    let fitW: number, fitH: number;
    if (srcAspect > canvasAspect) {
      fitW = canvasW;
      fitH = canvasW / srcAspect;
    } else {
      fitH = canvasH;
      fitW = canvasH * srcAspect;
    }
    const scaledWidth = fitW * scale;
    const scaledHeight = fitH * scale;
    const x = (canvasW - scaledWidth) / 2;
    const y = (canvasH - scaledHeight) / 2;

    const zoomState = state.calculateCurrentZoomState(video.currentTime, segment, canvas.width, canvas.height, srcW, srcH);

    // Supersample to keep zoom crisp
    const zf = zoomState?.zoomFactor ?? 1;
    const bgScale = Math.max(0.01, backgroundConfig.scale / 100);
    let ss = 1;

    if (isExportMode) {
      ss = zf > 1 ? Math.min(Math.ceil(zf / bgScale), 4) : 1;
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
    const interpolatedPosition = interpolateCursorPosition(cursorTime, mousePositions, state, backgroundConfig);
    const cursorVis = getCursorVisibility(video.currentTime, segment.cursorVisibilitySegments);
    const shouldRenderCustomCursor = segment.useCustomCursor !== false;
    const showCursor = shouldRenderCustomCursor && interpolatedPosition && cursorVis.opacity > 0.001;

    if (showCursor) {
      const isActuallyClicked = interpolatedPosition!.isClicked;
      const timeSinceLastHold = video.currentTime - state.lastHoldTime;
      const shouldBeSquished = isActuallyClicked || (state.lastHoldTime >= 0 && timeSinceLastHold < CLICK_FUSE_THRESHOLD && timeSinceLastHold > 0);
      if (isActuallyClicked) state.lastHoldTime = video.currentTime;
      const targetScale = shouldBeSquished ? 0.75 : 1.0;
      if (state.currentSquishScale > targetScale) {
        state.currentSquishScale = Math.max(targetScale, state.currentSquishScale - SQUISH_SPEED * (state.latestElapsed / (1000 / 120)));
      } else if (state.currentSquishScale < targetScale) {
        state.currentSquishScale = Math.min(targetScale, state.currentSquishScale + RELEASE_SPEED * (state.latestElapsed / (1000 / 120)));
      }
    }

    const bgStyle = getBackgroundStyle(ctx, backgroundConfig.backgroundType, state.customBgCache, () => {
      state.requestRedraw();
    }, backgroundConfig.customBackground);
    const sizeRatio = Math.min(canvas.width / srcW, canvas.height / srcH);

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
      if (bgStyle === GRADIENT4_STYLE_TOKEN) {
        fillGradient4Background(state.gradientCache, tCtx, canvasW, canvasH);
      } else if (bgStyle === GRADIENT5_STYLE_TOKEN) {
        fillGradient5Background(state.gradientCache, tCtx, canvasW, canvasH);
      } else if (bgStyle === GRADIENT6_STYLE_TOKEN) {
        fillGradient6Background(state.gradientCache, tCtx, canvasW, canvasH);
      } else if (bgStyle === GRADIENT7_STYLE_TOKEN) {
        fillGradient7Background(state.gradientCache, tCtx, canvasW, canvasH);
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

    const isPlaying = !video.paused;
    const targetSamples = anyBlurEnabled ? Math.max(2, Math.min(8, Math.ceil(maxBlurVal * 8.0))) : 1;
    const N = isExportMode ? targetSamples : (isPlaying ? Math.min(targetSamples, 3) : targetSamples);

    let cameraMoving = false;
    let cursorMoving = false;
    if (anyBlurEnabled && maxShutterSec > 0) {
      const halfShutter = maxShutterSec / 2;
      const t0 = video.currentTime - halfShutter;
      const t1 = video.currentTime + halfShutter;
      if (blurZoomVal > 0 || blurPanVal > 0) {
        const z0 = state.calculateCurrentZoomState(t0, segment, canvasW, canvasH, srcW, srcH);
        const z1 = state.calculateCurrentZoomState(t1, segment, canvasW, canvasH, srcW, srcH);
        if (z0 && z1) {
          if (blurZoomVal > 0 && Math.abs(z0.zoomFactor - z1.zoomFactor) > 0.002) cameraMoving = true;
          if (blurPanVal > 0 && (Math.abs(z0.positionX - z1.positionX) > 0.001 || Math.abs(z0.positionY - z1.positionY) > 0.001)) cameraMoving = true;
        }
      }
      if (blurCursorVal > 0 && shouldRenderCustomCursor && interpolatedPosition) {
        const delay = getCursorMovementDelaySec(backgroundConfig);
        const c0 = interpolateCursorPosition(t0 + delay, mousePositions, state, backgroundConfig);
        const c1 = interpolateCursorPosition(t1 + delay, mousePositions, state, backgroundConfig);
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

        const zState = state.calculateCurrentZoomState(cameraZoomSubT, segment, canvasW, canvasH, srcW, srcH);
        const pState = state.calculateCurrentZoomState(cameraPanSubT, segment, canvasW, canvasH, srcW, srcH);
        const subZoom: ZoomKeyframe | null = zState ? {
          ...zState,
          zoomFactor: blurZoomVal > 0 ? zState.zoomFactor : (zoomState?.zoomFactor ?? 1),
          positionX: blurPanVal > 0 && pState ? pState.positionX : (zoomState?.positionX ?? 0.5),
          positionY: blurPanVal > 0 && pState ? pState.positionY : (zoomState?.positionY ?? 0.5),
        } : zoomState;

        const subCur = cursorMoving
          ? interpolateCursorPosition(cursorSubT, mousePositions, state, backgroundConfig)
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
        const subCur = interpolateCursorPosition(subCursorT, mousePositions, state, backgroundConfig);
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


    if (segment.textSegments) {
      const FADE_DURATION = 0.3;
      const isTextPlaying = !video.paused;
      for (const textSegment of segment.textSegments) {
        if (video.currentTime >= textSegment.startTime && video.currentTime <= textSegment.endTime) {
          let fadeAlpha = 1.0;
          if (isTextPlaying) {
            const elapsed = video.currentTime - textSegment.startTime;
            const remaining = textSegment.endTime - video.currentTime;
            if (elapsed < FADE_DURATION) fadeAlpha = elapsed / FADE_DURATION;
            if (remaining < FADE_DURATION) fadeAlpha = Math.min(fadeAlpha, remaining / FADE_DURATION);
          }
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
