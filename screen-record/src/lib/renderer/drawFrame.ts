import { ZoomKeyframe } from '@/types/video';
import { getCursorVisibility } from '@/lib/cursorHiding';
import {
  getCursorMovementDelaySec,
} from './cursorDynamics';
import {
  resolveCursorRenderType,
  drawMouseCursor,
} from './cursorGraphics';
import {
  getBackgroundStyle,
  fillBuiltInBackground,
  parseBuiltInBackgroundToken,
} from './gradientGenerator';
import {
  drawActiveKeystrokeOverlays,
} from './keystrokeRenderer';
import {
  drawTextOverlay,
} from './overlayTextRenderer';
import type { RenderContext, RenderOptions } from './index';
import { getVisibleSubtitleSegments } from '@/lib/subtitleTracks';
import {
  getLogicalCropSize,
  sampleCaptureDimensionsAtTime,
} from '@/lib/dynamicCapture';
import {
  getVideoPlacementRect,
  resolveCodecAlignedCropGeometry,
} from '@/lib/videoGeometry';
import { drawWebcamOverlay } from './drawFrameWebcam';
import { getActiveTimedSegments } from '@/lib/timedSegmentIndex';
import {
  getBoundedCanvasSize,
  getBoundedCanvasScale,
  MAX_QUALITY_TEMP_PIXELS,
  MAX_REALTIME_TEMP_PIXELS,
  resolveOutputCanvasDimensions,
} from '@/lib/canvasRenderBudget';
import { resolveMotionBlurTiming } from './motionBlurTiming';
import type { RendererState } from './rendererState';
import {
  interpolateCursorPosition,
  logPreviewCursorDebug,
  updateSquishAnimation,
} from './drawFrameCursor';
import { isUnmaskedCanvasCoveringVideo, traceVideoFramePath } from './videoFrameSurface';

// ---------------------------------------------------------------------------
// drawFrame - main rendering entry point
// ---------------------------------------------------------------------------

export async function drawFrame(
  context: RenderContext,
  options: RenderOptions,
  state: RendererState,
): Promise<void> {
  const isExportMode = options.exportMode || false;
  if (state.isDrawing) {
    void isExportMode;
    return;
  }

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
  const isTimelineOnly = segment.mediaMode === 'timelineOnly';
  if (!isTimelineOnly && video.readyState < 2) return;
  if (!isTimelineOnly && video.seeking) return;
  const frameTime = isTimelineOnly ? context.currentTime : video.currentTime;

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

  const timelineOnlyDimensions = resolveOutputCanvasDimensions(
    backgroundConfig.canvasWidth ?? 1920,
    backgroundConfig.canvasHeight ?? 1080,
  );
  const vidW = isTimelineOnly
    ? timelineOnlyDimensions.width
    : video.videoWidth;
  const vidH = isTimelineOnly
    ? timelineOnlyDimensions.height
    : video.videoHeight;
  const webcamAspectRatio =
    webcamVideo && webcamVideo.videoWidth > 0 && webcamVideo.videoHeight > 0
      ? webcamVideo.videoWidth / webcamVideo.videoHeight
      : null;

  if (!vidW || !vidH) {
    state.isDrawing = false;
    return;
  }

  const requestedCrop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
  const autoGeometry = backgroundConfig.canvasMode !== 'custom'
    ? resolveCodecAlignedCropGeometry(vidW, vidH, requestedCrop)
    : null;
  const crop = autoGeometry?.crop ?? requestedCrop;
  const renderSegment = autoGeometry ? { ...segment, crop } : segment;
  const srcX = vidW * crop.x;
  const srcY = vidH * crop.y;
  const srcW = vidW * crop.width;
  const srcH = vidH * crop.height;

  const useExplicitCanvas =
    backgroundConfig.canvasMode === 'custom' &&
    backgroundConfig.canvasWidth &&
    backgroundConfig.canvasHeight;
  const requestedCanvasW = useExplicitCanvas
    ? backgroundConfig.canvasWidth!
    : (autoGeometry?.width ?? Math.round(srcW));
  const requestedCanvasH = useExplicitCanvas
    ? backgroundConfig.canvasHeight!
    : (autoGeometry?.height ?? Math.round(srcH));
  const { width: canvasW, height: canvasH } = resolveOutputCanvasDimensions(
    requestedCanvasW,
    requestedCanvasH,
  );

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
      frameTime,
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
    const contained = getVideoPlacementRect(
      canvasW,
      canvasH,
      logicalCrop.width,
      logicalCrop.height,
      scale,
    );
    const scaledWidth = contained.width;
    const scaledHeight = contained.height;
    const x = contained.left;
    const y = contained.top;

    const zoomState = state.calculateCurrentZoomState(
      frameTime, renderSegment, canvas.width, canvas.height, srcW, srcH, scale,
    );

    // Supersample to keep zoom crisp
    const zf = zoomState?.zoomFactor ?? 1;
    const bgScale = Math.max(0.01, backgroundConfig.scale / 100);
    const fullQualitySs = zf > 1 ? Math.min(Math.ceil(zf / bgScale), 4) : 1;
    const isRealtimePreview = !isExportMode && !isTimelineOnly && !video.paused;
    let requestedSs = fullQualitySs;
    if (isRealtimePreview) {
      const requiredSs = zf / bgScale;
      requestedSs = requiredSs > 1.05 ? Math.min(requiredSs, 2.5) : 1;
    }
    const ss = getBoundedCanvasScale(
      canvasW,
      canvasH,
      requestedSs,
      isRealtimePreview ? MAX_REALTIME_TEMP_PIXELS : MAX_QUALITY_TEMP_PIXELS,
      isRealtimePreview ? 4096 : 8192,
    );
    // --- Prepare tempCanvas (video + shadow + border radius) ---
    const tempW = Math.max(1, Math.round(canvasW * ss));
    const tempH = Math.max(1, Math.round(canvasH * ss));
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
    if (Math.abs(ss - 1) > 0.0001) tempCtx.scale(ss, ss);

    const radius = backgroundConfig.borderRadius;
    const frameRect = { left: x, top: y, width: scaledWidth, height: scaledHeight };
    const unmaskedCanvasCover = isUnmaskedCanvasCoveringVideo(
      frameRect,
      canvasW,
      canvasH,
      radius,
    );

    if (backgroundConfig.shadow && !unmaskedCanvasCover) {
      tempCtx.save();
      tempCtx.shadowColor = 'rgba(0, 0, 0, 0.5)';
      tempCtx.shadowBlur = backgroundConfig.shadow * ss;
      tempCtx.shadowOffsetY = backgroundConfig.shadow * 0.5 * ss;
      traceVideoFramePath(tempCtx, frameRect, radius);
      tempCtx.fillStyle = '#fff';
      tempCtx.fill();
      tempCtx.restore();
    }

    if (!unmaskedCanvasCover) {
      traceVideoFramePath(tempCtx, frameRect, radius);
      tempCtx.clip();
    }

    if (!isTimelineOnly) {
      try {
        tempCtx.drawImage(
          video,
          srcX, srcY, srcW, srcH * (1 - legacyCrop),
          x, y, scaledWidth, scaledHeight
        );
      } catch {
        // A media frame can be unavailable briefly while its source is changing.
      }
    }

    if (!unmaskedCanvasCover) {
      tempCtx.strokeStyle = 'rgba(0, 0, 0, 0.1)';
      tempCtx.lineWidth = 1;
      tempCtx.stroke();
    }
    tempCtx.restore();

    // --- Compute cursor state (squish, visibility) once per frame ---
    const cursorTime = frameTime + getCursorMovementDelaySec(backgroundConfig);
    const interpolatedPosition = interpolateCursorPosition(
      cursorTime,
      mousePositions,
      state,
      vidW,
      vidH,
      backgroundConfig
    );
    const cursorVis = getCursorVisibility(frameTime, segment.cursorVisibilitySegments);
    const shouldRenderCustomCursor = segment.useCustomCursor !== false;
    const showCursor = Boolean(shouldRenderCustomCursor && interpolatedPosition && cursorVis.opacity > 0.001);
    if (!isExportMode) {
      logPreviewCursorDebug(
        state,
        frameTime,
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

    const backgroundFollowsZoom = backgroundConfig.backgroundZoomWithVideo !== false;
    const drawBackground = (
      targetCtx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
    ) => {
      if (typeof bgStyle === 'string') {
        const builtInBackgroundId = parseBuiltInBackgroundToken(bgStyle);
        if (builtInBackgroundId) {
          fillBuiltInBackground(
            state.gradientCache,
            targetCtx,
            builtInBackgroundId,
            canvasW,
            canvasH,
            Boolean(context.interactiveBackgroundPreview)
          );
        } else {
          targetCtx.fillStyle = bgStyle;
          targetCtx.fillRect(0, 0, canvasW, canvasH);
        }
      } else {
        targetCtx.fillStyle = bgStyle;
        targetCtx.fillRect(0, 0, canvasW, canvasH);
      }
    };

    // Helper: draw one composited sub-frame (background + video + cursor)
    const drawSubFrame = (
      tCtx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
      subZoom: ZoomKeyframe | null,
      subCur: { x: number; y: number; isClicked: boolean; cursor_type: string; cursor_rotation?: number } | null,
      renderScale = 1,
    ) => {
      tCtx.save();
      tCtx.setTransform(renderScale, 0, 0, renderScale, 0, 0);
      if (!backgroundFollowsZoom) {
        drawBackground(tCtx);
      }
      if (subZoom && subZoom.zoomFactor !== 1) {
        const zW = canvasW * subZoom.zoomFactor;
        const zH = canvasH * subZoom.zoomFactor;
        tCtx.translate((canvasW - zW) * subZoom.positionX, (canvasH - zH) * subZoom.positionY);
        tCtx.scale(subZoom.zoomFactor, subZoom.zoomFactor);
      }
      if (backgroundFollowsZoom) {
        drawBackground(tCtx);
      }
      if (!isTimelineOnly) {
        tCtx.drawImage(tempCanvas, 0, 0, canvasW, canvasH);
      }
      tCtx.restore();

      if (subCur && showCursor) {
        tCtx.save();
        tCtx.setTransform(renderScale, 0, 0, renderScale, 0, 0);
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
    const blurTiming = resolveMotionBlurTiming(
      segment,
      frameTime,
      context.outputFrameRate ?? 60,
      blurZoomVal,
      blurPanVal,
      blurCursorVal,
    );
    const {
      zoomEnabled,
      panEnabled,
      cursorEnabled,
      zoomShutterSec,
      panShutterSec,
      cursorShutterSec,
      maxShutterSec,
      sampleCount: N,
    } = blurTiming;
    const anyBlurEnabled = N > 1;

    let cameraMoving = false;
    let cursorMoving = false;
    if (anyBlurEnabled && maxShutterSec > 0) {
      const halfShutter = maxShutterSec / 2;
      const t0 = frameTime - halfShutter;
      const t1 = frameTime + halfShutter;
      if (zoomEnabled || panEnabled) {
        const z0 = state.calculateCurrentZoomState(t0, renderSegment, canvasW, canvasH, srcW, srcH, scale);
        const z1 = state.calculateCurrentZoomState(t1, renderSegment, canvasW, canvasH, srcW, srcH, scale);
        if (z0 && z1) {
          if (zoomEnabled && Math.abs(z0.zoomFactor - z1.zoomFactor) > 0.002) cameraMoving = true;
          if (panEnabled && (Math.abs(z0.positionX - z1.positionX) > 0.001 || Math.abs(z0.positionY - z1.positionY) > 0.001)) cameraMoving = true;
        }
      }
      if (cursorEnabled && shouldRenderCustomCursor && interpolatedPosition) {
        const delay = getCursorMovementDelaySec(backgroundConfig);
        const c0 = interpolateCursorPosition(t0 + delay, mousePositions, state, vidW, vidH, backgroundConfig);
        const c1 = interpolateCursorPosition(t1 + delay, mousePositions, state, vidW, vidH, backgroundConfig);
        if (c0 && c1 && Math.hypot(c1.x - c0.x, c1.y - c0.y) > 1.0) cursorMoving = true;
      }
    }

    ctx.save();
    const blurSurface = getBoundedCanvasSize(
      canvasW,
      canvasH,
      MAX_REALTIME_TEMP_PIXELS,
      4096,
    );

    if (cameraMoving && N > 1) {
      if (!state.blurAccumCanvas || state.blurAccumCanvas.width !== blurSurface.width || state.blurAccumCanvas.height !== blurSurface.height) {
        state.blurAccumCanvas = new OffscreenCanvas(blurSurface.width, blurSurface.height);
        state.blurAccumCtx = state.blurAccumCanvas.getContext('2d')!;
      }
      if (!state.blurSubCanvas || state.blurSubCanvas.width !== blurSurface.width || state.blurSubCanvas.height !== blurSurface.height) {
        state.blurSubCanvas = new OffscreenCanvas(blurSurface.width, blurSurface.height);
        state.blurSubCtx = state.blurSubCanvas.getContext('2d')!;
      }
      const aCtx = state.blurAccumCtx!;
      const sCtx = state.blurSubCtx!;
      aCtx.setTransform(1, 0, 0, 1, 0, 0);
      aCtx.clearRect(0, 0, blurSurface.width, blurSurface.height);

      for (let i = 0; i < N; i++) {
        const f = N > 1 ? i / (N - 1) : 0.5;
        const cameraZoomSubT = frameTime - (zoomShutterSec / 2) + f * zoomShutterSec;
        const cameraPanSubT = frameTime - (panShutterSec / 2) + f * panShutterSec;
        const cursorSubT = frameTime + getCursorMovementDelaySec(backgroundConfig) - (cursorShutterSec / 2) + f * cursorShutterSec;

        const zState = state.calculateCurrentZoomState(cameraZoomSubT, renderSegment, canvasW, canvasH, srcW, srcH, scale);
        const pState = state.calculateCurrentZoomState(cameraPanSubT, renderSegment, canvasW, canvasH, srcW, srcH, scale);
        const subZoom: ZoomKeyframe | null = zState ? {
          ...zState,
          zoomFactor: zoomEnabled ? zState.zoomFactor : (zoomState?.zoomFactor ?? 1),
          positionX: panEnabled && pState ? pState.positionX : (zoomState?.positionX ?? 0.5),
          positionY: panEnabled && pState ? pState.positionY : (zoomState?.positionY ?? 0.5),
        } : zoomState;

        const subCur = cursorMoving
          ? interpolateCursorPosition(cursorSubT, mousePositions, state, vidW, vidH, backgroundConfig)
          : interpolatedPosition;

        sCtx.setTransform(1, 0, 0, 1, 0, 0);
        sCtx.clearRect(0, 0, blurSurface.width, blurSurface.height);
        drawSubFrame(sCtx, subZoom, subCur, blurSurface.scale);

        aCtx.save();
        aCtx.globalAlpha = 1 / (i + 1);
        aCtx.drawImage(state.blurSubCanvas!, 0, 0);
        aCtx.restore();
      }

      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.drawImage(state.blurAccumCanvas, 0, 0, canvasW, canvasH);

    } else if (cursorMoving && showCursor && N > 1) {
      // --- CURSOR-ONLY BLUR PATH: single video draw + multi-cursor ---
      drawSubFrame(ctx, zoomState, null);

      if (!state.blurAccumCanvas || state.blurAccumCanvas.width !== blurSurface.width || state.blurAccumCanvas.height !== blurSurface.height) {
        state.blurAccumCanvas = new OffscreenCanvas(blurSurface.width, blurSurface.height);
        state.blurAccumCtx = state.blurAccumCanvas.getContext('2d')!;
      }
      const aCtx = state.blurAccumCtx!;
      aCtx.setTransform(1, 0, 0, 1, 0, 0);
      aCtx.clearRect(0, 0, blurSurface.width, blurSurface.height);

      for (let i = 0; i < N; i++) {
        const f = N > 1 ? i / (N - 1) : 0.5;
        const subCursorT = frameTime + getCursorMovementDelaySec(backgroundConfig) - (cursorShutterSec / 2) + f * cursorShutterSec;
        const subCur = interpolateCursorPosition(subCursorT, mousePositions, state, vidW, vidH, backgroundConfig);
        if (!subCur) continue;

        aCtx.save();
        aCtx.setTransform(blurSurface.scale, 0, 0, blurSurface.scale, 0, 0);
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
      ctx.drawImage(state.blurAccumCanvas, 0, 0, canvasW, canvasH);

    } else {
      // --- NO BLUR PATH: single draw ---
      drawSubFrame(ctx, zoomState, interpolatedPosition);
    }
    drawWebcamOverlay(ctx, zoomState, state, video, webcamVideo, segment, webcamConfig, canvasW, canvasH, webcamAspectRatio);

    const overlayTextSegments = [
      ...getActiveTimedSegments(getVisibleSubtitleSegments(segment), frameTime),
      ...getActiveTimedSegments(segment.textSegments, frameTime),
    ];
    if (overlayTextSegments.length > 0) {
      for (const textSegment of overlayTextSegments) {
        drawTextOverlay(ctx, textSegment, canvas.width, canvas.height, 1, frameTime);
      }
      canvas.style.fontVariationSettings = 'normal';
    }

    const segmentDuration = Math.max(
      segment.trimEnd,
      ...(segment.trimSegments || []).map((trimSegment) => trimSegment.endTime),
      (!isTimelineOnly ? video.duration : 0) || segment.trimEnd || 0
    );
    drawActiveKeystrokeOverlays(
      state.keystrokeState,
      ctx,
      segment,
      frameTime,
      canvas.width,
      canvas.height,
      segmentDuration
    );

  } finally {
    state.isDrawing = false;
    ctx.restore();
  }
}
