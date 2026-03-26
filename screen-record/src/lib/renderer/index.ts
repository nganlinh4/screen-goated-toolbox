import {
  BackgroundConfig,
  MousePosition,
  VideoSegment,
  ZoomKeyframe,
  BakedCameraFrame,
  BakedCursorFrame,
  BakedOverlayPayload,
  BakedWebcamFrame,
  WebcamConfig,
} from '@/types/video';
import { getCursorVisibility } from '@/lib/cursorHiding';
import { getTrimSegments, toCompactTime } from '@/lib/trimSegments';
import {
  getCursorMovementDelaySec,
  processCursorPositions,
  interpolateCursorPositionInternal,
  shouldCursorRotate,
} from './cursorDynamics';
import {
  resolveCursorRenderType,
} from './cursorGraphics';
import {
  KeystrokeOverlayEditBounds,
  getKeystrokeOverlayConfig,
  getKeystrokeOverlayEditBounds,
} from './keystrokeRenderer';
import {
  TextDragState,
  handleMouseDown as textMouseDown,
  handleMouseMove as textMouseMove,
  handleMouseUp as textMouseUp,
  bakeOverlayAtlasAndPaths,
} from './overlayBaker';
import { calculateCurrentZoomStateInternal } from './cameraZoom';
import { createCursorImageSet, ensureCursorAnimations } from './cursorAssets';
import { drawFrame as drawFrameImpl, type RendererState } from './drawFrame';
import { normalizeMousePositionsToVideoSpace } from '@/lib/dynamicCapture';
import { buildBakedWebcamFrames, cloneWebcamConfig } from '@/lib/webcam';

// ---------------------------------------------------------------------------
// Public interfaces
// ---------------------------------------------------------------------------

export interface RenderContext {
  video: HTMLVideoElement;
  webcamVideo?: HTMLVideoElement | null;
  canvas: HTMLCanvasElement;
  tempCanvas: HTMLCanvasElement;
  segment: VideoSegment;
  backgroundConfig: BackgroundConfig;
  webcamConfig?: WebcamConfig;
  mousePositions: MousePosition[];
  currentTime: number;
  interactiveBackgroundPreview?: boolean;
}

export interface RenderOptions {
  exportMode?: boolean;
  highQuality?: boolean;
}

// Re-export needed types from submodules
export type { CursorRenderType } from './cursorGraphics';
export type { KeystrokeOverlayEditBounds } from './keystrokeRenderer';

// ---------------------------------------------------------------------------
// VideoRenderer class
// ---------------------------------------------------------------------------

class VideoRenderer {
  // --- Animation ---
  private animationFrame: number | null = null;
  private readonly FRAME_INTERVAL = 1000 / 120; // 120fps target

  // --- Renderer state (shared with drawFrame) ---
  private state: RendererState;

  // --- Zoom state ---
  private lastCalculatedState: ZoomKeyframe | null = null;
  private cachedBakedPath: BakedCameraFrame[] | null = null;
  private lastBakeSignature: string = '';
  private lastBakeSegment: VideoSegment | null = null;
  private lastBakeViewW: number = 0;
  private lastBakeViewH: number = 0;

  // --- Squish animation constants ---
  private readonly CLICK_FUSE_THRESHOLD = 0.15;
  private readonly SQUISH_SPEED = 0.015;
  private readonly RELEASE_SPEED = 0.01;

  // --- Text drag state ---
  private textDragState: TextDragState;

  // --- Active render context ---
  private activeRenderContext: RenderContext | null = null;

  constructor() {
    const cursorOffscreen = new OffscreenCanvas(128, 128);

    this.state = {
      cursorImages: createCursorImageSet(),
      cursorState: {
        cursorOffscreen,
        cursorOffscreenCtx: cursorOffscreen.getContext('2d')!,
        currentSquishScale: 1.0,
        loggedCursorTypes: new Set(),
        loggedCursorMappings: new Set(),
      },
      gradientCache: {
        renderedCanvasByKey: new Map(),
      },
      customBgCache: {
        customBackgroundPattern: null,
        lastCustomBackground: undefined,
        customBackgroundImage: null,
        customBackgroundCacheKey: undefined,
      },
      keystrokeState: {
        keystrokeLanguage: 'en',
        renderCache: {
          mode: 'off',
          segmentRef: null,
          eventsRef: null,
          visibilityRef: null,
          duration: 0,
          displayEvents: [],
          startTimes: [],
          effectiveEnds: [],
          keyboardStartTimes: [],
          keyboardIndices: [],
          mouseStartTimes: [],
          mouseIndices: [],
          keyboardMaxDuration: 0,
          mouseMaxDuration: 0,
          eventSlots: [],
          eventIdentities: [],
          keyboardSlotRepresentatives: [],
          mouseSlotRepresentatives: [],
        },
        layoutCache: new Map(),
      },
      currentSquishScale: 1.0,
      squishTarget: 1.0,
      squishAnimFrom: 1.0,
      squishAnimProgress: 1,
      squishAnimDuration: 0.15,
      squishHasRoom: true,
      lastHoldTime: -1,
      lastActiveEventId: null,
      blurAccumCanvas: null,
      blurAccumCtx: null,
      blurSubCanvas: null,
      blurSubCtx: null,
      webcamFrameCanvas: null,
      webcamFrameCtx: null,
      webcamFrameReady: false,
      isDrawing: false,
      lastDrawTime: 0,
      latestElapsed: 0,
      processedCursorPositions: null,
      lastMousePositionsRef: null,
      lastCursorProcessSignature: '',
      lastCursorNormalizationSignature: '',
      lastCursorPreviewDebugSignature: '',
      lastCursorPreviewDebugBucket: -1,
      lastCursorPreviewDebugPoint: null,
      calculateCurrentZoomState: (currentTime, segment, viewW, viewH, srcCropW?, srcCropH?) =>
        this.calculateCurrentZoomState(currentTime, segment, viewW, viewH, srcCropW, srcCropH),
      requestRedraw: () => {
        if (this.activeRenderContext) this.drawFrame(this.activeRenderContext);
      },
    };

    this.textDragState = {
      isDraggingText: false,
      draggedTextId: null,
      dragOffset: { x: 0, y: 0 },
    };
  }

  // ---------------------------------------------------------------------------
  // Public API: accessors
  // ---------------------------------------------------------------------------

  public getLastCalculatedState() { return this.lastCalculatedState; }

  public updateRenderContext(context: RenderContext) {
    this.activeRenderContext = context;
    // Lazy-init animated cursor frames only when the project has wait/appstarting moments.
    const pack = context.backgroundConfig.cursorPack ?? 'screenstudio';
    ensureCursorAnimations(pack, context.mousePositions, this.state.cursorImages);
  }

  // ---------------------------------------------------------------------------
  // Camera path generation (public, delegates to extracted cameraZoom module)
  // ---------------------------------------------------------------------------

  public generateBakedCursorPath(
    segment: VideoSegment,
    mousePositions: MousePosition[],
    backgroundConfig?: BackgroundConfig,
    fps: number = 60
  ): BakedCursorFrame[] {
    if (segment.useCustomCursor === false) {
      return [];
    }

    const baked: BakedCursorFrame[] = [];
    const step = 1 / fps;
    const duration = Math.max(segment.trimEnd, ...(segment.trimSegments || []).map(s => s.endTime));
    const trimSegments = getTrimSegments(segment, duration);

    const normalizedMousePositions = normalizeMousePositionsToVideoSpace(
      mousePositions,
      this.activeRenderContext?.video.videoWidth || 0,
      this.activeRenderContext?.video.videoHeight || 0
    );
    const processed = processCursorPositions(normalizedMousePositions, backgroundConfig);

    let simSquishScale = 1.0;
    let simLastHoldTime = -1;
    const simRatio = 2.0;

    const cursorOffsetSec = getCursorMovementDelaySec(backgroundConfig);

    const fullStart = trimSegments[0].startTime;
    const fullEnd = trimSegments[trimSegments.length - 1].endTime;

    for (let t = fullStart; t <= fullEnd + 0.00001; t += step) {
      const cursorT = t + cursorOffsetSec;
      const pos = interpolateCursorPositionInternal(cursorT, processed);

      if (!pos) {
        if (baked.length > 0) {
          const last = baked[baked.length - 1];
          baked.push({ ...last, time: t });
        } else {
          baked.push({
            time: t,
            x: 0,
            y: 0,
            scale: 1,
            isClicked: false,
            type: resolveCursorRenderType('default', backgroundConfig, false),
            opacity: 1
          });
        }
        continue;
      }

      const isClicked = pos.isClicked;
      const timeSinceLastHold = cursorT - simLastHoldTime;
      const shouldBeSquished = isClicked || (simLastHoldTime >= 0 && timeSinceLastHold < this.CLICK_FUSE_THRESHOLD && timeSinceLastHold > 0);

      if (isClicked) {
        simLastHoldTime = cursorT;
      }

      const targetScale = shouldBeSquished ? 0.75 : 1.0;

      if (simSquishScale > targetScale) {
        simSquishScale = Math.max(targetScale, simSquishScale - this.SQUISH_SPEED * simRatio);
      } else if (simSquishScale < targetScale) {
        simSquishScale = Math.min(targetScale, simSquishScale + this.RELEASE_SPEED * simRatio);
      }

      const cursorVis = getCursorVisibility(t, segment.cursorVisibilitySegments);
      const resolvedCursorType = resolveCursorRenderType(pos.cursor_type || 'default', backgroundConfig, Boolean(pos.isClicked));

      baked.push({
        time: t,
        x: pos.x,
        y: pos.y,
        scale: Number((simSquishScale * cursorVis.scale).toFixed(3)),
        isClicked: isClicked,
        type: resolvedCursorType,
        opacity: Number(cursorVis.opacity.toFixed(3)),
        rotation: shouldCursorRotate(resolvedCursorType) ? Number((pos.cursor_rotation || 0).toFixed(4)) : 0,
      });
    }

    return baked;
  }

  // --- BAKED CAMERA PATH GENERATION ---
  public generateBakedPath(
    segment: VideoSegment,
    videoWidth: number,
    videoHeight: number,
    fps: number = 60,
    srcCropW?: number,
    srcCropH?: number
  ): BakedCameraFrame[] {
    const t0 = performance.now();
    const bakedPath: BakedCameraFrame[] = [];
    const step = 1 / fps;
    const duration = Math.max(segment.trimEnd, ...(segment.trimSegments || []).map(s => s.endTime));
    const trimSegments = getTrimSegments(segment, duration);

    const crop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
    const croppedW = videoWidth * crop.width;
    const croppedH = videoHeight * crop.height;
    const cropOffsetX = videoWidth * crop.x;
    const cropOffsetY = videoHeight * crop.y;

    const fullStart = trimSegments[0].startTime;
    const fullEnd = trimSegments[trimSegments.length - 1].endTime;

    for (let t = fullStart; t <= fullEnd + 0.00001; t += step) {
      const state = calculateCurrentZoomStateInternal(t, segment, croppedW, croppedH, srcCropW, srcCropH);

      const globalX = cropOffsetX + (state.positionX * croppedW);
      const globalY = cropOffsetY + (state.positionY * croppedH);

      bakedPath.push({
        time: t,
        x: globalX,
        y: globalY,
        zoom: state.zoomFactor
      });
    }

    console.log(`[BakedPath] generateBakedPath: ${(performance.now() - t0).toFixed(1)}ms (${bakedPath.length} frames, ${duration.toFixed(1)}s)`);
    return bakedPath;
  }

  public generateBakedWebcamFrames(
    segment: VideoSegment,
    webcamConfig: WebcamConfig | null | undefined,
    outputWidth: number,
    outputHeight: number,
    webcamAspectRatio: number | null | undefined,
    fps: number = 60,
  ): BakedWebcamFrame[] {
    return buildBakedWebcamFrames(
      segment,
      cloneWebcamConfig(webcamConfig),
      outputWidth,
      outputHeight,
      webcamAspectRatio,
      (time) =>
        this.calculateCurrentZoomState(
          time,
          segment,
          outputWidth,
          outputHeight,
        ).zoomFactor,
      fps,
    );
  }

  public sampleZoomCurve(
    segment: VideoSegment,
    viewW: number,
    viewH: number,
    numSamples: number = 200
  ): Array<{ time: number; zoom: number; posX: number; posY: number }> {
    const samples: Array<{ time: number; zoom: number; posX: number; posY: number }> = [];
    const start = segment.trimStart;
    const end = segment.trimEnd;
    for (let i = 0; i <= numSamples; i++) {
      const t = start + (end - start) * (i / numSamples);
      const state = calculateCurrentZoomStateInternal(t, segment, viewW, viewH);
      samples.push({
        time: t - start,
        zoom: state.zoomFactor,
        posX: state.positionX,
        posY: state.positionY
      });
    }
    return samples;
  }

  // ---------------------------------------------------------------------------
  // Animation control
  // ---------------------------------------------------------------------------

  public startAnimation(renderContext: RenderContext) {
    this.stopAnimation();
    // Reset squish state unconditionally — stopAnimation() only resets when animationFrame
    // was running. A fresh startAnimation must always begin with cursor at full scale,
    // regardless of whether the previous session was playing or paused.
    this.state.lastHoldTime = -1;
    this.state.currentSquishScale = 1.0;
    this.state.cursorState.currentSquishScale = 1.0;
    this.state.squishTarget = 1.0;
    this.state.squishAnimFrom = 1.0;
    this.state.squishAnimProgress = 1;
    this.state.squishAnimDuration = 0.15;
    this.state.squishHasRoom = true;
    this.state.lastActiveEventId = null;
    this.state.lastDrawTime = 0;
    this.activeRenderContext = renderContext;
    const pack = renderContext.backgroundConfig.cursorPack ?? 'screenstudio';
    ensureCursorAnimations(pack, renderContext.mousePositions, this.state.cursorImages);

    const animate = () => {
      if (!this.activeRenderContext || this.activeRenderContext.video.paused) {
        this.animationFrame = null;
        return;
      }

      const now = performance.now();
      const elapsed = now - this.state.lastDrawTime;

      if (this.state.lastDrawTime === 0 || elapsed >= this.FRAME_INTERVAL) {
        this.drawFrame(this.activeRenderContext)
          .catch((err: unknown) => console.error('[VideoRenderer] Draw error:', err));
      }

      this.animationFrame = requestAnimationFrame(animate);
    };

    this.animationFrame = requestAnimationFrame(animate);
  }

  public stopAnimation() {
    if (this.animationFrame !== null) {
      cancelAnimationFrame(this.animationFrame);
      this.animationFrame = null;
      this.state.lastDrawTime = 0;
      this.activeRenderContext = null;
      this.state.lastHoldTime = -1;
      this.state.currentSquishScale = 1.0;
      this.state.cursorState.currentSquishScale = 1.0;
      this.state.squishTarget = 1.0;
      this.state.squishAnimFrom = 1.0;
      this.state.squishAnimProgress = 1;
      this.state.squishAnimDuration = 0.15;
      this.state.squishHasRoom = true;
      this.state.lastActiveEventId = null;
    }
  }

  // ---------------------------------------------------------------------------
  // Caching wrappers (instance-level cache management)
  // ---------------------------------------------------------------------------

  private calculateCurrentZoomState(
    currentTime: number,
    segment: VideoSegment,
    viewW: number,
    viewH: number,
    srcCropW?: number,
    srcCropH?: number
  ): ZoomKeyframe {
    const isPaused = this.activeRenderContext?.video?.paused ?? true;

    if (segment !== this.lastBakeSegment || viewW !== this.lastBakeViewW || viewH !== this.lastBakeViewH) {
      this.lastBakeSegment = segment;
      this.lastBakeViewW = viewW;
      this.lastBakeViewH = viewH;

      // Only include fields that actually affect the camera path.
      // cursorVisibilitySegments, keystroke*, etc. do NOT affect the baked zoom path.
      // Use lightweight fingerprints instead of mapping entire arrays through JSON.stringify.
      const pathLen = segment.smoothMotionPath?.length ?? 0;
      const pathHash = pathLen > 0
        ? `${pathLen}:${segment.smoothMotionPath![0].time}:${segment.smoothMotionPath![pathLen - 1].time}:${segment.smoothMotionPath![pathLen - 1].zoom}`
        : '0';
      const signature = JSON.stringify({
        trim: [segment.trimStart, segment.trimEnd],
        trimSegments: segment.trimSegments?.map(s => ({ s: s.startTime, e: s.endTime })),
        crop: segment.crop,
        smoothMotionPath: pathHash,
        zoomKeyframes: segment.zoomKeyframes?.map(k => ({ t: k.time, d: k.duration, x: k.positionX, y: k.positionY, z: k.zoomFactor })),
        zoomInfluence: segment.zoomInfluencePoints?.map(p => ({ t: p.time, v: p.value })),
        vidDims: [viewW, viewH]
      });

      if (this.lastBakeSignature !== signature) {
        this.cachedBakedPath = this.generateBakedPath(segment, viewW / (segment.crop?.width || 1), viewH / (segment.crop?.height || 1), 60, srcCropW, srcCropH);
        this.lastBakeSignature = signature;
      }
    }

    if (!isPaused && this.cachedBakedPath && this.cachedBakedPath.length > 0) {
      const timelineDuration = Math.max(
        segment.trimEnd,
        ...(segment.trimSegments || []).map(s => s.endTime)
      );
      const relTime = toCompactTime(currentTime, segment, timelineDuration);
      const step = 1 / 60;
      const idx = Math.floor(relTime / step);

      if (idx >= 0 && idx < this.cachedBakedPath.length) {
        const p1 = this.cachedBakedPath[idx];
        const p2 = this.cachedBakedPath[idx + 1] || p1;
        const t = (relTime % step) / step;

        const globalX = p1.x + (p2.x - p1.x) * t;
        const globalY = p1.y + (p2.y - p1.y) * t;
        const zoom = p1.zoom + (p2.zoom - p1.zoom) * t;

        const crop = segment.crop || { x: 0, y: 0, width: 1, height: 1 };
        const fullW = viewW / crop.width;
        const fullH = viewH / crop.height;
        const cropOffsetX = fullW * crop.x;
        const cropOffsetY = fullH * crop.y;

        const state: ZoomKeyframe = {
          time: currentTime,
          duration: 0,
          zoomFactor: zoom,
          positionX: Math.max(0, Math.min(1, (globalX - cropOffsetX) / viewW)),
          positionY: Math.max(0, Math.min(1, (globalY - cropOffsetY) / viewH)),
          easingType: 'linear'
        };
        this.lastCalculatedState = state;
        return state;
      }
    }

    const state = calculateCurrentZoomStateInternal(currentTime, segment, viewW, viewH, srcCropW, srcCropH);
    this.lastCalculatedState = state;
    return state;
  }

  // ---------------------------------------------------------------------------
  // drawFrame - delegates to extracted drawFrame module
  // ---------------------------------------------------------------------------

  public drawFrame = async (
    context: RenderContext,
    options: RenderOptions = {}
  ): Promise<void> => {
    return drawFrameImpl(context, options, this.state);
  };

  // ---------------------------------------------------------------------------
  // Keystroke overlay public API (delegates to extracted keystrokeRenderer)
  // ---------------------------------------------------------------------------

  public getKeystrokeOverlayConfig(segment: VideoSegment): { x: number; y: number; scale: number } {
    return getKeystrokeOverlayConfig(segment);
  }

  public getKeystrokeOverlayEditBounds(
    segment: VideoSegment,
    canvas: HTMLCanvasElement,
    currentTime: number,
    duration: number
  ): KeystrokeOverlayEditBounds | null {
    return getKeystrokeOverlayEditBounds(this.state.keystrokeState, segment, canvas, currentTime, duration);
  }

  // ---------------------------------------------------------------------------
  // Text drag handlers (delegates to extracted overlayBaker)
  // ---------------------------------------------------------------------------

  public handleMouseDown(e: MouseEvent, segment: VideoSegment, canvas: HTMLCanvasElement): string | null {
    return textMouseDown(e, segment, canvas, this.textDragState);
  }

  public handleMouseMove(
    e: MouseEvent,
    _segment: VideoSegment,
    canvas: HTMLCanvasElement,
    onTextMove: (id: string, x: number, y: number) => void
  ) {
    textMouseMove(e, _segment, canvas, onTextMove, this.textDragState);
  }

  public handleMouseUp() {
    textMouseUp(this.textDragState);
  }

  // ---------------------------------------------------------------------------
  // Overlay baking (delegates to extracted overlayBaker)
  // ---------------------------------------------------------------------------

  public async bakeOverlayAtlasAndPaths(
    segment: VideoSegment,
    outputWidth: number,
    outputHeight: number,
    fps: number = 60
  ): Promise<BakedOverlayPayload> {
    return bakeOverlayAtlasAndPaths(segment, outputWidth, outputHeight, fps, this.state.keystrokeState);
  }
}

export const videoRenderer = new VideoRenderer();
